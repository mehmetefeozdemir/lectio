//! Phase 4b: collections across courses (ADR-026), the Home filter and
//! sort, and the v5 → v6 migration.

use lectio_core::collections::{HomeFilter, SessionSort};
use lectio_core::{CollectionId, LabelKind, Library, ModelError, Recording, SessionId};

/// Two courses, three sessions (created at 10, 20, 30), one with a long recording.
fn library() -> (Library, [SessionId; 3]) {
    let (lib, a) = Library::default().with_course("CSCA67").unwrap();
    let (lib, b) = lib.with_course("MATA31").unwrap();
    let (lib, graphs) = lib.with_session(a, "graphs", 10).unwrap();
    let (lib, limits) = lib.with_session(b, "Limits", 20).unwrap();
    let (lib, trees) = lib.with_session(a, "Trees", 30).unwrap();
    let long = Recording { file_name: "l.wav".into(), duration_ms: 3_000_000, sample_rate: 16_000, cleaned_file: None };
    let lib = lib.with_recording(limits, long).unwrap();
    let (lib, _) = lib.with_note(trees, 0, "midterm topic", &[LabelKind::Important]).unwrap();
    (lib, [graphs, limits, trees])
}

fn titles(lib: &Library, filter: &HomeFilter, sort: SessionSort) -> Vec<String> {
    lib.home_sessions(filter, sort).iter().map(|(_, s)| s.title.clone()).collect()
}

#[test]
fn a_collection_groups_sessions_from_different_courses() {
    let (lib, [graphs, limits, _]) = library();

    let (lib, midterm) = lib.with_collection("Midterm 1").unwrap();
    let lib = lib.with_session_in_collection(midterm, graphs).unwrap();
    let lib = lib.with_session_in_collection(midterm, limits).unwrap();
    let lib = lib.with_session_in_collection(midterm, limits).unwrap(); // twice: still once

    let collection = &lib.collections()[0];
    assert_eq!(collection.name, "Midterm 1");
    assert_eq!(collection.sessions, vec![graphs, limits], "kept in the order added, no duplicates");
    assert_eq!(lib.collections_of(limits).len(), 1);
}

#[test]
fn collections_can_be_renamed_emptied_and_deleted() {
    let (lib, [graphs, ..]) = library();
    let (lib, id) = lib.with_collection("  Exam  ").unwrap();
    let lib = lib.with_session_in_collection(id, graphs).unwrap();

    let lib = lib.with_collection_renamed(id, "Final exam").unwrap();
    assert_eq!(lib.collections()[0].name, "Final exam");
    let lib = lib.without_session_in_collection(id, graphs).unwrap();
    assert!(lib.collections()[0].sessions.is_empty());
    let lib = lib.without_collection(id).unwrap();
    assert!(lib.collections().is_empty());
}

#[test]
fn collection_names_and_ids_are_checked() {
    let (lib, [graphs, ..]) = library();
    assert_eq!(lib.with_collection("   ").unwrap_err(), ModelError::EmptyName);
    let missing = CollectionId::new_random();
    assert_eq!(lib.with_session_in_collection(missing, graphs).unwrap_err(), ModelError::CollectionNotFound);
    let (lib, id) = lib.with_collection("C").unwrap();
    assert_eq!(lib.with_session_in_collection(id, SessionId::new_random()).unwrap_err(), ModelError::SessionNotFound);
}

#[test]
fn trashed_sessions_are_hidden_and_expired_ones_leave_their_collections() {
    let (lib, [graphs, limits, _]) = library();
    let (lib, id) = lib.with_collection("Midterm").unwrap();
    let lib = lib.with_session_in_collection(id, graphs).unwrap().with_session_in_collection(id, limits).unwrap();

    let lib = lib.with_session_trashed(graphs, 1_000).unwrap();
    assert_eq!(titles(&lib, &HomeFilter::Collection(id), SessionSort::Oldest), vec!["Limits"]);
    assert_eq!(lib.collection_sessions(id), vec![limits]);
    assert_eq!(lib.collections()[0].sessions.len(), 2, "restorable: still listed while in the trash");

    let (lib, removed) = lib.without_expired_trash(1_000 + 31 * 86_400_000);
    assert_eq!(removed.len(), 1);
    assert_eq!(lib.collections()[0].sessions, vec![limits]);
}

#[test]
fn home_sorts_by_date_title_or_length() {
    let (lib, _) = library();
    let all = HomeFilter::All;
    assert_eq!(titles(&lib, &all, SessionSort::Newest), vec!["Trees", "Limits", "graphs"]);
    assert_eq!(titles(&lib, &all, SessionSort::Oldest), vec!["graphs", "Limits", "Trees"]);
    assert_eq!(titles(&lib, &all, SessionSort::Title), vec!["graphs", "Limits", "Trees"], "ignores case");
    assert_eq!(titles(&lib, &all, SessionSort::Longest), vec!["Limits", "Trees", "graphs"], "then newest first");
}

#[test]
fn home_filters_by_course_collection_or_label() {
    let (lib, [graphs, ..]) = library();
    let course = lib.course_of(graphs).unwrap().id;
    let (lib, id) = lib.with_collection("Only graphs").unwrap();
    let lib = lib.with_session_in_collection(id, graphs).unwrap();

    assert_eq!(titles(&lib, &HomeFilter::Course(course), SessionSort::Newest), vec!["Trees", "graphs"]);
    assert_eq!(titles(&lib, &HomeFilter::Collection(id), SessionSort::Newest), vec!["graphs"]);
    assert_eq!(titles(&lib, &HomeFilter::Label(LabelKind::Important), SessionSort::Newest), vec!["Trees"]);
    let gone = HomeFilter::Collection(CollectionId::new_random());
    assert!(titles(&lib, &gone, SessionSort::Newest).is_empty(), "a deleted collection shows nothing, no panic");
}

#[test]
fn the_home_filter_and_sort_are_saved_with_the_preferences() {
    let (lib, [graphs, ..]) = library();
    let (lib, id) = lib.with_collection("C").unwrap();
    let lib = lib.with_session_in_collection(id, graphs).unwrap();
    let mut prefs = lib.preferences().clone();
    prefs.home_filter = HomeFilter::Collection(id);
    prefs.home_sort = SessionSort::Title;

    let reloaded = Library::from_json(&lib.with_preferences(prefs).to_json().unwrap()).unwrap();

    assert_eq!(reloaded.preferences().home_filter, HomeFilter::Collection(id));
    assert_eq!(reloaded.preferences().home_sort, SessionSort::Title);
    assert_eq!(reloaded.collections()[0].sessions, vec![graphs]);
}

#[test]
fn a_version_5_library_loads_with_no_collections_ink_or_formulas() {
    let v5 = r#"{
      "schema_version": 5,
      "courses": [{ "id": "6f1c1d6e-0000-4000-8000-000000000001", "name": "C", "sessions": [{
        "id": "6f1c1d6e-0000-4000-8000-000000000002", "title": "L1", "created_unix_ms": 5, "recording": null,
        "notes": [], "decks": [{ "id": "6f1c1d6e-0000-4000-8000-000000000004", "title": "W1",
          "blob": "6f1c1d6e-0000-4000-8000-000000000005.pdf", "page_count": 3 }] }]}],
      "labels": { "labels": [] },
      "preferences": { "speed_percent": 125 }
    }"#;

    let lib = Library::from_json(v5).unwrap();

    assert!(lib.collections().is_empty());
    let (_, session) = lib.live_sessions()[0];
    assert!(session.decks[0].ink.is_empty());
    assert_eq!(lib.preferences().home_filter, HomeFilter::All);
    assert_eq!(lib.preferences().home_sort, SessionSort::Newest);
    assert_eq!(lib.preferences().speed_percent, 125);
    assert!(Library::default().to_json().unwrap().contains("\"schema_version\": 6"));
}
