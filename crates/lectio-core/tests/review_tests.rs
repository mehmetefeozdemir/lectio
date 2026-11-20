//! Phase 4a: flashcards with spaced repetition (SM-2), tasks across courses,
//! the focus timer, the offline dictionary index, AI settings, and the
//! v4 → v5 migration.

use lectio_core::cards::{Grade, Review, review};
use lectio_core::dictionary::{DictIndex, Sense, base_forms, index_from_jsonl, parse_oewn_line, to_tsv};
use lectio_core::focus::{FocusEntry, FocusTimer};
use lectio_core::prefs::AiProvider;
use lectio_core::sources::Citation;
use lectio_core::tasks::{TaskFilter, tasks};
use lectio_core::{LabelKind, Library, SCHEMA_VERSION, ModelError, SessionId};

const DAY: u64 = 86_400_000;

fn with_definitions() -> (Library, SessionId) {
    let (lib, course) = Library::default().with_course("CSCA67").unwrap();
    let (lib, s) = lib.with_session(course, "Graphs", 0).unwrap();
    let (lib, _) = lib.with_note(s, 0, "BFS: visits by distance", &[LabelKind::Definition]).unwrap();
    let (lib, _) = lib.with_note(s, 0, "Queue: first in, first out", &[LabelKind::Definition]).unwrap();
    let (lib, _) = lib.with_note(s, 0, "not a definition", &[]).unwrap();
    (lib, s)
}

#[test]
fn sm2_grows_the_interval_after_each_good_answer() {
    let fresh = Review::new(0);
    let first = review(fresh, Grade::Good, 0);
    let second = review(first, Grade::Good, first.due_unix_ms);
    let third = review(second, Grade::Good, second.due_unix_ms);

    assert_eq!((first.interval_days, first.repetitions), (1, 1));
    assert_eq!(second.interval_days, 6);
    assert_eq!(third.interval_days, 15, "6 × 2.5");
    assert_eq!(first.due_unix_ms, DAY);
    assert!((third.ease - 2.5).abs() < 1e-6, "Good keeps the ease");
}

#[test]
fn again_resets_the_card_and_lowers_the_ease_but_not_below_1_3() {
    let mut r = review(review(Review::new(0), Grade::Good, 0), Grade::Good, DAY);
    r = review(r, Grade::Again, 10 * DAY);
    assert_eq!((r.repetitions, r.interval_days, r.lapses), (0, 1, 1));
    assert_eq!(r.due_unix_ms, 11 * DAY);
    for _ in 0..20 {
        r = review(r, Grade::Again, 0);
    }
    assert!((r.ease - 1.3).abs() < 1e-6);
}

#[test]
fn easy_and_hard_move_the_ease_in_opposite_directions() {
    let easy = review(Review::new(0), Grade::Easy, 0);
    let hard = review(Review::new(0), Grade::Hard, 0);
    assert!(easy.ease > 2.5 && hard.ease < 2.5);
}

#[test]
fn definition_notes_become_cards_once() {
    let (lib, s) = with_definitions();

    let (lib, added) = lib.with_definition_cards(s, 0).unwrap();
    let (lib, again) = lib.with_definition_cards(s, 0).unwrap();

    assert_eq!((added, again), (2, 0));
    let fronts: Vec<&str> = lib.cards().iter().map(|c| c.front.as_str()).collect();
    assert_eq!(fronts, vec!["BFS", "Queue"]);
    assert_eq!(lib.cards()[0].back, "visits by distance");
    assert_eq!(lib.due_cards(0).len(), 2, "new cards are due at once");
}

#[test]
fn reviewing_a_card_schedules_it_and_due_counts_follow() {
    let (lib, s) = with_definitions();
    let (lib, _) = lib.with_definition_cards(s, 0).unwrap();
    let card = lib.cards()[0].id;

    let lib = lib.with_card_reviewed(card, Grade::Good, 0).unwrap();

    assert_eq!(lib.due_cards(0).len(), 1);
    assert_eq!(lib.due_cards(DAY).len(), 2);
    assert!(lib.without_card(card).unwrap().cards().iter().all(|c| c.id != card));
}

#[test]
fn generated_cards_keep_their_citations_and_trashed_sessions_are_not_due() {
    let (lib, s) = with_definitions();
    let cite = Citation { offset_ms: Some(5_000), slide: None };
    let lib = lib.with_generated_cards(s, vec![("Stack".into(), "last in, first out".into(), vec![cite])], 0).unwrap();
    assert_eq!(lib.cards()[0].cites, vec![cite]);
    assert!(lib.with_generated_cards(s, vec![("".into(), "x".into(), vec![])], 0).is_err(), "empty side");

    let trashed = lib.with_session_trashed(s, 0).unwrap();
    assert!(trashed.due_cards(0).is_empty());
}

#[test]
fn tasks_are_listed_across_courses_undone_first_then_by_due_date() {
    let (lib, a) = Library::default().with_course("A").unwrap();
    let (lib, b) = lib.with_course("B").unwrap();
    let (lib, sa) = lib.with_session(a, "La", 10).unwrap();
    let (lib, sb) = lib.with_session(b, "Lb", 20).unwrap();
    let (lib, t1) = lib.with_note(sa, 1_000, "read chapter 3", &[LabelKind::Task]).unwrap();
    let (lib, t2) = lib.with_note(sb, 2_000, "problem set", &[LabelKind::Task]).unwrap();
    let (lib, t3) = lib.with_note(sb, 3_000, "email TA", &[LabelKind::Task]).unwrap();
    let (lib, _) = lib.with_note(sb, 4_000, "not a task", &[]).unwrap();
    let lib = lib.with_task_due(sb, t2, Some(5 * DAY)).unwrap();
    let lib = lib.with_task_due(sa, t1, Some(2 * DAY)).unwrap();
    let lib = lib.with_task_done(sb, t3, true).unwrap();

    let all: Vec<&str> = tasks(&lib, &TaskFilter { course: None, include_done: true }).iter().map(|r| r.note.body.as_str()).collect();
    assert_eq!(all, vec!["read chapter 3", "problem set", "email TA"]);
    let open_b: Vec<&str> =
        tasks(&lib, &TaskFilter { course: Some(b), include_done: false }).iter().map(|r| r.note.body.as_str()).collect();
    assert_eq!(open_b, vec!["problem set"]);
}

#[test]
fn only_task_notes_take_done_and_due() {
    let (lib, s) = with_definitions();
    let note = lib.session(s).unwrap().notes[0].id;
    assert_eq!(lib.with_task_done(s, note, true), Err(ModelError::NotATask));
}

#[test]
fn the_focus_timer_counts_down_pauses_and_finishes() {
    let timer = FocusTimer::new(25 * 60_000).start(1_000);
    assert_eq!(timer.remaining_ms(61_000), 24 * 60_000);
    let paused = timer.pause(61_000);
    assert_eq!(paused.remaining_ms(999_999), 24 * 60_000, "time stops while paused");
    let resumed = paused.resume(100_000);
    assert_eq!(resumed.elapsed_ms(160_000), 2 * 60_000);
    assert!(resumed.is_done(100_000 + 24 * 60_000));
    assert_eq!(resumed.remaining_ms(10_000_000), 0);
}

#[test]
fn focus_minutes_are_logged_per_course() {
    let (lib, course) = Library::default().with_course("C").unwrap();
    let lib = lib.with_focus_entry(FocusEntry { course, started_unix_ms: 10 * DAY, minutes: 25 }).unwrap();
    let lib = lib.with_focus_entry(FocusEntry { course, started_unix_ms: 2 * DAY, minutes: 50 }).unwrap();

    assert_eq!(lib.focus_minutes(course, 0), 75);
    assert_eq!(lib.focus_minutes(course, 7 * DAY), 25, "only since the given time");
    let bad = FocusEntry { course, started_unix_ms: 0, minutes: 0 };
    assert!(lib.with_focus_entry(bad).is_err());
}

#[test]
fn wordnet_lines_become_dictionary_entries() {
    let line = r#"{"@id": "oewn-03159654-a", "@members": "oewn-calibrated-a oewn-ice_cream-n oewn-o-ap-clock-r", "@partOfSpeech": "a", "Definition": "marked with or divided into degrees", "Example": "a calibrated thermometer"}"#;

    let entries = parse_oewn_line(line);

    let words: Vec<&str> = entries.iter().map(|(w, _)| w.as_str()).collect();
    assert_eq!(words, vec!["calibrated", "ice cream", "o'clock"]);
    assert_eq!(entries[0].1, Sense { pos: "adjective".into(), definition: "marked with or divided into degrees".into(), example: "a calibrated thermometer".into() });
    assert!(parse_oewn_line("not json").is_empty());
}

#[test]
fn a_whole_download_becomes_one_index_skipping_bad_lines() {
    let jsonl = concat!(
        r#"{"@members": "oewn-graph-n", "@partOfSpeech": "n", "Definition": "a diagram of nodes and edges"}"#,
        "\nnot json\n",
        r#"{"@members": "oewn-traverse-v", "@partOfSpeech": "v", "Definition": "travel across"}"#,
    );

    let index = DictIndex::from_tsv(&index_from_jsonl(jsonl));

    assert_eq!(index.len(), 2);
    assert_eq!(index.lookup("graphs")[0].definition, "a diagram of nodes and edges");
}

#[test]
fn lookups_find_base_forms_through_the_compact_index() {
    let sense = |d: &str| Sense { pos: "noun".into(), definition: d.into(), example: String::new() };
    let tsv = to_tsv(&[("queue".into(), sense("a line of people")), ("vertex".into(), sense("a corner point")), ("study".into(), sense("learning"))]);
    let index = DictIndex::from_tsv(&tsv);

    assert_eq!(index.lookup("Queues")[0].definition, "a line of people");
    assert_eq!(index.lookup("studies")[0].definition, "learning");
    assert_eq!(index.lookup("queued").len(), 1);
    assert!(index.lookup("zebra").is_empty());
    assert!(base_forms("studies").contains(&"study".to_owned()));
}

#[test]
fn ai_settings_never_store_a_key_and_hide_details_in_debug_output() {
    let provider = AiProvider::OpenAi { base_url: "https://example.org/v1".into(), model: "m".into() };
    let json = serde_json::to_string(&provider).unwrap();
    assert!(!json.to_lowercase().contains("key"));
    assert_eq!(AiProvider::default(), AiProvider::None);
}

#[test]
fn a_version_4_library_loads_with_no_study_data() {
    let v4 = r#"{
      "schema_version": 4,
      "courses": [{ "id": "6f1c1d6e-0000-4000-8000-000000000001", "name": "C", "sessions": [{
        "id": "6f1c1d6e-0000-4000-8000-000000000002", "title": "L1", "created_unix_ms": 5, "recording": null,
        "notes": [{ "id": "6f1c1d6e-0000-4000-8000-000000000003", "offset_ms": 0, "body": "hw", "labels": ["task"] }] }]}],
      "labels": { "labels": [] },
      "preferences": { "speed_percent": 150 }
    }"#;

    let lib = Library::from_json(v4).unwrap();

    let (_, session) = lib.live_sessions()[0];
    assert!(session.outline.is_none());
    assert!(session.quiz_attempts.is_empty());
    assert_eq!(session.notes[0].task, None, "a task is open until marked done");
    assert!(lib.cards().is_empty());
    assert_eq!(lib.preferences().speed_percent, 150);
    assert_eq!(lib.preferences().ai, AiProvider::None);
    // Version 6 is written since Phase 4b (organize_tests); a newer file is still refused.
    let newer = format!(r#"{{"schema_version": {}, "courses": [], "labels": {{"labels": []}}}}"#, SCHEMA_VERSION + 1);
    let refused = Library::from_json(&newer);
    assert!(matches!(refused, Err(ModelError::UnsupportedSchema { found, supported }) if found == SCHEMA_VERSION + 1 && supported == SCHEMA_VERSION));
}

#[test]
fn the_online_provider_builds_its_url_and_reads_its_answer() {
    use lectio_core::dictionary::{DefinitionProvider, FreeDictionary};
    let provider = FreeDictionary;
    assert_eq!(provider.url("Queue's"), "https://api.dictionaryapi.dev/api/v2/entries/en/queue");
    let body = r#"[{"word":"queue","meanings":[{"partOfSpeech":"noun","definitions":[{"definition":"A line of people.","example":"a queue at the bank"}]},{"partOfSpeech":"verb","definitions":[{"definition":"To wait in line."}]}]}]"#;

    let senses = provider.parse(body);

    assert_eq!(senses.len(), 2);
    assert_eq!(senses[0], Sense { pos: "noun".into(), definition: "A line of people.".into(), example: "a queue at the bank".into() });
    assert_eq!(senses[1].pos, "verb");
    assert!(provider.parse("{\"title\":\"No Definitions Found\"}").is_empty());
}
