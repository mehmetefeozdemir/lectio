//! The pictures a document needs (ADR-025): formulas, slide pages with their
//! ink, and photos/sketches, as PNG with the size to show them at. Shared by
//! the desktop export job (one picture per frame) and the mobile apps (all
//! at once, on a background thread, `make_all`).

use std::collections::HashMap;

use lectio_core::Session;
use lectio_core::material::BlobKey;
use lectio_core::paper::{AssetKey, Block, assets};

use crate::{Picture, Pictures, formula, images, ink, slides};

/// Formulas are drawn at twice their shown size, for sharp printing.
pub const FORMULA_PX: f32 = 40.0;
/// Pictures are shown at most this wide in documents (CSS pixels).
pub const PICTURE_WIDTH: u32 = 640;
/// Photos are scaled down to this before they go into a document.
pub const PHOTO_MAX_SIDE: u32 = 1_600;

/// A picture of `width × height` pixels shown at most `max` wide, keeping its shape.
pub fn shown(png: Vec<u8>, width: u32, height: u32, max: u32) -> Picture {
    let w = width.min(max).max(1);
    Picture { png, width: w, height: (u64::from(height) * u64::from(w) / u64::from(width.max(1))) as u32 }
}

/// The stored file a picture is made from (`None` for formulas).
pub fn blob_for(key: &AssetKey, session: &Session) -> Option<BlobKey> {
    match key {
        AssetKey::Slide(deck, _) => session.decks.iter().find(|d| d.id == *deck).map(|d| d.blob.clone()),
        AssetKey::Image(blob) => Some(blob.clone()),
        AssetKey::Formula(_) => None,
    }
}

/// Makes one picture. `file` is the stored file from `blob_for` (the whole
/// PDF for a slide, the photo for an image), not needed for formulas.
pub fn picture_for(key: &AssetKey, session: &Session, file: Option<&[u8]>) -> Result<Picture, String> {
    match key {
        AssetKey::Formula(latex) => {
            let (png, w, h) = formula::png(latex, FORMULA_PX)?;
            Ok(Picture { png, width: w / 2, height: h / 2 })
        }
        AssetKey::Slide(deck, page) => {
            let bytes = file.ok_or("the slides file is missing")?;
            let strokes = session.decks.iter().find(|d| d.id == *deck).map(|d| d.ink_on(*page).to_vec()).unwrap_or_default();
            let rendered = slides::render_one(bytes, *page)?;
            let png = ink::page_png(&rendered, &strokes)?;
            Ok(shown(png, rendered.width, rendered.height, PICTURE_WIDTH))
        }
        AssetKey::Image(_) => {
            let decoded = images::decode(file.ok_or("the photo file is missing")?, PHOTO_MAX_SIDE)?;
            Ok(shown(decoded.to_png()?, decoded.width, decoded.height, PICTURE_WIDTH))
        }
    }
}

/// Every picture `blocks` need. `load` reads a stored file; a file that
/// can't be read or drawn leaves its picture out (the text stays).
pub fn make_all(blocks: &[Block], session: &Session, load: &mut dyn FnMut(&BlobKey) -> Result<Vec<u8>, String>) -> Pictures {
    let mut files: HashMap<BlobKey, Option<Vec<u8>>> = HashMap::new();
    let mut pictures = Pictures::new();
    for key in assets(blocks) {
        let file = blob_for(&key, session).and_then(|blob| files.entry(blob.clone()).or_insert_with(|| load(&blob).ok()).clone());
        if let Ok(picture) = picture_for(&key, session, file.as_deref()) {
            pictures.insert(key, picture);
        }
    }
    pictures
}

#[cfg(test)]
mod tests {
    use lectio_core::ink::{InkColor, InkStroke, InkTool};
    use lectio_core::material::{Attachment, AttachmentKind, MediaKind, SlideLink};
    use lectio_core::paper::{PaperOptions, document};
    use lectio_core::{Library, Recording};

    use super::*;

    const SLIDES: &[u8] = include_bytes!("../../../assets/sample-slides.pdf");

    fn photo_png() -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(2_000, 1_000, image::Rgba([20, 120, 200, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn every_picture_a_session_needs_is_made_from_its_stored_files() {
        let (lib, course) = Library::default().with_course("C").unwrap();
        let (lib, s) = lib.with_session(course, "Graphs", 0).unwrap();
        let rec = Recording { file_name: "g.wav".into(), duration_ms: 60_000, sample_rate: 16_000, cleaned_file: None };
        let lib = lib.with_recording(s, rec).unwrap();
        let slides_key = BlobKey::new_random(MediaKind::Pdf);
        let (lib, deck) = lib.with_deck(s, "W3.pdf", slides_key.clone(), 2).unwrap();
        let stroke = InkStroke { tool: InkTool::Pen, color: InkColor::Red, width: 0.004, points: vec![[0.2, 0.5], [0.8, 0.5]] };
        let lib = lib.with_ink_stroke(s, deck, 2, stroke).unwrap();
        let (lib, _) = lib.with_slide_item(s, 1_000, SlideLink { deck, page: 2 }).unwrap();
        let (lib, _) = lib.with_math_note(s, 2_000, r"\frac{1}{2}", &[]).unwrap();
        let photo_key = BlobKey::new_random(MediaKind::Png);
        let photo = Attachment { kind: AttachmentKind::Photo, blob: photo_key.clone(), width: 2_000, height: 1_000 };
        let (lib, _) = lib.with_attachment_note(s, 3_000, photo, "board", &[]).unwrap();
        let session = lib.session(s).unwrap().clone();
        let blocks = document(&lib, s, &PaperOptions::default()).unwrap();
        let mut asked = Vec::new();

        let pictures = make_all(&blocks, &session, &mut |key| {
            asked.push(key.clone());
            if *key == slides_key { Ok(SLIDES.to_vec()) } else { Ok(photo_png()) }
        });

        assert_eq!(pictures.len(), 3, "slide, formula, photo");
        assert_eq!(asked.len(), 2, "each stored file is read once");
        let slide = &pictures[&AssetKey::Slide(deck, 2)];
        assert!(slide.png.starts_with(b"\x89PNG") && slide.width == PICTURE_WIDTH);
        let photo = &pictures[&AssetKey::Image(photo_key)];
        assert_eq!((photo.width, photo.height), (640, 320), "shown at the column width, same shape");
    }

    #[test]
    fn a_file_that_cannot_be_read_leaves_only_its_picture_out() {
        let (lib, course) = Library::default().with_course("C").unwrap();
        let (lib, s) = lib.with_session(course, "S", 0).unwrap();
        let (lib, deck) = lib.with_deck(s, "W.pdf", BlobKey::new_random(MediaKind::Pdf), 2).unwrap();
        let (lib, _) = lib.with_slide_item(s, 0, SlideLink { deck, page: 1 }).unwrap();
        let (lib, _) = lib.with_math_note(s, 0, "x^2", &[]).unwrap();
        let blocks = document(&lib, s, &PaperOptions::default()).unwrap();

        let pictures = make_all(&blocks, lib.session(s).unwrap(), &mut |_| Err("gone".into()));

        assert_eq!(pictures.keys().collect::<Vec<_>>(), vec![&AssetKey::Formula("x^2".into())]);
    }
}
