//! Sample data for `--screenshot` and the web `#sample` page: a populated
//! timeline with slides and a sketch, without touching the user's real
//! library or needing a microphone.

use lectio_core::material::{Attachment, AttachmentKind, BlobKey, MediaKind, SlideLink};
use lectio_core::transcript::{Segment, Transcript};
use lectio_core::{LabelKind, Library, ModelError, Recording, SessionId};

use crate::media::sketch::{SKETCH_HEIGHT, SKETCH_WIDTH, StrokePoints, rasterize_png};

const MINUTE_MS: u64 = 60_000;
const DAY_MS: u64 = 86_400_000;
/// 2026-11-17 (UTC), the sample's main lecture day, as days since 1970-01-01.
const SAMPLE_DAY: i64 = 20_774;

/// 10:00 UTC on the sample day plus `offset` days.
fn lecture_day(offset: i64) -> u64 {
    (SAMPLE_DAY + offset) as u64 * DAY_MS + 10 * 60 * MINUTE_MS
}
/// A two-slide deck (also used by the slide tests and the web smoke test).
const SAMPLE_SLIDES: &[u8] = include_bytes!("../assets/sample-slides.pdf");

pub struct Sample {
    pub library: Library,
    pub session: SessionId,
    /// The sample's files (slides, sketch), served instead of a blob store.
    pub blobs: Vec<(BlobKey, Vec<u8>)>,
}

pub fn sample_library() -> Sample {
    try_sample_library().expect("the built-in sample data is valid")
}

fn try_sample_library() -> Result<Sample, ModelError> {
    let (lib, algorithms) = Library::default().with_course("CSCA67")?;
    let (lib, _) = lib.with_session(algorithms, "Lecture 1", lecture_day(-14))?;
    let (lib, _) = lib.with_session(algorithms, "Lecture 2", lecture_day(-7))?;
    let (lib, session) = lib.with_session(algorithms, "Graph traversal", lecture_day(0))?;
    let (lib, calculus) = lib.with_course("MATA31")?;
    let (lib, _) = lib.with_session(calculus, "Limits", lecture_day(1))?;
    let (lib, old_session) = lib.with_session(calculus, "Office hours (test run)", lecture_day(-3))?;
    let lib = lib.with_session_trashed(old_session, lecture_day(2))?;

    let lib = lib.with_recording(
        session,
        Recording { file_name: "sample.wav".into(), duration_ms: 50 * MINUTE_MS, sample_rate: 16_000, cleaned_file: None },
    )?;
    let slides_key = BlobKey::new_random(MediaKind::Pdf);
    let (lib, deck) = lib.with_deck(session, "Week 3 - Graphs.pdf", slides_key.clone(), 2)?;

    // (offset, text, labels, slide page)
    let notes: [(u64, &str, &[LabelKind], Option<u32>); 7] = [
        (2 * MINUTE_MS + 10_000, "Graphs: a set of vertices and a set of edges", &[LabelKind::Definition], Some(1)),
        (6 * MINUTE_MS, "Adjacency list vs. matrix: space trade-off", &[], Some(1)),
        (11 * MINUTE_MS + 40_000, "", &[LabelKind::Important], None),
        (17 * MINUTE_MS + 5_000, "BFS visits vertices in order of distance from the start", &[LabelKind::Review], Some(2)),
        (24 * MINUTE_MS + 30_000, "Proof sketch on the board; copy it from the slides later", &[LabelKind::Task], None),
        (33 * MINUTE_MS, "DFS uses a stack (or recursion)", &[], None),
        (41 * MINUTE_MS + 20_000, "Exam hint: cycle detection", &[LabelKind::Important, LabelKind::Review], None),
    ];
    let lib = notes.iter().try_fold(lib, |lib, (offset, body, labels, page)| {
        let (lib, note) = lib.with_note(session, *offset, body, labels)?;
        let link = page.map(|page| SlideLink { deck, page });
        lib.with_slide_link(session, note, link)
    })?;

    // Section headings and slides posted to the feed (ADR-015).
    let (lib, _) = lib.with_note(session, 30_000, "Graph basics", &[LabelKind::Heading])?;
    let (lib, _) = lib.with_slide_item(session, MINUTE_MS, SlideLink { deck, page: 1 })?;
    let (lib, _) = lib.with_note(session, 14 * MINUTE_MS + 30_000, "Breadth-first search", &[LabelKind::Heading])?;
    let (lib, bfs_slide) = lib.with_slide_item(session, 15 * MINUTE_MS, SlideLink { deck, page: 2 })?;
    let lib = lib.with_note_text(session, bfs_slide, "BFS step by step")?;

    let sketch_png = rasterize_png(&sample_tree()).map_err(|_| ModelError::UnsupportedFile)?;
    let sketch_key = BlobKey::new_random(MediaKind::Png);
    let sketch = Attachment {
        kind: AttachmentKind::Sketch,
        blob: sketch_key.clone(),
        width: SKETCH_WIDTH,
        height: SKETCH_HEIGHT,
    };
    let (lib, sketch_note) = lib.with_attachment_note(session, 18 * MINUTE_MS, sketch, "BFS tree from the board", &[])?;
    let lib = lib.with_slide_link(session, sketch_note, Some(SlideLink { deck, page: 2 }))?;
    let lib = lib.with_transcript(session, sample_transcript())?;

    let blobs = vec![(slides_key, SAMPLE_SLIDES.to_vec()), (sketch_key, sketch_png)];
    Ok(Sample { library: lib, session, blobs })
}

/// A few transcript segments around the playhead (17:40), so the Transcript
/// tab has something to show.
fn sample_transcript() -> Transcript {
    let lines: [(u64, u64, &str); 8] = [
        (16 * MINUTE_MS + 30_000, 16 * MINUTE_MS + 36_000, "So let's look at breadth-first search."),
        (16 * MINUTE_MS + 36_000, 16 * MINUTE_MS + 44_000, "We start at one vertex and visit all of its neighbours first."),
        (16 * MINUTE_MS + 44_000, 16 * MINUTE_MS + 53_000, "To remember who comes next, we keep a queue."),
        (16 * MINUTE_MS + 53_000, 17 * MINUTE_MS + 4_000, "Vertices leave the queue in the order they were discovered."),
        (17 * MINUTE_MS + 4_000, 17 * MINUTE_MS + 15_000, "That means we visit them in order of distance from the start."),
        (17 * MINUTE_MS + 15_000, 17 * MINUTE_MS + 29_000, "This is why BFS finds shortest paths when every edge counts the same."),
        (17 * MINUTE_MS + 29_000, 17 * MINUTE_MS + 47_000, "Let me draw the tree it builds on the board."),
        (17 * MINUTE_MS + 47_000, 17 * MINUTE_MS + 58_000, "Each level of the tree is one more step away from the root."),
    ];
    let segments =
        lines.iter().map(|&(start_ms, end_ms, text)| Segment { start_ms, end_ms, text: text.to_owned() }).collect();
    Transcript { language: "en".into(), model: "base".into(), segments, complete: true }
}

/// A small tree (root with two children), like a BFS diagram on a board.
fn sample_tree() -> Vec<StrokePoints> {
    let circle = |cx: f32, cy: f32| -> StrokePoints {
        (0..=24)
            .map(|i| {
                let a = i as f32 / 24.0 * std::f32::consts::TAU;
                [cx + 40.0 * a.cos(), cy + 40.0 * a.sin()]
            })
            .collect()
    };
    vec![
        circle(480.0, 120.0),
        circle(300.0, 400.0),
        circle(660.0, 400.0),
        vec![[455.0, 152.0], [325.0, 368.0]],
        vec![[505.0, 152.0], [635.0, 368.0]],
    ]
}
