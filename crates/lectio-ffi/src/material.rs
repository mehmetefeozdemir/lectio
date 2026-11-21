//! Recordings, slides, ink and photos for the mobile apps. Files are stored
//! where the desktop stores them (`recordings/<session>.wav`, `blobs/`), and
//! imports are checked by the same rules (real file type, size limits).

use lectio_core::ink::InkStroke;
use lectio_core::material::{Attachment, AttachmentKind, BlobKey, MediaKind, SlideLink, check_file};
use lectio_core::transcript::{Segment, Transcript};
use lectio_core::{DeckId, ModelError, NoteId, Recording, SessionId};
use lectio_export::ink::{alpha, relative_width};

use crate::library::LectioLibrary;
use crate::{InkColor, InkStrokeInfo, InkTool, LectioError, Point, Result, SegmentInfo, id};

/// Recordings are stored at 16 kHz (ADR-009), as on the desktop.
pub const SAMPLE_RATE: u32 = 16_000;
/// The eraser's reach, as a fraction of the page (the desktop's value).
const ERASER_RADIUS: f32 = 0.015;
const PEN_WIDTH: f32 = 0.003;
const HIGHLIGHTER_WIDTH: f32 = 0.004;

fn ink_info(stroke: &InkStroke) -> InkStrokeInfo {
    InkStrokeInfo {
        tool: stroke.tool.into(),
        color: stroke.color.into(),
        // A fraction of the page width; not stroke_width(.., 1.0), whose
        // one-pixel minimum made every stroke as wide as the page (P-073).
        drawn_width: relative_width(stroke),
        opacity: f32::from(alpha(stroke)) / 255.0,
        points: stroke.points.iter().map(|[x, y]| Point { x: *x, y: *y }).collect(),
    }
}

#[uniffi::export]
impl LectioLibrary {
    // ---------- recordings ----------

    /// Where the session's recording goes: `recordings/<session-id>.wav`.
    pub fn recording_path(&self, session_id: String) -> Result<String> {
        let session = id::<SessionId>(&session_id)?;
        self.read(|inner| Ok(inner.store.audio_path(&format!("{session}.wav")).to_string_lossy().into_owned()))
    }

    /// Records that the session's WAV is finished and how long it is.
    pub fn attach_recording(&self, session_id: String, duration_ms: u64) -> Result<()> {
        let session = id::<SessionId>(&session_id)?;
        let recording = Recording { file_name: format!("{session}.wav"), duration_ms, sample_rate: SAMPLE_RATE, cleaned_file: None };
        self.update(|lib| lib.with_recording(session, recording))
    }

    /// Stores segments from the speech engine (merged with any already there).
    pub fn store_transcript(&self, session_id: String, segments: Vec<SegmentInfo>, language: String, model: String, complete: bool) -> Result<()> {
        let session = id::<SessionId>(&session_id)?;
        self.update(|lib| {
            let existing = lib.session(session).and_then(|s| s.transcript.as_ref()).map(|t| t.segments.clone()).unwrap_or_default();
            let new: Vec<Segment> = segments.into_iter().map(|s| Segment { start_ms: s.start_ms, end_ms: s.end_ms, text: s.text }).collect();
            let merged = lectio_core::transcript::merged(&existing, &new);
            lib.with_transcript(session, Transcript { language: language.clone(), model: model.clone(), segments: merged, complete })
        })
    }

    // ---------- slides ----------

    /// Imports a PDF: checked, page count read, text read for search.
    pub fn import_slides(&self, session_id: String, file_name: String, bytes: Vec<u8>) -> Result<String> {
        let session = id::<SessionId>(&session_id)?;
        check_file(&bytes, &[MediaKind::Pdf])?;
        let pdf = lectio_export::slides::open(bytes.clone())?;
        let page_count = lectio_export::slides::page_count(&bytes)?;
        let text = lectio_export::slides::all_text(&pdf);
        let blob = BlobKey::new_random(MediaKind::Pdf);
        self.read(|inner| Ok(inner.blobs.put(&blob, &bytes)?))?;
        self.change(|lib| {
            let (lib, deck) = lib.with_deck(session, &file_name, blob.clone(), page_count)?;
            Ok((lib.with_slide_text(session, deck, text)?, deck))
        })
        .map(|deck| deck.to_string())
    }

    /// Links a note to a page (`deck_id` = `None` removes the link).
    pub fn link_note_to_slide(&self, session_id: String, note_id: String, deck_id: Option<String>, page: u32) -> Result<()> {
        let (session, note) = (id::<SessionId>(&session_id)?, id::<NoteId>(&note_id)?);
        let link = deck_id.map(|d| id::<DeckId>(&d).map(|deck| SlideLink { deck, page })).transpose()?;
        self.update(|lib| lib.with_slide_link(session, note, link))
    }

    /// Posts a slide to the feed at `offset_ms`.
    pub fn post_slide(&self, session_id: String, offset_ms: u64, deck_id: String, page: u32) -> Result<String> {
        let (session, deck) = (id::<SessionId>(&session_id)?, id::<DeckId>(&deck_id)?);
        self.change(|lib| lib.with_slide_item(session, offset_ms, SlideLink { deck, page })).map(|id| id.to_string())
    }

    // ---------- ink ----------

    pub fn ink(&self, session_id: String, deck_id: String, page: u32) -> Result<Vec<InkStrokeInfo>> {
        let (session, deck) = (id::<SessionId>(&session_id)?, id::<DeckId>(&deck_id)?);
        self.read(|inner| {
            let s = inner.library.session(session).ok_or(ModelError::SessionNotFound)?;
            let d = s.decks.iter().find(|d| d.id == deck).ok_or(ModelError::DeckNotFound)?;
            Ok(d.ink_on(page).iter().map(ink_info).collect())
        })
    }

    pub fn add_ink_stroke(&self, session_id: String, deck_id: String, page: u32, tool: InkTool, color: InkColor, points: Vec<Point>) -> Result<()> {
        let (session, deck) = (id::<SessionId>(&session_id)?, id::<DeckId>(&deck_id)?);
        let width = if tool == InkTool::Highlighter { HIGHLIGHTER_WIDTH } else { PEN_WIDTH };
        let stroke = InkStroke { tool: tool.into(), color: color.into(), width, points: points.iter().map(|p| [p.x, p.y]).collect() };
        self.update(|lib| lib.with_ink_stroke(session, deck, page, stroke))
    }

    /// Removes the strokes the eraser touches at `at`; returns how many.
    pub fn erase_ink(&self, session_id: String, deck_id: String, page: u32, at: Point) -> Result<u32> {
        let (session, deck) = (id::<SessionId>(&session_id)?, id::<DeckId>(&deck_id)?);
        self.change(|lib| lib.with_ink_erased(session, deck, page, [at.x, at.y], ERASER_RADIUS)).map(|n| n as u32)
    }

    pub fn undo_ink(&self, session_id: String, deck_id: String, page: u32) -> Result<()> {
        let (session, deck) = (id::<SessionId>(&session_id)?, id::<DeckId>(&deck_id)?);
        self.update(|lib| lib.without_last_stroke(session, deck, page))
    }

    pub fn clear_ink(&self, session_id: String, deck_id: String, page: u32) -> Result<()> {
        let (session, deck) = (id::<SessionId>(&session_id)?, id::<DeckId>(&deck_id)?);
        self.update(|lib| lib.without_page_ink(session, deck, page))
    }

    // ---------- photos ----------

    /// A photo note from a camera or library image (PNG or JPEG bytes).
    pub fn add_photo_note(&self, session_id: String, offset_ms: u64, bytes: Vec<u8>, caption: String) -> Result<String> {
        let session = id::<SessionId>(&session_id)?;
        let kind = check_file(&bytes, &[MediaKind::Png, MediaKind::Jpeg])?;
        let (width, height) = lectio_export::images::upright_size(&bytes).map_err(LectioError::Failed)?;
        let blob = BlobKey::new_random(kind);
        self.read(|inner| Ok(inner.blobs.put(&blob, &bytes)?))?;
        let attachment = Attachment { kind: AttachmentKind::Photo, blob, width, height };
        self.change(|lib| lib.with_attachment_note(session, offset_ms, attachment, &caption, &[])).map(|id| id.to_string())
    }
}
