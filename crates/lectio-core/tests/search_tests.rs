//! Phase 3: search across notes, headings, transcripts, slide titles and
//! slide text, with ranking and snippets.

use lectio_core::material::{BlobKey, MediaKind, SlideLink};
use lectio_core::search::{HitKind, fold, search, search_deck};
use lectio_core::transcript::{Segment, Transcript};
use lectio_core::{LabelKind, Library, Recording, SessionId};

struct Fixture {
    lib: Library,
    graphs: SessionId,
    limits: SessionId,
}

fn fixture() -> Fixture {
    let (lib, course) = Library::default().with_course("CSCA67").unwrap();
    let (lib, graphs) = lib.with_session(course, "Graph traversal", 1_000).unwrap();
    let (lib, limits) = lib.with_session(course, "Limits", 2_000).unwrap();
    let recording = Recording { file_name: "g.wav".into(), duration_ms: 600_000, sample_rate: 16_000, cleaned_file: None };
    let lib = lib.with_recording(graphs, recording).unwrap();
    let (lib, _) = lib.with_note(graphs, 1_000, "Breadth-first search", &[LabelKind::Heading]).unwrap();
    let (lib, _) = lib.with_note(graphs, 5_000, "BFS uses a queue", &[]).unwrap();
    let (lib, deck) = lib.with_deck(graphs, "Week 3.pdf", BlobKey::new_random(MediaKind::Pdf), 2).unwrap();
    let lib = lib
        .with_slide_text(graphs, deck, vec!["Graphs: vertices and edges".into(), "Queue, distance order".into()])
        .unwrap();
    let (lib, item) = lib.with_slide_item(graphs, 9_000, SlideLink { deck, page: 2 }).unwrap();
    let lib = lib.with_note_text(graphs, item, "Queue example").unwrap();
    let transcript = Transcript {
        language: "en".into(),
        model: "tiny".into(),
        segments: vec![Segment { start_ms: 20_000, end_ms: 24_000, text: "So the queue holds the frontier.".into() }],
        complete: true,
    };
    let lib = lib.with_transcript(graphs, transcript).unwrap();
    let (lib, _) = lib.with_note(limits, 0, "Résumé of epsilon-delta", &[]).unwrap();
    Fixture { lib, graphs, limits }
}

#[test]
fn every_kind_of_text_is_found() {
    let f = fixture();

    let kinds: Vec<HitKind> = search(&f.lib, "queue", 20).iter().map(|h| h.kind).collect();

    assert!(kinds.contains(&HitKind::Note));
    assert!(kinds.contains(&HitKind::SlideTitle));
    assert!(kinds.contains(&HitKind::SlideText));
    assert!(kinds.contains(&HitKind::Transcript));
}

#[test]
fn hits_carry_where_to_jump() {
    let f = fixture();
    let hits = search(&f.lib, "frontier", 20);

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].session, f.graphs);
    assert_eq!(hits[0].offset_ms, Some(20_000));

    let slide = search(&f.lib, "distance", 20);
    assert_eq!(slide[0].kind, HitKind::SlideText);
    assert_eq!(slide[0].slide.map(|s| s.page), Some(2));
}

#[test]
fn headings_rank_above_notes_and_notes_above_transcripts() {
    let (lib, course) = Library::default().with_course("C").unwrap();
    let (lib, s) = lib.with_session(course, "S", 0).unwrap();
    let recording = Recording { file_name: "s.wav".into(), duration_ms: 60_000, sample_rate: 16_000, cleaned_file: None };
    let lib = lib.with_recording(s, recording).unwrap();
    let (lib, _) = lib.with_note(s, 0, "stack", &[]).unwrap();
    let (lib, _) = lib.with_note(s, 1_000, "stack", &[LabelKind::Heading]).unwrap();
    let segments = vec![Segment { start_ms: 0, end_ms: 1_000, text: "stack".into() }];
    let lib = lib.with_transcript(s, Transcript { language: "en".into(), model: "tiny".into(), segments, complete: true }).unwrap();

    let kinds: Vec<HitKind> = search(&lib, "stack", 10).iter().map(|h| h.kind).collect();

    assert_eq!(kinds, vec![HitKind::Heading, HitKind::Note, HitKind::Transcript]);
}

#[test]
fn every_word_must_match_and_the_last_word_may_be_a_prefix() {
    let f = fixture();

    assert_eq!(search(&f.lib, "bfs queue", 20).len(), 1, "only the note has both words");
    assert!(search(&f.lib, "bfs stack", 20).is_empty());
    assert!(!search(&f.lib, "que", 20).is_empty(), "prefix of the last word");
    assert!(search(&f.lib, "que bfs", 20).is_empty(), "only the last word is a prefix");
}

#[test]
fn matching_ignores_case_and_accents() {
    let f = fixture();

    let hits = search(&f.lib, "RESUME", 20);

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].session, f.limits);
    assert_eq!(fold("Résumé ÇİĞ"), "resume cig");
}

#[test]
fn a_whole_phrase_ranks_above_scattered_words() {
    let (lib, course) = Library::default().with_course("C").unwrap();
    let (lib, s) = lib.with_session(course, "S", 0).unwrap();
    let recording = Recording { file_name: "s.wav".into(), duration_ms: 60_000, sample_rate: 16_000, cleaned_file: None };
    let lib = lib.with_recording(s, recording).unwrap();
    let (lib, _) = lib.with_note(s, 0, "order of the queue and the distance", &[]).unwrap();
    let (lib, _) = lib.with_note(s, 1_000, "queue distance", &[]).unwrap();

    let hits = search(&lib, "queue distance", 10);

    assert_eq!(hits[0].offset_ms, Some(1_000));
}

#[test]
fn trashed_sessions_are_not_searched() {
    let f = fixture();
    let lib = f.lib.with_session_trashed(f.limits, 0).unwrap();

    assert!(search(&lib, "resume", 20).is_empty());
}

#[test]
fn snippets_show_the_match_in_context() {
    let (lib, course) = Library::default().with_course("C").unwrap();
    let (lib, s) = lib.with_session(course, "S", 0).unwrap();
    let recording = Recording { file_name: "s.wav".into(), duration_ms: 60_000, sample_rate: 16_000, cleaned_file: None };
    let lib = lib.with_recording(s, recording).unwrap();
    let long = format!("{} the key idea is amortised analysis {}", "word ".repeat(40), "tail ".repeat(40));
    let (lib, _) = lib.with_note(s, 0, &long, &[]).unwrap();

    let snippet = &search(&lib, "amortised", 10)[0].snippet;

    assert!(snippet.contains("amortised analysis"));
    assert!(snippet.starts_with('…') && snippet.ends_with('…'));
    assert!(snippet.chars().count() <= 100);
}

#[test]
fn an_empty_query_finds_nothing_and_results_are_limited() {
    let f = fixture();

    assert!(search(&f.lib, "  ", 20).is_empty());
    assert_eq!(search(&f.lib, "queue", 2).len(), 2);
}

#[test]
fn the_slide_search_box_lists_matching_pages() {
    let f = fixture();
    let deck = &f.lib.session(f.graphs).unwrap().decks[0];

    assert_eq!(search_deck(deck, "queue"), vec![2]);
    assert_eq!(search_deck(deck, "graphs edges"), vec![1]);
    assert!(search_deck(deck, "calculus").is_empty());
}

#[test]
fn slide_text_must_have_one_entry_per_page() {
    let f = fixture();
    let deck = f.lib.session(f.graphs).unwrap().decks[0].id;

    assert!(f.lib.with_slide_text(f.graphs, deck, vec!["only one".into()]).is_err());
}

#[test]
fn snippets_start_at_the_beginning_of_a_word() {
    let (lib, course) = Library::default().with_course("C").unwrap();
    let (lib, s) = lib.with_session(course, "S", 0).unwrap();
    let recording = Recording { file_name: "s.wav".into(), duration_ms: 60_000, sample_rate: 16_000, cleaned_file: None };
    let lib = lib.with_recording(s, recording).unwrap();
    let text = "Today we look at GraphTraversal, a graph is a set of vertices and a set of edges.";
    let (lib, _) = lib.with_note(s, 0, text, &[]).unwrap();

    let snippet = &search(&lib, "graph", 10)[0].snippet;

    assert!(snippet.starts_with("Today"), "{snippet}");
}
