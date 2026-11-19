//! Phase 2 actions on `LectioApp`: importing slides and photos, the camera,
//! sketches, the slide viewer and the reading view. A child module of
//! `app.rs`, so it can use the app's private fields.

use lectio_core::material::{Attachment, AttachmentKind, BlobKey, MediaKind, check_file};

use super::*;
use crate::material::{CameraState, Import, Picked};
use crate::media::sketch::{SKETCH_HEIGHT, SKETCH_WIDTH, StrokePoints, rasterize_png};
use crate::media::{images, slides};
use crate::platform::{FileKind, PickedFile};
use crate::ui::SketchPad;

const NEEDS_TIMELINE: &str = "Start recording (or open a recorded session) to add photos and sketches to its timeline.";

/// One image to store as a note.
pub(super) struct NewImage {
    pub(super) kind: AttachmentKind,
    pub(super) media: MediaKind,
    pub(super) bytes: Vec<u8>,
    pub(super) width: u32,
    pub(super) height: u32,
}

impl LectioApp {
    pub(super) fn apply_material(&mut self, action: Action) {
        match action {
            Action::ImportSlides => self.start_import(FileKind::Slides),
            Action::AddPhotoFromFile => self.start_import(FileKind::Photo),
            Action::ShowDeck(deck) => self.material.show_deck(deck, 1),
            Action::TurnSlide(delta) => self.turn_slide(delta),
            Action::GoToSlide(link) => self.material.show_deck(link.deck, link.page),
            Action::SetLinkNewNotes(on) => self.material.viewer.link_new_notes = on,
            Action::LinkNote(note, link) => self.change_session(|lib, s| lib.with_slide_link(s, note, link)),
            Action::RemoveDeck(deck) => self.remove_deck_with_undo(deck),
            Action::OpenCamera => self.open_camera(),
            Action::TakePhoto => self.take_photo(),
            Action::CloseCamera => self.material.camera = None,
            Action::OpenSketch if self.can_take_notes() => {
                self.buffers.sketch = Some(SketchPad::new(self.current_offset_ms()));
            }
            Action::OpenSketch => self.show_error(NEEDS_TIMELINE),
            Action::SaveSketch { strokes, caption, offset_ms } => self.save_sketch(&strokes, &caption, offset_ms),
            Action::CloseSketch => self.buffers.sketch = None,
            Action::SetReading(on) => self.material.reading = on,
            _ => {} // Phase 1 actions are handled in `apply`.
        }
    }

    /// Per-frame Phase 2 upkeep. Returns true while something is loading.
    pub(super) fn tick_material(&mut self, ctx: &egui::Context) -> bool {
        let before = self.library.clone();
        for finished in self.material.finished_imports() {
            match finished {
                Ok(picked) => self.finish_import(picked),
                Err(e) => self.show_error(e),
            }
        }
        if self.library != before {
            self.persist();
        }
        if let Some(session) = self.selected.and_then(|id| self.library.session(id)) {
            let (platform, samples) = (self.platform.as_deref(), &self.sample_blobs);
            let load = |key: &BlobKey| load_blob_from(platform, samples, key);
            self.material.ensure_loaded(session, &load);
        }
        let busy = self.material.update_textures(ctx);
        self.store_slide_texts();
        busy
    }

    /// Saves the page text read from decks (for search, ADR-018).
    fn store_slide_texts(&mut self) {
        let Some(session) = self.selected else { return };
        let texts = self.material.take_texts();
        if texts.is_empty() {
            return;
        }
        for (deck, text) in texts {
            // A deck with no readable text still gets one (empty) entry per
            // page, so it isn't read again on every visit.
            if let Ok(library) = self.library.with_slide_text(session, deck, text) {
                self.library = library;
            }
        }
        self.persist();
    }

    fn start_import(&mut self, kind: FileKind) {
        let Some(session) = self.selected else { return };
        let Some(platform) = self.platform.as_ref().filter(|_| self.can_save) else {
            return self.show_error("Importing files is turned off in this window.");
        };
        if kind == FileKind::Photo && !self.can_take_notes() {
            return self.show_error(NEEDS_TIMELINE);
        }
        let file = platform.pick_file(kind);
        let import = match kind {
            FileKind::Slides => Import::Slides { session, file },
            FileKind::Photo => Import::Photo { session, offset_ms: self.current_offset_ms(), file },
        };
        self.material.start_import(import);
    }

    fn finish_import(&mut self, picked: Picked) {
        let result = match picked {
            Picked::Slides { session, file } => self.add_deck(session, file),
            Picked::Photo { session, offset_ms, file } => self.add_photo_file(session, offset_ms, file),
        };
        if let Err(e) = result {
            self.show_error(e);
        }
    }

    fn add_deck(&mut self, session: SessionId, file: PickedFile) -> Result<(), String> {
        check_file(&file.bytes, &[MediaKind::Pdf]).map_err(|e| e.to_string())?;
        let pages = slides::page_count(&file.bytes)?;
        let key = BlobKey::new_random(MediaKind::Pdf);
        self.store_blob(&key, file.bytes.clone())?;
        let (library, deck) = self.library.with_deck(session, &file.name, key, pages).map_err(|e| e.to_string())?;
        self.library = library;
        self.material.seed_deck(deck, file.bytes);
        if self.selected == Some(session) {
            self.material.show_deck(deck, 1);
        }
        Ok(())
    }

    fn add_photo_file(&mut self, session: SessionId, offset_ms: u64, file: PickedFile) -> Result<(), String> {
        let media = check_file(&file.bytes, &[MediaKind::Png, MediaKind::Jpeg]).map_err(|e| e.to_string())?;
        let (width, height) = images::upright_size(&file.bytes)?;
        let image = NewImage { kind: AttachmentKind::Photo, media, bytes: file.bytes, width, height };
        self.add_image_note(session, offset_ms, image, "")
    }

    fn open_camera(&mut self) {
        if !self.can_take_notes() {
            return self.show_error(NEEDS_TIMELINE);
        }
        match self.platform.as_ref().and_then(|p| p.open_camera()) {
            Some(session) => self.material.camera = Some(CameraState { session, preview: None, problem: None }),
            None => self.show_error("This version of Lectio can't use a camera. Use \"Add photo…\" instead."),
        }
    }

    fn take_photo(&mut self) {
        let (Some(session), Some(camera)) = (self.selected, self.material.camera.as_mut()) else { return };
        let shot = match camera.session.snapshot() {
            Ok(shot) => shot,
            Err(e) => {
                camera.problem = Some(e);
                return;
            }
        };
        let image = NewImage {
            kind: AttachmentKind::Photo,
            media: MediaKind::Jpeg,
            bytes: shot.jpeg,
            width: shot.width,
            height: shot.height,
        };
        let offset = self.current_offset_ms();
        match self.add_image_note(session, offset, image, "") {
            Ok(()) => self.material.camera = None,
            Err(e) => self.show_error(e),
        }
    }

    fn save_sketch(&mut self, strokes: &[StrokePoints], caption: &str, offset_ms: u64) {
        let Some(session) = self.selected else { return };
        let saved = rasterize_png(strokes).and_then(|png| {
            let image = NewImage {
                kind: AttachmentKind::Sketch,
                media: MediaKind::Png,
                bytes: png,
                width: SKETCH_WIDTH,
                height: SKETCH_HEIGHT,
            };
            self.add_image_note(session, offset_ms, image, caption)
        });
        match saved {
            Ok(()) => self.buffers.sketch = None,
            Err(e) => self.show_error(e),
        }
    }

    /// Stores the image as a blob first, then adds the note that refers to it,
    /// so a note never points at a file that doesn't exist (ADR-012).
    pub(super) fn add_image_note(
        &mut self,
        session: SessionId,
        offset_ms: u64,
        image: NewImage,
        caption: &str,
    ) -> Result<(), String> {
        let key = BlobKey::new_random(image.media);
        self.store_blob(&key, image.bytes.clone())?;
        let attachment = Attachment { kind: image.kind, blob: key.clone(), width: image.width, height: image.height };
        let added = self.library.with_attachment_note(session, offset_ms, attachment, caption, &[]);
        if self.adopt_new_note(session, added) {
            self.material.seed_image(key, image.bytes);
        }
        Ok(())
    }

    fn store_blob(&self, key: &BlobKey, bytes: Vec<u8>) -> Result<(), String> {
        let platform = self.platform.as_ref().ok_or("Files can't be stored in this window.")?;
        platform.put_blob(key, bytes).map_err(|e| format!("The file could not be stored: {e}"))
    }

    fn turn_slide(&mut self, delta: i32) {
        let Some(session) = self.selected.and_then(|id| self.library.session(id)) else { return };
        let Some(deck) = self.material.viewer.deck.and_then(|d| session.decks.iter().find(|x| x.id == d)) else {
            return;
        };
        let page = self.material.viewer.page.saturating_add_signed(delta);
        self.material.viewer.page = page.clamp(1, deck.page_count);
    }
}

/// Bytes of a stored file: built-in sample files first, then the platform.
fn load_blob_from(
    platform: Option<&dyn Platform>,
    samples: &HashMap<BlobKey, Vec<u8>>,
    key: &BlobKey,
) -> Pending<Vec<u8>> {
    match (samples.get(key), platform) {
        (Some(bytes), _) => Pending::ready(Ok(bytes.clone())),
        (None, Some(platform)) => platform.get_blob(key),
        (None, None) => Pending::ready(Err("This file is not available here.".into())),
    }
}
