//! Data-model tests: courses, sessions, notes and labels.

use lectio_core::{LabelKind, Library, ModelError, Recording, SessionId};

fn library_with_session() -> (Library, SessionId) {
    let (lib, course) = Library::default().with_course("CSCA67").unwrap();
    let (lib, session) = lib.with_session(course, "Lecture 1", 1_000).unwrap();
    (lib, session)
}

#[test]
fn new_library_has_four_default_labels_and_no_courses() {
    let lib = Library::default();

    assert!(lib.courses().is_empty());
    let names: Vec<&str> = lib.labels().iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Important", "Review", "Task", "Definition"]);
}

#[test]
fn adding_a_course_returns_a_new_library_and_leaves_the_old_one_unchanged() {
    let original = Library::default();

    let (updated, id) = original.with_course("  CSCA67 ").unwrap();

    assert!(original.courses().is_empty());
    assert_eq!(updated.courses().len(), 1);
    assert_eq!(updated.course(id).unwrap().name, "CSCA67");
}

#[test]
fn course_name_must_not_be_blank() {
    let err = Library::default().with_course("   ").unwrap_err();
    assert_eq!(err, ModelError::EmptyName);
}

#[test]
fn course_name_has_a_length_limit() {
    let long = "x".repeat(200);
    let err = Library::default().with_course(&long).unwrap_err();
    assert!(matches!(err, ModelError::NameTooLong { .. }));
}

#[test]
fn session_is_created_inside_its_course() {
    let (lib, session) = library_with_session();

    let course = &lib.courses()[0];
    assert_eq!(course.sessions.len(), 1);
    assert_eq!(lib.session(session).unwrap().title, "Lecture 1");
    assert_eq!(lib.course_of(session).unwrap().id, course.id);
}

#[test]
fn session_in_unknown_course_is_rejected() {
    let (_, missing) = Library::default().with_course("Other").unwrap();
    let err = Library::default().with_session(missing, "L1", 0).unwrap_err();
    assert_eq!(err, ModelError::CourseNotFound);
}

#[test]
fn note_stores_its_recording_offset_in_ms() {
    let (lib, session) = library_with_session();

    let (lib, note) = lib.with_note(session, 83_250, "Big-O definition", &[]).unwrap();

    let stored = lib.session(session).unwrap().note(note).unwrap();
    assert_eq!(stored.offset_ms, 83_250);
    assert_eq!(stored.body, "Big-O definition");
}

#[test]
fn notes_are_kept_in_timeline_order_even_when_added_out_of_order() {
    let (lib, session) = library_with_session();

    let (lib, _) = lib.with_note(session, 5_000, "second", &[]).unwrap();
    let (lib, _) = lib.with_note(session, 1_000, "first", &[]).unwrap();
    let (lib, _) = lib.with_note(session, 9_000, "third", &[]).unwrap();
    let (lib, _) = lib.with_note(session, 5_000, "second-b", &[]).unwrap();

    let bodies: Vec<&str> =
        lib.session(session).unwrap().notes.iter().map(|n| n.body.as_str()).collect();
    assert_eq!(bodies, ["first", "second", "second-b", "third"]);
}

#[test]
fn one_tap_label_creates_a_label_only_moment() {
    let (lib, session) = library_with_session();

    let (lib, note) = lib.with_note(session, 42_000, "", &[LabelKind::Task]).unwrap();

    let stored = lib.session(session).unwrap().note(note).unwrap();
    assert_eq!(stored.labels, vec![LabelKind::Task]);
    assert!(stored.body.is_empty());
}

#[test]
fn note_without_text_or_label_is_rejected() {
    let (lib, session) = library_with_session();
    let err = lib.with_note(session, 0, "   ", &[]).unwrap_err();
    assert_eq!(err, ModelError::EmptyNote);
}

#[test]
fn duplicate_labels_on_a_note_are_stored_once() {
    let (lib, session) = library_with_session();
    let (lib, note) =
        lib.with_note(session, 0, "x", &[LabelKind::Review, LabelKind::Review]).unwrap();
    assert_eq!(lib.session(session).unwrap().note(note).unwrap().labels, vec![LabelKind::Review]);
}

#[test]
fn toggling_a_label_adds_then_removes_it() {
    let (lib, session) = library_with_session();
    let (lib, note) = lib.with_note(session, 0, "x", &[]).unwrap();

    let on = lib.with_label_toggled(session, note, LabelKind::Important).unwrap();
    let off = on.with_label_toggled(session, note, LabelKind::Important).unwrap();

    assert_eq!(on.session(session).unwrap().note(note).unwrap().labels, vec![LabelKind::Important]);
    assert!(off.session(session).unwrap().note(note).unwrap().labels.is_empty());
}

#[test]
fn removing_the_last_label_from_an_empty_note_is_refused() {
    let (lib, session) = library_with_session();
    let (lib, note) = lib.with_note(session, 0, "", &[LabelKind::Task]).unwrap();
    let err = lib.with_label_toggled(session, note, LabelKind::Task).unwrap_err();
    assert_eq!(err, ModelError::EmptyNote);
}

#[test]
fn deleting_a_note_removes_only_that_note() {
    let (lib, session) = library_with_session();
    let (lib, keep) = lib.with_note(session, 0, "keep", &[]).unwrap();
    let (lib, drop) = lib.with_note(session, 10, "drop", &[]).unwrap();

    let lib = lib.without_note(session, drop).unwrap();

    let notes = &lib.session(session).unwrap().notes;
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].id, keep);
}

#[test]
fn renaming_a_label_keeps_its_identity() {
    let lib = Library::default().with_label_renamed(LabelKind::Task, "To do").unwrap();

    assert_eq!(lib.labels().name(LabelKind::Task), "To do");
    assert_eq!(lib.labels().name(LabelKind::Important), "Important");
}

#[test]
fn label_name_must_not_be_blank() {
    let err = Library::default().with_label_renamed(LabelKind::Review, " ").unwrap_err();
    assert_eq!(err, ModelError::EmptyName);
}

#[test]
fn recording_can_be_attached_once() {
    let (lib, session) = library_with_session();
    let rec = Recording { file_name: "a.wav".into(), duration_ms: 60_000, sample_rate: 16_000 };

    let lib = lib.with_recording(session, rec.clone()).unwrap();
    let err = lib.with_recording(session, rec).unwrap_err();

    assert_eq!(lib.session(session).unwrap().recording.as_ref().unwrap().duration_ms, 60_000);
    assert_eq!(err, ModelError::RecordingAlreadyExists);
}

#[test]
fn note_after_the_end_of_the_recording_is_rejected() {
    let (lib, session) = library_with_session();
    let rec = Recording { file_name: "a.wav".into(), duration_ms: 1_000, sample_rate: 16_000 };
    let lib = lib.with_recording(session, rec).unwrap();

    assert!(lib.with_note(session, 1_000, "end", &[]).is_ok());
    assert_eq!(lib.with_note(session, 1_001, "late", &[]).unwrap_err(), ModelError::OffsetOutOfRange);
}

#[test]
fn library_round_trips_through_json() {
    let (lib, session) = library_with_session();
    let (lib, _) = lib.with_note(session, 1_500, "hello", &[LabelKind::Definition]).unwrap();
    let lib = lib.with_label_renamed(LabelKind::Definition, "Term").unwrap();

    let json = lib.to_json().unwrap();
    let back = Library::from_json(&json).unwrap();

    assert_eq!(back, lib);
}

#[test]
fn json_from_a_newer_app_version_is_refused() {
    let json = r#"{"schema_version": 999, "courses": [], "labels": {"labels": []}}"#;
    let err = Library::from_json(json).unwrap_err();
    assert!(matches!(err, ModelError::UnsupportedSchema { found: 999, .. }));
}
