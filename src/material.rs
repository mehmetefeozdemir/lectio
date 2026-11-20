//! Phase 2 state that isn't part of the saved library: files being imported,
//! which slide is shown, and caches of rendered slide pages and decoded
//! images (as GPU textures). All of it can be rebuilt from the library and
//! the blob store, so none of it is saved.

use std::collections::HashMap;

use eframe::egui::{self, TextureHandle, TextureOptions};
use lectio_core::material::{BlobKey, SlideLink};
use lectio_core::{DeckId, NoteKind, Session, SessionId};

use crate::media::images::decode_for_display;
use crate::media::slides::DeckRenderer;
use crate::platform::{CameraSession, Pending, PickedFile};

/// Longest side of a photo/sketch texture, in pixels.
const IMAGE_MAX_SIDE: u32 = 1600;
/// Decoding a big JPEG takes a while (especially in the browser), so at most
/// this many images are decoded per frame.
const DECODES_PER_FRAME: usize = 2;

/// Loads a blob's bytes (from the platform, or from built-in sample data).
pub type BlobLoader<'a> = &'a dyn Fn(&BlobKey) -> Pending<Vec<u8>>;

pub enum Import {
    Slides { session: SessionId, file: Pending<Option<PickedFile>> },
    /// `offset_ms` is when "Add photo" was clicked, so the note lands where the
    /// student was in the lecture, not when the dialog closed.
    Photo { session: SessionId, offset_ms: u64, file: Pending<Option<PickedFile>> },
}

/// A finished file pick, ready to be stored and added to the library.
pub enum Picked {
    Slides { session: SessionId, file: PickedFile },
    Photo { session: SessionId, offset_ms: u64, file: PickedFile },
}

pub struct Viewer {
    pub deck: Option<DeckId>,
    /// 1-based.
    pub page: u32,
    /// New notes get a link to the slide being shown.
    pub link_new_notes: bool,
}

impl Default for Viewer {
    fn default() -> Self {
        Self { deck: None, page: 1, link_new_notes: true }
    }
}

pub enum ImageState {
    Loading(Pending<Vec<u8>>),
    Decoding(Vec<u8>),
    Ready(TextureHandle),
    Failed(String),
}

enum DeckSource {
    Loading(Pending<Vec<u8>>),
    Ready(DeckRenderer),
    Failed(String),
}

struct DeckView {
    source: DeckSource,
    pages: HashMap<u32, Result<TextureHandle, String>>,
}

/// What the slide panel can show for the current page.
pub enum PageView<'a> {
    Ready(&'a TextureHandle),
    Loading,
    Failed(&'a str),
}

pub struct CameraState {
    pub session: Box<dyn CameraSession>,
    pub preview: Option<TextureHandle>,
    pub problem: Option<String>,
}

#[derive(Default)]
pub struct MaterialState {
    imports: Vec<Import>,
    pub viewer: Viewer,
    pub reading: bool,
    pub camera: Option<CameraState>,
    images: HashMap<BlobKey, ImageState>,
    decks: HashMap<DeckId, DeckView>,
    /// A page the session shows is still being rendered.
    pages_missing: bool,
    /// A deck's text is still being read (keeps frames coming in the browser).
    text_wanted: bool,
    texts: Vec<(DeckId, Vec<String>)>,
}

impl MaterialState {
    pub fn is_importing(&self) -> bool {
        !self.imports.is_empty()
    }

    pub fn start_import(&mut self, import: Import) {
        self.imports.push(import);
    }

    /// Imports whose file pick has finished (or failed / was cancelled).
    pub fn finished_imports(&mut self) -> Vec<Result<Picked, String>> {
        let mut done = Vec::new();
        self.imports.retain(|import| {
            let (result, session, offset) = match import {
                Import::Slides { file, session } => (file.take(), *session, None),
                Import::Photo { file, session, offset_ms } => (file.take(), *session, Some(*offset_ms)),
            };
            let Some(result) = result else { return true };
            match (result, offset) {
                (Ok(None), _) => {} // cancelled
                (Ok(Some(file)), None) => done.push(Ok(Picked::Slides { session, file })),
                (Ok(Some(file)), Some(offset_ms)) => done.push(Ok(Picked::Photo { session, offset_ms, file })),
                (Err(e), _) => done.push(Err(e)),
            }
            false
        });
        done
    }

    /// Resets the viewer for a newly opened session: first deck, page 1.
    pub fn open_session(&mut self, session: &Session) {
        self.viewer = Viewer { deck: session.decks.first().map(|d| d.id), ..Viewer::default() };
        self.reading = false;
    }

    pub fn show_deck(&mut self, deck: DeckId, page: u32) {
        self.viewer.deck = Some(deck);
        self.viewer.page = page.max(1);
    }

    /// The link a new note gets, if "link new notes" is on and a deck is shown.
    pub fn current_link(&self, session: &Session) -> Option<SlideLink> {
        self.shown_slide(session).filter(|_| self.viewer.link_new_notes)
    }

    /// The slide shown right now (for "link this note to the shown slide").
    pub fn shown_slide(&self, session: &Session) -> Option<SlideLink> {
        let deck = self.viewer.deck.filter(|d| session.decks.iter().any(|x| x.id == *d))?;
        Some(SlideLink { deck, page: self.viewer.page })
    }

    /// Bytes that are already in memory (just imported) skip a storage round trip.
    pub fn seed_image(&mut self, key: BlobKey, bytes: Vec<u8>) {
        self.images.insert(key, ImageState::Decoding(bytes));
    }

    pub fn seed_deck(&mut self, deck: DeckId, bytes: Vec<u8>) {
        let source = DeckSource::Ready(DeckRenderer::start(bytes));
        self.decks.insert(deck, DeckView { source, pages: HashMap::new() });
    }

    pub fn forget_deck(&mut self, deck: DeckId) {
        self.decks.remove(&deck);
        if self.viewer.deck == Some(deck) {
            self.viewer.deck = None;
        }
    }

    pub fn forget_image(&mut self, key: &BlobKey) {
        self.images.remove(key);
    }

    pub fn image(&self, key: &BlobKey) -> Option<&ImageState> {
        self.images.get(key)
    }

    pub fn page(&self, deck: DeckId, page: u32) -> PageView<'_> {
        let Some(view) = self.decks.get(&deck) else { return PageView::Loading };
        if let DeckSource::Failed(e) = &view.source {
            return PageView::Failed(e);
        }
        match view.pages.get(&page) {
            Some(Ok(texture)) => PageView::Ready(texture),
            Some(Err(e)) => PageView::Failed(e),
            None => PageView::Loading,
        }
    }

    /// Starts loading whatever the open session needs to show: its note
    /// images, the shown slide page, and the pages that slide items in the
    /// feed show as thumbnails.
    pub fn ensure_loaded(&mut self, session: &Session, load: BlobLoader<'_>) {
        for attachment in session.notes.iter().filter_map(|n| n.attachment.as_ref()) {
            self.images
                .entry(attachment.blob.clone())
                .or_insert_with(|| ImageState::Loading(load(&attachment.blob)));
        }
        let shown = self.shown_slide(session);
        let posted = session.notes.iter().filter(|n| n.kind == NoteKind::Slide).filter_map(|n| n.slide);
        // The paper view also shows the slides notes refer to (Phase 4b).
        let reading = self.reading;
        let linked = session.notes.iter().filter(|n| reading && n.kind != NoteKind::Slide).filter_map(|n| n.slide);
        self.pages_missing = false;
        let links: Vec<SlideLink> = shown.into_iter().chain(posted).chain(linked).collect();
        for link in links {
            self.request_page(session, link, load);
        }
        // Decks without their page text yet (imported before Phase 3, or just now).
        self.text_wanted = false;
        for deck in session.decks.iter().filter(|d| d.page_text.is_empty()) {
            let view = self
                .decks
                .entry(deck.id)
                .or_insert_with(|| DeckView { source: DeckSource::Loading(load(&deck.blob)), pages: HashMap::new() });
            if let DeckSource::Ready(renderer) = &mut view.source {
                renderer.request_text();
            }
            self.text_wanted = true;
        }
    }

    /// Page text read since the last call, per deck (to store in the library).
    pub fn take_texts(&mut self) -> Vec<(DeckId, Vec<String>)> {
        std::mem::take(&mut self.texts)
    }

    fn request_page(&mut self, session: &Session, link: SlideLink, load: BlobLoader<'_>) {
        let Some(deck) = session.decks.iter().find(|x| x.id == link.deck) else { return };
        let view = self
            .decks
            .entry(deck.id)
            .or_insert_with(|| DeckView { source: DeckSource::Loading(load(&deck.blob)), pages: HashMap::new() });
        if let DeckSource::Ready(renderer) = &mut view.source
            && !view.pages.contains_key(&link.page)
        {
            renderer.request(link.page);
            self.pages_missing = true;
        }
    }

    /// Turns finished loads and renders into textures. Call once per frame.
    /// Returns true while work is still in flight (so the UI keeps redrawing).
    pub fn update_textures(&mut self, ctx: &egui::Context) -> bool {
        let images_busy = self.update_images(ctx);
        let decks_busy = self.update_decks(ctx);
        let camera_busy = self.update_camera(ctx);
        images_busy || decks_busy || camera_busy || self.is_importing()
    }

    fn update_images(&mut self, ctx: &egui::Context) -> bool {
        let mut decoded = 0;
        let mut busy = false;
        for (key, state) in self.images.iter_mut() {
            if let ImageState::Loading(pending) = state {
                match pending.take() {
                    Some(Ok(bytes)) => *state = ImageState::Decoding(bytes),
                    Some(Err(e)) => *state = ImageState::Failed(e),
                    None => busy = true,
                }
            }
            if let ImageState::Decoding(bytes) = state {
                if decoded == DECODES_PER_FRAME {
                    busy = true;
                    continue;
                }
                decoded += 1;
                *state = match decode_for_display(bytes, IMAGE_MAX_SIDE) {
                    Ok(image) => ImageState::Ready(ctx.load_texture(key.as_str(), image, TextureOptions::LINEAR)),
                    Err(e) => ImageState::Failed(e),
                };
            }
        }
        busy
    }

    fn update_decks(&mut self, ctx: &egui::Context) -> bool {
        let mut busy = false;
        for (deck, view) in self.decks.iter_mut() {
            if let DeckSource::Loading(pending) = &view.source {
                match pending.take() {
                    Some(Ok(bytes)) => view.source = DeckSource::Ready(DeckRenderer::start(bytes)),
                    Some(Err(e)) => view.source = DeckSource::Failed(e),
                    None => busy = true,
                }
            }
            if let DeckSource::Ready(renderer) = &mut view.source {
                for (page, rendered) in renderer.poll() {
                    let name = format!("{deck}-page-{page}");
                    let texture = rendered.map(|image| ctx.load_texture(name, image, TextureOptions::LINEAR));
                    view.pages.insert(page, texture);
                }
                if let Some(text) = renderer.take_text() {
                    self.texts.push((*deck, text));
                }
                busy |= self.viewer.deck == Some(*deck) && !view.pages.contains_key(&self.viewer.page);
            }
        }
        busy || self.pages_missing || self.text_wanted
    }

    fn update_camera(&mut self, ctx: &egui::Context) -> bool {
        let Some(camera) = &mut self.camera else { return false };
        if let Some(problem) = camera.session.take_error() {
            camera.problem = Some(problem);
        }
        if let Some(frame) = camera.session.preview() {
            match &mut camera.preview {
                Some(texture) => texture.set(frame, TextureOptions::LINEAR),
                None => camera.preview = Some(ctx.load_texture("camera-preview", frame, TextureOptions::LINEAR)),
            }
        }
        true
    }
}
