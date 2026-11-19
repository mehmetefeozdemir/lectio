//! Sample data for `--screenshot` and the web `#sample` page: a populated
//! timeline with slides and a sketch, without touching the user's real
//! library or needing a microphone.

use lectio_core::material::{Attachment, AttachmentKind, BlobKey, MediaKind, SlideLink};
use lectio_core::{LabelKind, Library, ModelError, Recording, SessionId};

use crate::media::sketch::{SKETCH_HEIGHT, SKETCH_WIDTH, StrokePoints, rasterize_png};

const MINUTE_MS: u64 = 60_000;
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
    let (lib, _) = lib.with_session(algorithms, "Lecture 1", 0)?;
    let (lib, _) = lib.with_session(algorithms, "Lecture 2", 0)?;
    let (lib, session) = lib.with_session(algorithms, "Lecture 3", 0)?;
    let (lib, calculus) = lib.with_course("MATA31")?;
    let (lib, _) = lib.with_session(calculus, "Lecture 1", 0)?;

    let lib = lib.with_recording(
        session,
        Recording { file_name: "sample.wav".into(), duration_ms: 50 * MINUTE_MS, sample_rate: 16_000 },
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

    let blobs = vec![(slides_key, SAMPLE_SLIDES.to_vec()), (sketch_key, sketch_png)];
    Ok(Sample { library: lib, session, blobs })
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
