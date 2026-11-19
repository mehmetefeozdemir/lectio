//! Local-first storage: the library survives a restart, and a broken file is
//! set aside instead of being silently overwritten.

use lectio_core::material::{BlobKey, MediaKind};
use lectio_core::storage::{FileBlobStore, JsonFileStore, LibraryStore, SavedData, interpret_saved};
use lectio_core::{LabelKind, Library};

// ---- blobs (PDFs, photos, sketches) live next to the library, not in it ----

#[test]
fn a_blob_is_stored_as_its_own_file_and_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let blobs = FileBlobStore::new(dir.path());
    let key = BlobKey::new_random(MediaKind::Pdf);

    blobs.put(&key, b"%PDF-1.4 slides").unwrap();

    assert_eq!(blobs.get(&key).unwrap(), b"%PDF-1.4 slides");
    assert_eq!(blobs.path(&key), dir.path().join("blobs").join(key.as_str()));
    let names: Vec<String> = std::fs::read_dir(dir.path().join("blobs"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, [key.as_str()], "no temporary file is left behind");
}

#[test]
fn deleting_a_blob_removes_the_file_and_tolerates_a_missing_one() {
    let dir = tempfile::tempdir().unwrap();
    let blobs = FileBlobStore::new(dir.path());
    let key = BlobKey::new_random(MediaKind::Png);
    blobs.put(&key, b"png").unwrap();

    blobs.delete(&key).unwrap();
    blobs.delete(&key).unwrap();

    assert!(blobs.get(&key).is_err());
}

// ---- the shared decision every store uses (file system and browser) ----

#[test]
fn valid_saved_text_is_read_back_as_a_library() {
    let (lib, _) = Library::default().with_course("CSCA67").unwrap();
    let text = lib.to_json().unwrap();
    assert_eq!(interpret_saved(&text), SavedData::Valid(lib));
}

#[test]
fn saved_text_from_a_newer_version_must_be_left_alone() {
    let text = r#"{"schema_version": 999, "courses": [], "labels": {"labels": []}}"#;
    assert!(matches!(interpret_saved(text), SavedData::Newer(_)));
}

#[test]
fn garbage_saved_text_is_unreadable() {
    assert_eq!(interpret_saved("{ not json"), SavedData::Unreadable);
}

#[test]
fn stores_without_background_work_report_no_background_errors() {
    let dir = tempfile::tempdir().unwrap();
    assert!(JsonFileStore::new(dir.path()).take_background_error().is_none());
}

#[test]
fn missing_file_loads_an_empty_library() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonFileStore::new(dir.path());

    let outcome = store.load().unwrap();

    assert_eq!(outcome.library, Library::default());
    assert!(outcome.recovered_from.is_none());
}

#[test]
fn saved_library_is_loaded_back_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (lib, course) = Library::default().with_course("CSCA67").unwrap();
    let (lib, session) = lib.with_session(course, "Lecture 1", 0).unwrap();
    let (lib, _) = lib.with_note(session, 7_000, "pivot", &[LabelKind::Important]).unwrap();

    JsonFileStore::new(dir.path()).save(&lib).unwrap();
    let reloaded = JsonFileStore::new(dir.path()).load().unwrap().library;

    assert_eq!(reloaded, lib);
}

#[test]
fn save_leaves_no_temporary_file_behind() {
    let dir = tempfile::tempdir().unwrap();
    JsonFileStore::new(dir.path()).save(&Library::default()).unwrap();

    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["library.json"]);
}

#[test]
fn corrupt_file_is_moved_aside_and_an_empty_library_is_returned() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("library.json"), "{ not json").unwrap();

    let outcome = JsonFileStore::new(dir.path()).load().unwrap();

    assert_eq!(outcome.library, Library::default());
    let backup = outcome.recovered_from.expect("backup path");
    assert_eq!(std::fs::read_to_string(backup).unwrap(), "{ not json");
    assert!(!dir.path().join("library.json").exists());
}

#[test]
fn file_from_a_newer_version_is_left_untouched_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.json");
    std::fs::write(&path, r#"{"schema_version": 999, "courses": [], "labels": {"labels": []}}"#).unwrap();

    let result = JsonFileStore::new(dir.path()).load();

    assert!(result.is_err());
    assert!(path.exists());
}

#[test]
fn audio_files_live_in_a_recordings_folder() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonFileStore::new(dir.path());
    assert_eq!(store.audio_path("s1.wav"), dir.path().join("recordings").join("s1.wav"));
}
