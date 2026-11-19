//! Phase 2 model: slide decks, links from notes to slide pages, photo and
//! sketch notes, and keeping big files (blobs) out of the library JSON.

use lectio_core::material::{
    Attachment, AttachmentKind, BlobKey, MAX_IMAGE_BYTES, MediaKind, SlideLink, check_file, deck_title,
    orphaned_blobs,
};
use lectio_core::{LabelKind, Library, ModelError, Recording, SessionId};

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n rest of a png";
const JPEG_MAGIC: &[u8] = b"\xFF\xD8\xFF\xE0 rest of a jpeg";
const PDF_MAGIC: &[u8] = b"%PDF-1.4 rest of a pdf";

fn library_with_session() -> (Library, SessionId) {
    let (lib, course) = Library::default().with_course("CSCA67").unwrap();
    let (lib, session) = lib.with_session(course, "Lecture 1", 0).unwrap();
    (lib, session)
}

fn photo(width: u32, height: u32) -> Attachment {
    Attachment { kind: AttachmentKind::Photo, blob: BlobKey::new_random(MediaKind::Jpeg), width, height }
}

// ---------- blob keys: they become file names, so they must be safe ----------

#[test]
fn random_blob_keys_are_a_uuid_plus_the_right_extension() {
    let key = BlobKey::new_random(MediaKind::Pdf);
    assert!(key.as_str().ends_with(".pdf"));
    assert_eq!(key.as_str().len(), 36 + 4);
    assert_eq!(BlobKey::parse(key.as_str()).unwrap(), key);
    assert_eq!(key.kind(), MediaKind::Pdf);
}

#[test]
fn blob_keys_that_could_escape_the_folder_are_rejected() {
    for bad in [
        "../../etc/passwd",
        "a/b.pdf",
        "..\\x.png",
        "",
        ".pdf",
        "not-a-uuid.png",
        "123e4567-e89b-12d3-a456-426614174000.exe",
        "123e4567-e89b-12d3-a456-426614174000.pdf/..",
    ] {
        assert_eq!(BlobKey::parse(bad).unwrap_err(), ModelError::InvalidBlobKey, "{bad:?}");
    }
}

#[test]
fn a_tampered_blob_key_in_saved_json_fails_to_load() {
    let (lib, session) = library_with_session();
    let (lib, _) = lib.with_deck(session, "slides.pdf", BlobKey::new_random(MediaKind::Pdf), 3).unwrap();
    let json = lib.to_json().unwrap();
    let key = lib.session(session).unwrap().decks[0].blob.as_str().to_owned();
    let tampered = json.replace(&key, "../../secret.pdf");
    assert!(Library::from_json(&tampered).is_err());
}

// ---------- checking imported files ----------

#[test]
fn file_type_is_detected_from_its_first_bytes_not_its_name() {
    assert_eq!(check_file(PDF_MAGIC, &[MediaKind::Pdf]).unwrap(), MediaKind::Pdf);
    assert_eq!(check_file(PNG_MAGIC, &[MediaKind::Png, MediaKind::Jpeg]).unwrap(), MediaKind::Png);
    assert_eq!(check_file(JPEG_MAGIC, &[MediaKind::Png, MediaKind::Jpeg]).unwrap(), MediaKind::Jpeg);
}

#[test]
fn a_file_of_the_wrong_type_is_refused_with_a_clear_error() {
    assert_eq!(check_file(PNG_MAGIC, &[MediaKind::Pdf]).unwrap_err(), ModelError::UnsupportedFile);
    assert_eq!(check_file(b"hello", &[MediaKind::Png, MediaKind::Jpeg]).unwrap_err(), ModelError::UnsupportedFile);
}

#[test]
fn an_oversized_image_is_refused() {
    let mut big = PNG_MAGIC.to_vec();
    big.resize(MAX_IMAGE_BYTES as usize + 1, 0);
    assert!(matches!(check_file(&big, &[MediaKind::Png]), Err(ModelError::FileTooLarge { .. })));
}

#[test]
fn deck_title_comes_from_the_file_name() {
    assert_eq!(deck_title("Week 3 - Graphs.pdf"), "Week 3 - Graphs");
    assert_eq!(deck_title("   "), "Slides");
    assert_eq!(deck_title(&"x".repeat(300)).chars().count(), lectio_core::MAX_NAME_CHARS);
}

// ---------- slide decks and links ----------

#[test]
fn a_deck_is_stored_by_key_and_page_count_only() {
    let (lib, session) = library_with_session();
    let key = BlobKey::new_random(MediaKind::Pdf);

    let (lib, deck) = lib.with_deck(session, "Week 3.pdf", key.clone(), 32).unwrap();

    let stored = &lib.session(session).unwrap().decks[0];
    assert_eq!((stored.id, stored.blob.clone(), stored.page_count), (deck, key, 32));
    assert_eq!(stored.title, "Week 3");
}

#[test]
fn a_deck_must_be_a_pdf_with_at_least_one_page() {
    let (lib, session) = library_with_session();
    let png = BlobKey::new_random(MediaKind::Png);
    assert_eq!(lib.with_deck(session, "x", png, 3).unwrap_err(), ModelError::UnsupportedFile);
    let pdf = BlobKey::new_random(MediaKind::Pdf);
    assert!(matches!(lib.with_deck(session, "x", pdf, 0), Err(ModelError::PageOutOfRange { .. })));
}

#[test]
fn a_note_can_link_to_a_slide_page_and_be_unlinked() {
    let (lib, session) = library_with_session();
    let (lib, deck) = lib.with_deck(session, "s.pdf", BlobKey::new_random(MediaKind::Pdf), 10).unwrap();
    let (lib, note) = lib.with_note(session, 1_000, "pivot", &[]).unwrap();

    let linked = lib.with_slide_link(session, note, Some(SlideLink { deck, page: 4 })).unwrap();
    let unlinked = linked.with_slide_link(session, note, None).unwrap();

    assert_eq!(linked.session(session).unwrap().note(note).unwrap().slide, Some(SlideLink { deck, page: 4 }));
    assert_eq!(unlinked.session(session).unwrap().note(note).unwrap().slide, None);
}

#[test]
fn a_link_to_a_missing_page_or_deck_is_refused() {
    let (lib, session) = library_with_session();
    let (lib, deck) = lib.with_deck(session, "s.pdf", BlobKey::new_random(MediaKind::Pdf), 10).unwrap();
    let (lib, note) = lib.with_note(session, 0, "x", &[]).unwrap();

    let page_0 = lib.with_slide_link(session, note, Some(SlideLink { deck, page: 0 }));
    let page_11 = lib.with_slide_link(session, note, Some(SlideLink { deck, page: 11 }));
    let (other_lib, other_session) = library_with_session();
    let (_, foreign_deck) =
        other_lib.with_deck(other_session, "o.pdf", BlobKey::new_random(MediaKind::Pdf), 1).unwrap();
    let foreign = lib.with_slide_link(session, note, Some(SlideLink { deck: foreign_deck, page: 1 }));

    assert!(matches!(page_0, Err(ModelError::PageOutOfRange { page: 0, page_count: 10 })));
    assert!(matches!(page_11, Err(ModelError::PageOutOfRange { page: 11, .. })));
    assert_eq!(foreign.unwrap_err(), ModelError::DeckNotFound);
}

#[test]
fn removing_a_deck_also_removes_the_links_to_it() {
    let (lib, session) = library_with_session();
    let (lib, deck) = lib.with_deck(session, "s.pdf", BlobKey::new_random(MediaKind::Pdf), 5).unwrap();
    let (lib, note) = lib.with_note(session, 0, "x", &[]).unwrap();
    let lib = lib.with_slide_link(session, note, Some(SlideLink { deck, page: 2 })).unwrap();

    let lib = lib.without_deck(session, deck).unwrap();

    assert!(lib.session(session).unwrap().decks.is_empty());
    assert_eq!(lib.session(session).unwrap().note(note).unwrap().slide, None);
}

// ---------- photo and sketch notes ----------

#[test]
fn a_photo_note_needs_no_text() {
    let (lib, session) = library_with_session();
    let (lib, note) = lib.with_attachment_note(session, 5_000, photo(1600, 1200), "", &[]).unwrap();

    let stored = lib.session(session).unwrap().note(note).unwrap();
    assert_eq!(stored.attachment.as_ref().unwrap().kind, AttachmentKind::Photo);
    assert!(stored.body.is_empty());
}

#[test]
fn a_sketch_note_keeps_its_caption_and_labels() {
    let (lib, session) = library_with_session();
    let sketch = Attachment {
        kind: AttachmentKind::Sketch,
        blob: BlobKey::new_random(MediaKind::Png),
        width: 800,
        height: 500,
    };
    let (lib, note) =
        lib.with_attachment_note(session, 0, sketch, " tree diagram ", &[LabelKind::Review]).unwrap();

    let stored = lib.session(session).unwrap().note(note).unwrap();
    assert_eq!(stored.body, "tree diagram");
    assert_eq!(stored.labels, vec![LabelKind::Review]);
}

#[test]
fn an_attachment_must_be_an_image_with_a_size() {
    let (lib, session) = library_with_session();
    let pdf_as_photo = Attachment {
        kind: AttachmentKind::Photo,
        blob: BlobKey::new_random(MediaKind::Pdf),
        width: 10,
        height: 10,
    };
    let err = lib.with_attachment_note(session, 0, pdf_as_photo, "", &[]).unwrap_err();
    assert_eq!(err, ModelError::UnsupportedFile);
    assert_eq!(lib.with_attachment_note(session, 0, photo(0, 10), "", &[]).unwrap_err(), ModelError::UnsupportedFile);
}

#[test]
fn removing_the_last_label_from_a_photo_note_is_allowed() {
    let (lib, session) = library_with_session();
    let (lib, note) = lib.with_attachment_note(session, 0, photo(10, 10), "", &[LabelKind::Task]).unwrap();
    assert!(lib.with_label_toggled(session, note, LabelKind::Task).is_ok());
}

// ---------- ordering on the timeline ----------

#[test]
fn photo_sketch_and_text_notes_share_one_timeline_order() {
    let (lib, session) = library_with_session();
    let (lib, _) = lib.with_note(session, 9_000, "late text", &[]).unwrap();
    let (lib, _) = lib.with_attachment_note(session, 1_000, photo(10, 10), "early photo", &[]).unwrap();
    let (lib, _) = lib.with_note(session, 5_000, "middle text", &[]).unwrap();
    let (lib, _) = lib.with_attachment_note(session, 5_000, photo(10, 10), "photo at same moment", &[]).unwrap();

    let bodies: Vec<&str> = lib.session(session).unwrap().notes.iter().map(|n| n.body.as_str()).collect();
    assert_eq!(bodies, ["early photo", "middle text", "photo at same moment", "late text"]);
}

#[test]
fn linking_a_slide_does_not_move_a_note_on_the_timeline() {
    let (lib, session) = library_with_session();
    let (lib, deck) = lib.with_deck(session, "s.pdf", BlobKey::new_random(MediaKind::Pdf), 9).unwrap();
    let (lib, first) = lib.with_note(session, 1_000, "a", &[]).unwrap();
    let (lib, _) = lib.with_note(session, 2_000, "b", &[]).unwrap();

    let lib = lib.with_slide_link(session, first, Some(SlideLink { deck, page: 9 })).unwrap();

    assert_eq!(lib.session(session).unwrap().notes[0].id, first);
}

#[test]
fn a_photo_after_the_end_of_the_recording_is_refused() {
    let (lib, session) = library_with_session();
    let rec = Recording { file_name: "a.wav".into(), duration_ms: 1_000, sample_rate: 16_000 };
    let lib = lib.with_recording(session, rec).unwrap();
    let err = lib.with_attachment_note(session, 1_001, photo(10, 10), "", &[]).unwrap_err();
    assert_eq!(err, ModelError::OffsetOutOfRange);
}

// ---------- keeping blobs out of the JSON ----------

#[test]
fn library_json_stays_small_however_big_the_files_are() {
    let (lib, session) = library_with_session();
    let (lib, _) = lib.with_deck(session, "huge.pdf", BlobKey::new_random(MediaKind::Pdf), 400).unwrap();
    let (lib, _) = lib.with_attachment_note(session, 0, photo(4000, 3000), "", &[]).unwrap();

    let json = lib.to_json().unwrap();

    assert!(json.len() < 3_000, "library JSON is {} bytes", json.len());
}

#[test]
fn deleting_a_photo_note_reports_its_blob_as_orphaned() {
    let (lib, session) = library_with_session();
    let attachment = photo(10, 10);
    let key = attachment.blob.clone();
    let (before, note) = lib.with_attachment_note(session, 0, attachment, "", &[]).unwrap();
    let (before, _) = before.with_deck(session, "s.pdf", BlobKey::new_random(MediaKind::Pdf), 1).unwrap();

    let after = before.without_note(session, note).unwrap();

    assert_eq!(orphaned_blobs(&before, &after), vec![key]);
    assert!(orphaned_blobs(&before, &before).is_empty());
}

// ---------- reading Phase 1 files ----------

#[test]
fn a_phase_1_library_file_still_loads() {
    let (lib, session) = library_with_session();
    let (lib, _) = lib.with_note(session, 1_500, "old note", &[LabelKind::Task]).unwrap();
    // Phase 1 wrote schema_version 1 with no `decks`, `slide` or `attachment` fields.
    let v1 = phase_1_json(&lib.to_json().unwrap());
    assert!(v1.contains("\"schema_version\": 1"));
    assert!(!v1.contains("decks"), "fixture must look like a Phase 1 file:\n{v1}");

    let loaded = Library::from_json(&v1).unwrap();

    assert_eq!(loaded.session(session).unwrap().notes[0].body, "old note");
    assert!(loaded.session(session).unwrap().decks.is_empty());
}

/// Rewrites today's JSON into the Phase 1 shape: version 1, no `decks` key.
fn phase_1_json(current: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(current).unwrap();
    value["schema_version"] = 1.into();
    for course in value["courses"].as_array_mut().unwrap() {
        for session in course["sessions"].as_array_mut().unwrap() {
            session.as_object_mut().unwrap().remove("decks");
        }
    }
    serde_json::to_string_pretty(&value).unwrap()
}
