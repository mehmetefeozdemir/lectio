//! Session workflow model (ADR-015): Heading label, slide items in the feed,
//! audio-stream sections with label tints, trash with restore and expiry,
//! saved layout preferences, Markdown export, and schema v2 → v3 migration.

use lectio_core::export::session_markdown;
use lectio_core::material::{BlobKey, MediaKind, SlideLink};
use lectio_core::prefs::{Preferences, ThemeChoice};
use lectio_core::stream::{SECTION_MS, Section, audio_sections, section_at, section_labels};
use lectio_core::trash::TRASH_DAYS;
use lectio_core::{LabelKind, Library, ModelError, NoteKind, Recording, SessionId};

const DAY_MS: u64 = 86_400_000;

fn library_with_deck() -> (Library, SessionId, lectio_core::DeckId) {
    let (lib, course) = Library::default().with_course("CSCA67").unwrap();
    let (lib, session) = lib.with_session(course, "Graphs", 0).unwrap();
    let (lib, deck) = lib.with_deck(session, "Week 3.pdf", BlobKey::new_random(MediaKind::Pdf), 5).unwrap();
    (lib, session, deck)
}

// ---------- Heading label ----------

#[test]
fn heading_is_a_fifth_label_with_its_own_name() {
    let lib = Library::default();
    assert_eq!(LabelKind::ALL.len(), 5);
    assert_eq!(lib.labels().name(LabelKind::Heading), "Heading");
}

#[test]
fn a_heading_note_is_marked_by_its_label() {
    let (lib, session, _) = library_with_deck();
    let (lib, note) = lib.with_note(session, 1_000, "Breadth-first search", &[LabelKind::Heading]).unwrap();
    assert!(lib.session(session).unwrap().note(note).unwrap().is_heading());
}

// ---------- slide items in the feed ----------

#[test]
fn posting_a_slide_adds_a_slide_item_at_that_moment() {
    let (lib, session, deck) = library_with_deck();
    let (lib, item) = lib.with_slide_item(session, 4_000, SlideLink { deck, page: 2 }).unwrap();

    let note = lib.session(session).unwrap().note(item).unwrap();
    assert_eq!(note.kind, NoteKind::Slide);
    assert_eq!(note.slide, Some(SlideLink { deck, page: 2 }));
    assert_eq!(note.body, "Slide 2");
    assert_eq!(note.offset_ms, 4_000);
}

#[test]
fn a_slide_item_needs_a_real_page() {
    let (lib, session, deck) = library_with_deck();
    let err = lib.with_slide_item(session, 0, SlideLink { deck, page: 9 }).unwrap_err();
    assert!(matches!(err, ModelError::PageOutOfRange { .. }));
}

#[test]
fn a_slide_item_title_can_be_edited_but_not_emptied() {
    let (lib, session, deck) = library_with_deck();
    let (lib, item) = lib.with_slide_item(session, 0, SlideLink { deck, page: 1 }).unwrap();
    let lib = lib.with_note_text(session, item, " Intro to graphs ").unwrap();
    assert_eq!(lib.session(session).unwrap().note(item).unwrap().body, "Intro to graphs");
    // An empty title falls back to the page name instead of vanishing.
    let lib = lib.with_note_text(session, item, "  ").unwrap();
    assert_eq!(lib.session(session).unwrap().note(item).unwrap().body, "Slide 1");
}

#[test]
fn slide_items_take_their_place_in_time_order() {
    let (lib, session, deck) = library_with_deck();
    let (lib, _) = lib.with_note(session, 5_000, "later note", &[]).unwrap();
    let (lib, _) = lib.with_slide_item(session, 0, SlideLink { deck, page: 1 }).unwrap();
    let kinds: Vec<NoteKind> = lib.session(session).unwrap().notes.iter().map(|n| n.kind).collect();
    assert_eq!(kinds, [NoteKind::Slide, NoteKind::Text]);
}

// ---------- audio stream sections ----------

#[test]
fn a_recording_is_split_into_ten_second_sections() {
    let sections = audio_sections(25_000);
    assert_eq!(SECTION_MS, 10_000);
    assert_eq!(
        sections,
        [
            Section { start_ms: 0, end_ms: 10_000 },
            Section { start_ms: 10_000, end_ms: 20_000 },
            Section { start_ms: 20_000, end_ms: 25_000 }
        ]
    );
    assert!(audio_sections(0).is_empty());
}

#[test]
fn the_playing_section_is_found_by_time() {
    let sections = audio_sections(25_000);
    assert_eq!(section_at(&sections, 0), Some(0));
    assert_eq!(section_at(&sections, 19_999), Some(1));
    assert_eq!(section_at(&sections, 25_000), Some(2), "the very end belongs to the last section");
    assert_eq!(section_at(&[], 5), None);
}

#[test]
fn sections_take_the_colour_of_a_label_in_their_time_range() {
    let (lib, session, _) = library_with_deck();
    let (lib, _) = lib.with_note(session, 2_000, "", &[LabelKind::Important]).unwrap();
    let (lib, _) = lib.with_note(session, 12_000, "plain", &[]).unwrap();
    let (lib, _) = lib.with_note(session, 23_000, "", &[LabelKind::Task]).unwrap();
    let (lib, _) = lib.with_note(session, 24_000, "", &[LabelKind::Review]).unwrap();
    let sections = audio_sections(25_000);

    let tints = section_labels(&sections, &lib.session(session).unwrap().notes);

    assert_eq!(tints, [Some(LabelKind::Important), None, Some(LabelKind::Task)], "first label in the range wins");
}

#[test]
fn headings_do_not_tint_the_audio() {
    let (lib, session, _) = library_with_deck();
    let (lib, _) = lib.with_note(session, 1_000, "Part 2", &[LabelKind::Heading]).unwrap();
    let tints = section_labels(&audio_sections(10_000), &lib.session(session).unwrap().notes);
    assert_eq!(tints, [None]);
}

// ---------- trash ----------

#[test]
fn a_trashed_session_leaves_the_list_and_can_be_restored() {
    let (lib, session, _) = library_with_deck();
    let trashed = lib.with_session_trashed(session, 1_000).unwrap();

    assert!(trashed.live_sessions().is_empty());
    assert_eq!(trashed.trashed_sessions().len(), 1);
    let restored = trashed.with_session_restored(session).unwrap();
    assert_eq!(restored.live_sessions().len(), 1);
    assert!(restored.trashed_sessions().is_empty());
}

#[test]
fn trash_is_emptied_after_thirty_days() {
    let (lib, session, _) = library_with_deck();
    let lib = lib.with_session_trashed(session, 0).unwrap();
    assert_eq!(TRASH_DAYS, 30);

    let (kept, purged) = lib.without_expired_trash(TRASH_DAYS * DAY_MS - 1);
    assert!(purged.is_empty());
    assert_eq!(kept, lib);

    let (emptied, purged) = lib.without_expired_trash(TRASH_DAYS * DAY_MS);
    assert_eq!(purged.len(), 1);
    assert!(emptied.session(session).is_none());
    assert!(emptied.blob_keys().is_empty(), "the deck's PDF becomes an orphan the app deletes");
}

#[test]
fn live_sessions_are_listed_newest_first() {
    let (lib, course) = Library::default().with_course("C").unwrap();
    let (lib, _) = lib.with_session(course, "old", 1_000).unwrap();
    let (lib, _) = lib.with_session(course, "new", 9_000).unwrap();
    let titles: Vec<&str> = lib.live_sessions().iter().map(|(_, s)| s.title.as_str()).collect();
    assert_eq!(titles, ["new", "old"]);
}

// ---------- preferences ----------

#[test]
fn layout_and_theme_preferences_are_saved_with_the_library() {
    let defaults = Preferences::default();
    assert!(defaults.show_slides && defaults.show_feed && defaults.show_audio);
    let prefs = Preferences { show_slides: false, theme: ThemeChoice::Dark, ..defaults };
    let lib = Library::default().with_preferences(prefs.clone());

    let back = Library::from_json(&lib.to_json().unwrap()).unwrap();

    assert_eq!(back.preferences(), &prefs);
}

// ---------- Markdown export ----------

#[test]
fn markdown_export_has_headings_slides_notes_and_labels() {
    let (lib, session, deck) = library_with_deck();
    let recording = Recording { file_name: "a.wav".into(), duration_ms: 60_000, sample_rate: 16_000, cleaned_file: None };
    let lib = lib.with_recording(session, recording).unwrap();
    let (lib, _) = lib.with_note(session, 0, "Introduction", &[LabelKind::Heading]).unwrap();
    let (lib, _) = lib.with_slide_item(session, 1_000, SlideLink { deck, page: 1 }).unwrap();
    let (lib, _) = lib.with_note(session, 59_000, "BFS uses a queue", &[LabelKind::Important]).unwrap();

    let md = session_markdown(&lib, session).unwrap();

    assert!(md.starts_with("# Graphs\n"), "{md}");
    assert!(md.contains("## Introduction"), "{md}");
    assert!(md.contains("**Slide 1** (Week 3, page 1) · 0:01"), "{md}");
    assert!(md.contains("- 0:59 BFS uses a queue *(Important)*"), "{md}");
}

// ---------- schema v2 → v3 ----------

#[test]
fn a_version_2_library_loads_with_defaults_for_everything_new() {
    let v2 = r#"{
      "schema_version": 2,
      "courses": [{ "id": "6f1c1d6e-0000-4000-8000-000000000001", "name": "CSCA67", "sessions": [{
        "id": "6f1c1d6e-0000-4000-8000-000000000002", "title": "L1", "created_unix_ms": 5,
        "recording": null, "decks": [],
        "notes": [{ "id": "6f1c1d6e-0000-4000-8000-000000000003", "offset_ms": 10, "body": "old", "labels": ["task"] }]
      }]}],
      "labels": { "labels": [
        { "kind": "important", "name": "Important" }, { "kind": "review", "name": "Review" },
        { "kind": "task", "name": "To do" }, { "kind": "definition", "name": "Definition" } ] }
    }"#;

    let lib = Library::from_json(v2).unwrap();

    let (_, session) = lib.live_sessions()[0];
    assert_eq!(session.notes[0].kind, NoteKind::Text);
    assert_eq!(session.deleted_unix_ms, None);
    assert_eq!(lib.labels().name(LabelKind::Task), "To do", "renamed labels survive");
    assert_eq!(lib.labels().name(LabelKind::Heading), "Heading", "the new label gets its default name");
    assert_eq!(lib.preferences(), &Preferences::default());
}

