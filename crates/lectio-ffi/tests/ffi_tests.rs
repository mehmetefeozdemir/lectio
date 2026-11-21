//! The mobile surface (ADR-029), tested from Rust: what Swift (and later
//! Kotlin) can call, and that it keeps the desktop's files unchanged.

use std::path::PathBuf;
use std::sync::Arc;

use lectio_ffi::{
    AudioPlayer, AudioRecorder, ExportFormat, HomeFilter, InkColor, InkTool, LectioLibrary, NoteDraft, NoteKind, NoteLabel, Point,
    SessionSort, check_formula, render_formula_png, speech_models, study_models, verify_download,
};

const SLIDES: &[u8] = include_bytes!("../../../assets/sample-slides.pdf");

/// A fresh data folder in the system's temporary folder, removed afterwards.
struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("lectio-ffi-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn library(folder: &Folder) -> Arc<LectioLibrary> {
    LectioLibrary::open(folder.path()).unwrap()
}

#[test]
fn notes_persist_in_the_desktop_format() {
    let folder = Folder::new("persist");
    let lib = library(&folder);
    let course = lib.add_course("CSCA67".into()).unwrap();
    let session = lib.add_session(course.clone(), "Graphs".into()).unwrap();
    lib.add_note(session.clone(), 0, "Breadth-first search".into(), vec![NoteLabel::Heading]).unwrap();
    lib.add_note(session.clone(), 1_500, "BFS uses a queue".into(), vec![NoteLabel::Review]).unwrap();

    // The same folder, read by the desktop's own store, has the same notes.
    let saved = lectio_core::storage::JsonFileStore::new(folder.0.clone());
    let desktop = lectio_core::storage::LibraryStore::load(&saved).unwrap().library;
    let (_, s) = desktop.live_sessions()[0];
    assert_eq!(s.title, "Graphs");
    assert_eq!(s.notes[1].body, "BFS uses a queue");
    assert_eq!(s.notes[1].offset_ms, 1_500);

    let reopened = library(&folder);
    let detail = reopened.session(session).unwrap();
    let labels: Vec<Vec<NoteLabel>> = detail.notes.iter().map(|n| n.labels.clone()).collect();
    assert_eq!(labels, vec![vec![NoteLabel::Heading], vec![NoteLabel::Review]]);
    assert_eq!(reopened.courses().unwrap()[0].session_count, 1);
}

#[test]
fn bad_input_is_an_error_with_a_plain_message() {
    let folder = Folder::new("errors");
    let lib = library(&folder);
    let message = lib.add_course("   ".into()).unwrap_err().to_string();
    assert!(message.contains("name"), "{message}");
    assert!(lib.add_session("not-a-uuid".into(), "S".into()).is_err());
    // A folder can't be made inside a file, on any OS (a path like
    // /not/writable is writable on Windows runners: it becomes C:\not\writable).
    let file = folder.0.join("a-file");
    std::fs::write(&file, b"x").unwrap();
    assert!(LectioLibrary::open(file.join("lectio").to_string_lossy().into_owned()).is_err());
}

#[test]
fn notes_labels_formulas_and_edits() {
    let folder = Folder::new("notes");
    let lib = library(&folder);
    let course = lib.add_course("MATA31".into()).unwrap();
    let session = lib.add_session(course, "Limits".into()).unwrap();

    let note = lib.add_note(session.clone(), 100, "epsilon delta".into(), vec![]).unwrap();
    lib.toggle_label(session.clone(), note.clone(), NoteLabel::Important).unwrap();
    lib.edit_note_text(session.clone(), note.clone(), "epsilon-delta definition".into()).unwrap();
    let formula = lib.add_formula_note(session.clone(), 200, r"\lim_{x \to 0} \frac{\sin x}{x} = 1".into(), vec![]).unwrap();
    assert!(lib.add_formula_note(session.clone(), 300, r"\frac{1}{".into(), vec![]).is_err());

    let notes = lib.session(session.clone()).unwrap().notes;
    assert_eq!((notes[0].body.as_str(), notes[0].labels.clone()), ("epsilon-delta definition", vec![NoteLabel::Important]));
    assert_eq!(notes[1].kind, NoteKind::Formula);
    lib.delete_note(session.clone(), formula).unwrap();
    assert_eq!(lib.session(session).unwrap().notes.len(), 1);
}

#[test]
fn formulas_are_checked_and_drawn_for_the_preview() {
    assert_eq!(check_formula("x^2".into()), None);
    assert!(check_formula(r"\nope".into()).unwrap().contains(r"\nope"));
    let png = render_formula_png(r"\frac{a}{b}".into(), 32.0, false).unwrap();
    assert!(png.starts_with(b"\x89PNG"));
}

#[test]
fn collections_filter_sort_and_search() {
    let folder = Folder::new("collections");
    let lib = library(&folder);
    let a = lib.add_course("A".into()).unwrap();
    let b = lib.add_course("B".into()).unwrap();
    let first = lib.add_session(a.clone(), "graphs".into()).unwrap();
    let second = lib.add_session(b, "Limits".into()).unwrap();
    lib.add_note(second.clone(), 0, "squeeze theorem".into(), vec![NoteLabel::Important]).unwrap();
    let midterm = lib.add_collection("Midterm".into()).unwrap();
    lib.add_to_collection(midterm.clone(), first.clone()).unwrap();

    let titles = |filter: HomeFilter, sort: SessionSort| -> Vec<String> {
        lib.sessions(filter, sort).unwrap().into_iter().map(|s| s.title).collect()
    };
    assert_eq!(titles(HomeFilter::All, SessionSort::Title), vec!["graphs", "Limits"]);
    assert_eq!(titles(HomeFilter::Collection { id: midterm.clone() }, SessionSort::Newest), vec!["graphs"]);
    assert_eq!(titles(HomeFilter::Course { id: a }, SessionSort::Newest), vec!["graphs"]);
    assert_eq!(titles(HomeFilter::Label { label: NoteLabel::Important }, SessionSort::Newest), vec!["Limits"]);
    assert_eq!(lib.collections().unwrap()[0].session_ids, vec![first.clone()]);
    lib.remove_from_collection(midterm.clone(), first).unwrap();
    lib.rename_collection(midterm.clone(), "Final".into()).unwrap();
    assert_eq!(lib.collections().unwrap()[0].name, "Final");

    let hits = lib.search("squeeze".into()).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].session_id.as_str(), hits[0].offset_ms), (second.as_str(), Some(0)));
}

#[test]
fn slides_ink_and_photos() {
    let folder = Folder::new("slides");
    let lib = library(&folder);
    let course = lib.add_course("C".into()).unwrap();
    let session = lib.add_session(course, "S".into()).unwrap();

    let deck = lib.import_slides(session.clone(), "Week 3.pdf".into(), SLIDES.to_vec()).unwrap();
    let detail = lib.session(session.clone()).unwrap();
    assert_eq!((detail.decks[0].title.as_str(), detail.decks[0].page_count), ("Week 3", 2));
    assert_eq!(std::fs::read(&detail.decks[0].pdf_path).unwrap(), SLIDES, "PDFKit reads the stored file");
    assert!(lib.import_slides(session.clone(), "fake.pdf".into(), b"not a pdf".to_vec()).is_err());

    let note = lib.add_note(session.clone(), 0, "about slide 2".into(), vec![]).unwrap();
    lib.link_note_to_slide(session.clone(), note, Some(deck.clone()), 2).unwrap();
    assert_eq!(lib.session(session.clone()).unwrap().notes[0].slide.as_ref().map(|s| s.page), Some(2));

    let line = vec![Point { x: 0.1, y: 0.5 }, Point { x: 0.9, y: 0.5 }];
    lib.add_ink_stroke(session.clone(), deck.clone(), 2, InkTool::Pen, InkColor::Red, line).unwrap();
    lib.add_ink_stroke(session.clone(), deck.clone(), 2, InkTool::Highlighter, InkColor::Yellow, vec![Point { x: 0.5, y: 0.9 }]).unwrap();
    let ink = lib.ink(session.clone(), deck.clone(), 2).unwrap();
    assert_eq!(ink.len(), 2);
    // Widths are fractions of the page width: thin lines, not the whole page (P-073).
    let widths: Vec<f32> = ink.iter().map(|stroke| stroke.drawn_width).collect();
    assert!(widths.iter().all(|w| *w > 0.0 && *w < 0.05), "drawn widths {widths:?}");
    assert_eq!(lib.erase_ink(session.clone(), deck.clone(), 2, Point { x: 0.5, y: 0.51 }).unwrap(), 1);
    lib.undo_ink(session.clone(), deck.clone(), 2).unwrap();
    assert!(lib.ink(session.clone(), deck, 2).unwrap().is_empty());

    let png = render_formula_png("x".into(), 20.0, false).unwrap(); // any small PNG works as a photo
    let photo = lib.add_photo_note(session.clone(), 500, png, "board".into()).unwrap();
    let notes = lib.session(session.clone()).unwrap().notes;
    let shown = notes.iter().find(|n| n.id == photo).unwrap().photo.clone().unwrap();
    assert!(std::path::Path::new(&shown.path).exists() && shown.width > 0);
    assert!(lib.add_photo_note(session, 600, b"nope".to_vec(), String::new()).is_err());
}

/// One second of a 440 Hz tone at 48 kHz, stereo, as the microphone gives it.
fn tone_48k_stereo() -> Vec<f32> {
    (0..48_000)
        .flat_map(|i| {
            let v = (i as f32 / 48_000.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            [v, v]
        })
        .collect()
}

#[test]
fn recording_writes_the_desktop_wav_and_attaches_it() {
    let folder = Folder::new("record");
    let lib = library(&folder);
    let course = lib.add_course("C".into()).unwrap();
    let session = lib.add_session(course, "S".into()).unwrap();
    let path = lib.recording_path(session.clone()).unwrap();
    // Compare path components, not text: Windows separates them with `\`.
    assert!(std::path::Path::new(&path).ends_with(std::path::Path::new("recordings").join(format!("{session}.wav"))), "{path}");

    let recorder = AudioRecorder::start(path.clone()).unwrap();
    let samples = tone_48k_stereo();
    for chunk in samples.chunks(4_800) {
        recorder.push(chunk.to_vec(), 48_000, 2).unwrap();
    }
    assert_eq!(recorder.duration_ms(), 1_000);
    let duration = recorder.finish().unwrap();
    lib.attach_recording(session.clone(), duration).unwrap();

    let wav = hound::WavReader::open(&path).unwrap();
    let spec = wav.spec();
    assert_eq!((spec.channels, spec.sample_rate, spec.bits_per_sample), (1, 16_000, 16));
    assert_eq!(wav.len(), 16_000);
    assert_eq!(lib.session(session).unwrap().info.duration_ms, Some(1_000));
}

#[test]
fn playback_keeps_time_at_any_speed_and_seeks() {
    let folder = Folder::new("play");
    let path = folder.0.join("tone.wav").to_string_lossy().into_owned();
    let recorder = AudioRecorder::start(path.clone()).unwrap();
    for _ in 0..4 {
        recorder.push(tone_48k_stereo(), 48_000, 2).unwrap();
    }
    recorder.finish().unwrap();

    let player = AudioPlayer::open(path).unwrap();
    assert_eq!((player.duration_ms(), player.sample_rate()), (4_000, 16_000));
    player.set_speed_percent(200);
    let mut heard = 0;
    while !player.next_block(1_600).is_empty() {
        heard += 1;
    }
    assert!((19..=21).contains(&heard), "4 s at 2× is about 2 s = 20 blocks of 0.1 s, got {heard}");
    assert!(player.is_finished());

    player.seek_ms(1_000);
    player.set_speed_percent(100);
    assert_eq!(player.position_ms(), 1_000);
    assert_eq!(player.next_block(1_600).len(), 1_600);
    assert!((1_090..=1_110).contains(&player.position_ms()));
}

#[test]
fn exports_are_whole_documents() {
    let folder = Folder::new("export");
    let lib = library(&folder);
    let course = lib.add_course("C".into()).unwrap();
    let session = lib.add_session(course, "Graphs".into()).unwrap();
    let deck = lib.import_slides(session.clone(), "W3.pdf".into(), SLIDES.to_vec()).unwrap();
    let note = lib.add_note(session.clone(), 0, "BFS uses a queue".into(), vec![]).unwrap();
    lib.link_note_to_slide(session.clone(), note, Some(deck), 1).unwrap();
    lib.add_formula_note(session.clone(), 1, "x^2".into(), vec![]).unwrap();

    let pdf = lib.export(session.clone(), ExportFormat::Pdf, false).unwrap();
    let docx = lib.export(session.clone(), ExportFormat::Docx, false).unwrap();
    let markdown = String::from_utf8(lib.export(session.clone(), ExportFormat::Markdown, false).unwrap()).unwrap();

    assert!(pdf.starts_with(b"%PDF-") && pdf.len() > 20_000, "with the slide picture");
    assert!(docx.starts_with(b"PK"));
    assert!(markdown.contains("BFS uses a queue") && markdown.contains("$$\nx^2\n$$"));
    assert_eq!(lib.export_file_name(session, ExportFormat::Docx).unwrap(), "Graphs.docx");
}

#[test]
fn offline_study_quiz_and_flashcards() {
    let folder = Folder::new("study");
    let lib = library(&folder);
    let course = lib.add_course("C".into()).unwrap();
    let session = lib.add_session(course, "S".into()).unwrap();
    lib.add_note(session.clone(), 0, "Queue: first in, first out".into(), vec![NoteLabel::Definition]).unwrap();
    lib.add_note(session.clone(), 10, "Stack: last in, first out".into(), vec![NoteLabel::Definition]).unwrap();

    let quiz = lib.offline_quiz(vec![session.clone()], 5).unwrap();
    assert_eq!(quiz.len(), 2);
    assert_eq!(quiz[0].answer, "Queue");
    assert!(lectio_ffi::grade_answer("the queue".into(), quiz[0].answer.clone()));
    lib.record_quiz_attempt(session.clone(), 2, 1).unwrap();
    assert_eq!(lib.session(session.clone()).unwrap().quiz_scores.len(), 1);

    assert_eq!(lib.make_definition_cards(session.clone(), 0).unwrap(), 2);
    assert_eq!(lib.make_definition_cards(session, 0).unwrap(), 0, "never twice");
    let due = lib.due_cards(0).unwrap();
    assert_eq!(due.len(), 2);
    lib.grade_card(due[0].id.clone(), lectio_ffi::Grade::Good, 0).unwrap();
    assert_eq!(lib.due_cards(0).unwrap().len(), 1, "a good answer is due tomorrow");
}

#[test]
fn model_files_are_listed_and_checked() {
    let speech = speech_models();
    assert_eq!(speech.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), vec!["tiny", "base", "small"]);
    assert!(speech[0].files.iter().all(|f| f.url.starts_with("https://") && f.sha256.len() == 64));
    assert!(!study_models().is_empty());

    let folder = Folder::new("verify");
    let path = folder.0.join("f.bin");
    std::fs::write(&path, b"abc").unwrap();
    let sha_of_abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let path = path.to_string_lossy().into_owned();
    assert!(verify_download(path.clone(), 3, sha_of_abc.into()));
    assert!(!verify_download(path.clone(), 4, sha_of_abc.into()), "wrong size");
    assert!(!verify_download(path, 3, "00".repeat(32)), "wrong checksum");
}

#[test]
fn a_note_is_stamped_when_typing_started() {
    let draft = NoteDraft::new();
    assert_eq!(draft.anchor_ms(5_000), 5_000, "nothing typed: now");
    draft.set_text("   ".into(), 1_000);
    draft.set_text("Mito".into(), 2_000);
    draft.set_text("Mitochondria make ATP".into(), 9_000);
    assert_eq!(draft.anchor_ms(12_000), 2_000, "the first real keystroke");
    draft.set_text(String::new(), 13_000);
    assert_eq!(draft.anchor_ms(14_000), 14_000, "cleared: starts again");
}

/// Kotlin moves a `List<Float>` through JNA one boxed number at a time; the
/// byte versions move a block in one copy and must give the same audio.
#[test]
fn audio_as_little_endian_bytes_matches_audio_as_floats() {
    let folder = Folder::new("bytes");
    let (by_float, by_bytes) = (folder.0.join("floats.wav"), folder.0.join("bytes.wav"));
    let floats = AudioRecorder::start(by_float.to_string_lossy().into_owned()).unwrap();
    let bytes = AudioRecorder::start(by_bytes.to_string_lossy().into_owned()).unwrap();
    for chunk in tone_48k_stereo().chunks(4_800) {
        floats.push(chunk.to_vec(), 48_000, 2).unwrap();
        bytes.push_f32le(chunk.iter().flat_map(|s| s.to_le_bytes()).collect(), 48_000, 2).unwrap();
    }
    assert_eq!(floats.finish().unwrap(), bytes.finish().unwrap());
    assert_eq!(std::fs::read(&by_float).unwrap(), std::fs::read(&by_bytes).unwrap());
    assert!(bytes_recorder_refuses_a_partial_sample(&folder));

    let player = AudioPlayer::open(by_bytes.to_string_lossy().into_owned()).unwrap();
    let twin = AudioPlayer::open(by_float.to_string_lossy().into_owned()).unwrap();
    let block: Vec<u8> = player.next_block_f32le(2_048);
    let expected: Vec<u8> = twin.next_block(2_048).iter().flat_map(|s| s.to_le_bytes()).collect();
    assert_eq!(block, expected);
    assert_eq!(block.len(), 2_048 * 4);
}

fn bytes_recorder_refuses_a_partial_sample(folder: &Folder) -> bool {
    let recorder = AudioRecorder::start(folder.0.join("odd.wav").to_string_lossy().into_owned()).unwrap();
    recorder.push_f32le(vec![0, 0, 0], 16_000, 1).is_err()
}

/// The store screenshots' sample: the desktop's sample lecture, with its
/// slides and a (silent) recording on disk, and nothing added twice.
#[test]
fn the_sample_library_has_a_playable_lecture_with_slides_and_a_quiz() {
    let folder = Folder::new("sample");
    let lib = library(&folder);
    let session = lib.add_sample_library().unwrap();
    let detail = lib.session(session.clone()).unwrap();
    assert_eq!(detail.info.title, "Graph traversal");
    assert_eq!(detail.transcript.len(), 8);
    assert!(std::path::Path::new(&detail.decks[0].pdf_path).exists());
    assert_eq!(lib.ink(session.clone(), detail.decks[0].id.clone(), 2).unwrap().len(), 2);

    let player = AudioPlayer::open(detail.recording_path.unwrap()).unwrap();
    assert_eq!(player.duration_ms(), detail.info.duration_ms.unwrap());
    assert_eq!(lib.offline_quiz(vec![session], 10).unwrap().len(), 3);
    assert!(!lib.due_cards(u64::MAX).unwrap().is_empty(), "flashcards from the Definition notes");

    let again = lib.add_sample_library().unwrap_err().to_string();
    assert!(again.contains("empty"), "{again}");
}
