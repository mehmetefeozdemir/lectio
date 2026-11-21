//! Sample data for `--screenshot` and the web `#sample` page: a populated
//! timeline with slides and a sketch, without touching the user's real
//! library or needing a microphone. The lecture itself is shared with the
//! mobile apps' store screenshots (`lectio_core::sample`); the desktop adds a
//! sketch, which is drawn with the desktop's sketch code.

use lectio_core::material::{Attachment, AttachmentKind, BlobKey, MediaKind, SlideLink};
use lectio_core::sample::SAMPLE_SLIDES;
use lectio_core::{ModelError, SessionId};

use crate::media::sketch::{SKETCH_HEIGHT, SKETCH_WIDTH, StrokePoints, rasterize_png};

const MINUTE_MS: u64 = 60_000;

pub struct Sample {
    pub library: lectio_core::Library,
    pub session: SessionId,
    /// The sample's files (slides, sketch), served instead of a blob store.
    pub blobs: Vec<(BlobKey, Vec<u8>)>,
}

pub fn sample_library() -> Sample {
    try_sample_library().expect("the built-in sample data is valid")
}

fn try_sample_library() -> Result<Sample, ModelError> {
    let shared = lectio_core::sample::sample_library()?;
    let (session, deck) = (shared.session, shared.deck);

    let sketch_png = rasterize_png(&sample_tree()).map_err(|_| ModelError::UnsupportedFile)?;
    let sketch_key = BlobKey::new_random(MediaKind::Png);
    let sketch = Attachment { kind: AttachmentKind::Sketch, blob: sketch_key.clone(), width: SKETCH_WIDTH, height: SKETCH_HEIGHT };
    let (lib, sketch_note) = shared.library.with_attachment_note(session, 18 * MINUTE_MS, sketch, "BFS tree from the board", &[])?;
    let lib = lib.with_slide_link(session, sketch_note, Some(SlideLink { deck, page: 2 }))?;

    let blobs = vec![(shared.slides, SAMPLE_SLIDES.to_vec()), (sketch_key, sketch_png)];
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
