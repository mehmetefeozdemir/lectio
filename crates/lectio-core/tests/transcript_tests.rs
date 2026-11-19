//! Phase 3: transcript segments aligned to the recording, window stepping,
//! storing a transcript, and the v3 → v4 migration.

use lectio_core::transcript::{
    RawSegment, Segment, Transcript, WINDOW_MS, align_window, merged, next_window_start, segment_at, timestamp_ms,
};
use lectio_core::{Library, ModelError, Recording, SessionId};

fn raw(start_ms: u64, end_ms: u64, text: &str) -> RawSegment {
    RawSegment { start_ms, end_ms, text: text.to_owned() }
}

fn seg(start_ms: u64, end_ms: u64, text: &str) -> Segment {
    Segment { start_ms, end_ms, text: text.to_owned() }
}

fn recorded_session(duration_ms: u64) -> (Library, SessionId) {
    let (lib, course) = Library::default().with_course("CSCA67").unwrap();
    let (lib, session) = lib.with_session(course, "L1", 0).unwrap();
    let recording = Recording { file_name: "a.wav".into(), duration_ms, sample_rate: 16_000, cleaned_file: None };
    (lib.with_recording(session, recording).unwrap(), session)
}

#[test]
fn whisper_timestamp_tokens_are_20_ms_apart() {
    assert_eq!(timestamp_ms(0), 0);
    assert_eq!(timestamp_ms(1), 20);
    assert_eq!(timestamp_ms(1500), 30_000);
}

#[test]
fn window_segments_are_moved_to_recording_time() {
    let segments = align_window(60_000, &[raw(0, 2_400, " BFS uses a queue. "), raw(2_400, 5_000, "Then DFS.")], 600_000);

    assert_eq!(segments, vec![seg(60_000, 62_400, "BFS uses a queue."), seg(62_400, 65_000, "Then DFS.")]);
}

#[test]
fn segments_past_the_end_of_the_recording_are_clamped_or_dropped() {
    let segments = align_window(90_000, &[raw(0, 8_000, "last words"), raw(9_000, 12_000, "noise")], 95_000);

    assert_eq!(segments, vec![seg(90_000, 95_000, "last words")]);
}

#[test]
fn empty_and_bracketed_non_speech_segments_are_dropped() {
    let window = [raw(0, 1_000, "  "), raw(1_000, 2_000, "[Music]"), raw(2_000, 3_000, "(applause)"), raw(3_000, 4_000, "Hi")];

    assert_eq!(align_window(0, &window, 60_000), vec![seg(3_000, 4_000, "Hi")]);
}

#[test]
fn overlapping_or_backwards_segments_are_made_monotonic() {
    let segments = align_window(0, &[raw(0, 3_000, "a"), raw(2_000, 2_500, "b"), raw(4_000, 3_500, "c")], 60_000);

    assert_eq!(segments, vec![seg(0, 3_000, "a"), seg(3_000, 3_000, "b"), seg(4_000, 4_000, "c")]);
}

#[test]
fn the_next_window_starts_where_the_last_complete_segment_ended() {
    assert_eq!(next_window_start(30_000, &[raw(0, 4_000, "a"), raw(4_000, 27_300, "b")], WINDOW_MS), 57_300);
}

#[test]
fn a_window_without_speech_is_skipped_whole() {
    assert_eq!(next_window_start(30_000, &[], WINDOW_MS), 60_000);
    assert_eq!(next_window_start(30_000, &[], 12_000), 42_000, "a short last window");
}

#[test]
fn the_next_window_always_moves_forward() {
    assert_eq!(next_window_start(30_000, &[raw(0, 200, "uh")], WINDOW_MS), 31_000);
}

#[test]
fn merging_drops_repeated_segments_and_trims_overlap() {
    let existing = [seg(0, 4_000, "a"), seg(4_000, 9_000, "b")];
    let new = [seg(3_000, 9_000, "b"), seg(8_000, 12_000, "c"), seg(12_000, 15_000, "d")];

    let expected = vec![seg(0, 4_000, "a"), seg(4_000, 9_000, "b"), seg(9_000, 12_000, "c"), seg(12_000, 15_000, "d")];
    assert_eq!(merged(&existing, &new), expected);
}

#[test]
fn the_segment_playing_is_found_by_offset() {
    let segments = [seg(1_000, 4_000, "a"), seg(4_000, 9_000, "b"), seg(12_000, 15_000, "c")];

    assert_eq!(segment_at(&segments, 500), None);
    assert_eq!(segment_at(&segments, 1_000), Some(0));
    assert_eq!(segment_at(&segments, 4_000), Some(1));
    assert_eq!(segment_at(&segments, 10_000), Some(1), "a gap keeps the previous segment");
    assert_eq!(segment_at(&segments, 99_000), Some(2));
}

#[test]
fn a_transcript_is_stored_on_a_recorded_session() {
    let (lib, session) = recorded_session(60_000);
    let transcript =
        Transcript { language: "en".into(), model: "tiny".into(), segments: vec![seg(0, 2_000, "Hello")], complete: true };

    let lib = lib.with_transcript(session, transcript.clone()).unwrap();

    assert_eq!(lib.session(session).unwrap().transcript.as_ref(), Some(&transcript));
    let cleared = lib.without_transcript(session).unwrap();
    assert_eq!(cleared.session(session).unwrap().transcript, None);
}

#[test]
fn a_transcript_needs_a_recording_and_segments_inside_it() {
    let (lib, course) = Library::default().with_course("C").unwrap();
    let (lib, unrecorded) = lib.with_session(course, "L", 0).unwrap();
    let transcript =
        Transcript { language: "en".into(), model: "tiny".into(), segments: vec![seg(0, 2_000, "Hi")], complete: true };
    assert_eq!(lib.with_transcript(unrecorded, transcript.clone()), Err(ModelError::NoRecording));

    let (lib, session) = recorded_session(1_000);
    assert_eq!(lib.with_transcript(session, transcript), Err(ModelError::OffsetOutOfRange));
}

#[test]
fn a_cleaned_copy_is_remembered_on_the_recording() {
    let (lib, session) = recorded_session(60_000);

    let lib = lib.with_cleaned_copy(session, Some("a-clean.wav")).unwrap();

    let recording = lib.session(session).unwrap().recording.clone().unwrap();
    assert_eq!(recording.cleaned_file.as_deref(), Some("a-clean.wav"));
    assert_eq!(lib.with_cleaned_copy(session, Some("../x.wav")), Err(ModelError::InvalidBlobKey));
}

#[test]
fn a_version_3_library_loads_with_no_transcripts_and_default_audio_settings() {
    let v3 = r#"{
      "schema_version": 3,
      "courses": [{ "id": "6f1c1d6e-0000-4000-8000-000000000001", "name": "C", "sessions": [{
        "id": "6f1c1d6e-0000-4000-8000-000000000002", "title": "L1", "created_unix_ms": 5,
        "recording": { "file_name": "x.wav", "duration_ms": 1000, "sample_rate": 16000 },
        "decks": [{ "id": "6f1c1d6e-0000-4000-8000-000000000004", "title": "W3",
                    "blob": "6f1c1d6e-0000-4000-8000-000000000005.pdf", "page_count": 2 }],
        "notes": [] }]}],
      "labels": { "labels": [] },
      "preferences": { "show_slides": false, "show_feed": true, "show_audio": true, "theme": "dark" }
    }"#;

    let lib = Library::from_json(v3).unwrap();

    let (_, session) = lib.live_sessions()[0];
    assert_eq!(session.transcript, None);
    assert_eq!(session.recording.as_ref().unwrap().cleaned_file, None);
    assert!(session.decks[0].page_text.is_empty());
    assert!(!lib.preferences().show_slides, "existing choices survive");
    assert_eq!(lib.preferences().speed_percent, 100);
    assert!(!lib.preferences().filters.any());
}

#[test]
fn version_4_is_written_and_newer_files_are_refused() {
    assert!(Library::default().to_json().unwrap().contains("\"schema_version\": 4"));
    let v5 = r#"{"schema_version": 5, "courses": [], "labels": {"labels": []}}"#;
    assert!(matches!(Library::from_json(v5), Err(ModelError::UnsupportedSchema { found: 5, supported: 4 })));
}
