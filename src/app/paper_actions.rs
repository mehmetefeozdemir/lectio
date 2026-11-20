//! Phase 4b actions: formula notes (ADR-023) and the paper view's Copy,
//! Print and Export (ADR-025).
//!
//! Exporting is a small job: the session's blocks are fixed when it starts,
//! then each picture they need (formula, slide page with its ink, photo) is
//! made one per frame, so the window keeps responding on every platform
//! (the browser has no threads for this). Stored files are read with the
//! platform's `get_blob`, which may answer a few frames later.

use std::collections::{HashMap, VecDeque};

use lectio_core::DeckId;
use lectio_core::paper::{AssetKey, Block, PaperOptions, assets, document, markdown};
use lectio_export::{Fonts, Picture, Pictures};

use super::*;
use crate::media::{fonts, images, ink, math, slides};
use crate::platform::{DocFormat, Pending};

/// Formulas are drawn at twice their shown size, for sharp printing.
const FORMULA_PX: f32 = 40.0;
/// Pictures are shown at most this wide in exports (CSS pixels).
const PICTURE_WIDTH: u32 = 640;
/// Photos are scaled down to this before they go into a document.
const PHOTO_MAX_SIDE: u32 = 1_600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportTarget {
    File,
    Print,
}

pub struct ExportJob {
    target: ExportTarget,
    format: DocFormat,
    session: SessionId,
    title: String,
    blocks: Vec<Block>,
    options: PaperOptions,
    todo: VecDeque<AssetKey>,
    waiting: Option<(AssetKey, Pending<Vec<u8>>)>,
    decks: HashMap<DeckId, Vec<u8>>,
    pictures: Pictures,
    total: usize,
}

impl ExportJob {
    /// (pictures made, pictures needed), for the progress bar.
    pub fn progress(&self) -> (usize, usize) {
        (self.total - self.todo.len(), self.total)
    }

    pub fn format(&self) -> DocFormat {
        self.format
    }
}

#[derive(Default)]
pub struct PaperState {
    pub formula_mode: bool,
    pub options: PaperOptions,
    pub job: Option<ExportJob>,
}

/// A picture of `width × height` pixels shown at most `PICTURE_WIDTH` wide.
fn shown(png: Vec<u8>, width: u32, height: u32, max: u32) -> Picture {
    let w = width.min(max).max(1);
    Picture { png, width: w, height: (u64::from(height) * u64::from(w) / u64::from(width.max(1))) as u32 }
}

fn file_name(title: &str, format: DocFormat) -> String {
    format!("{}.{}", title.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "-"), format.extension())
}

impl LectioApp {
    /// Handles formula and paper-view actions; returns any other action unchanged.
    pub(super) fn apply_paper(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::SetFormulaMode(on) => self.paper.formula_mode = on,
            Action::SetPaperTimes(on) => self.paper.options.times = on,
            Action::SetPaperTranscript(on) => self.paper.options.transcript = on,
            Action::Export(format) => self.start_export(ExportTarget::File, format),
            Action::Print => {
                let format = self.platform.as_ref().map_or(DocFormat::Pdf, |p| p.print_format());
                self.start_export(ExportTarget::Print, format);
            }
            other => return Some(other),
        }
        None
    }

    /// The paper view's text for Copy (Markdown, with the chosen options).
    pub(super) fn paper_markdown(&self) -> Option<String> {
        let session = self.selected?;
        document(&self.library, session, &self.paper.options).ok().map(|blocks| markdown(&blocks, &self.paper.options))
    }

    fn start_export(&mut self, target: ExportTarget, format: DocFormat) {
        if self.platform.is_none() || !self.can_save {
            return self.show_error("Exporting is turned off in this window (sample data). Use Copy instead.");
        }
        if self.paper.job.is_some() {
            return self.show_error("An export is already being prepared.");
        }
        let Some(session) = self.selected else { return };
        let blocks = match document(&self.library, session, &self.paper.options) {
            Ok(blocks) => blocks,
            Err(e) => return self.show_error(e),
        };
        let todo: VecDeque<AssetKey> = if format == DocFormat::Markdown { VecDeque::new() } else { assets(&blocks).into() };
        let title = self.library.session(session).map_or_else(|| "Lectio notes".to_owned(), |s| s.title.clone());
        let total = todo.len();
        self.paper.job = Some(ExportJob {
            target,
            format,
            session,
            title,
            blocks,
            options: self.paper.options,
            todo,
            waiting: None,
            decks: HashMap::new(),
            pictures: HashMap::new(),
            total,
        });
    }

    /// Per frame: makes the next picture, or writes the document when all
    /// are done. True while an export runs.
    pub(super) fn tick_paper(&mut self) -> bool {
        let Some(mut job) = self.paper.job.take() else { return false };
        if let Some((key, pending)) = job.waiting.take() {
            match pending.take() {
                None => job.waiting = Some((key, pending)),
                Some(result) => self.loaded(&mut job, key, result),
            }
        } else if let Some(key) = job.todo.pop_front() {
            self.make_picture(&mut job, key);
        } else {
            self.finish_export(job);
            return false;
        }
        self.paper.job = Some(job);
        true
    }

    fn make_picture(&mut self, job: &mut ExportJob, key: AssetKey) {
        match &key {
            AssetKey::Formula(latex) => {
                // A formula that can't be drawn keeps its LaTeX text instead.
                if let Ok((png, w, h)) = math::png(latex, FORMULA_PX) {
                    job.pictures.insert(key, Picture { png, width: w / 2, height: h / 2 });
                }
            }
            AssetKey::Slide(deck, page) => match job.decks.get(deck) {
                Some(bytes) => {
                    let strokes = self.library.session(job.session).and_then(|s| s.decks.iter().find(|d| d.id == *deck)).map(|d| d.ink_on(*page).to_vec());
                    let made = slides::render_one(bytes, *page).and_then(|image| {
                        let [w, h] = image.size;
                        ink::page_png(&image, &strokes.unwrap_or_default()).map(|png| shown(png, w as u32, h as u32, PICTURE_WIDTH))
                    });
                    if let Ok(picture) = made {
                        job.pictures.insert(key, picture);
                    }
                }
                None => self.request_blob(job, key),
            },
            AssetKey::Image(_) => self.request_blob(job, key),
        }
    }

    /// Asks for the stored file behind a slide deck or photo.
    fn request_blob(&mut self, job: &mut ExportJob, key: AssetKey) {
        let blob = match &key {
            AssetKey::Slide(deck, _) => self.library.session(job.session).and_then(|s| s.decks.iter().find(|d| d.id == *deck)).map(|d| d.blob.clone()),
            AssetKey::Image(blob) => Some(blob.clone()),
            AssetKey::Formula(_) => None,
        };
        if let (Some(blob), Some(platform)) = (blob, self.platform.as_ref()) {
            job.waiting = Some((key, platform.get_blob(&blob)));
        }
    }

    fn loaded(&mut self, job: &mut ExportJob, key: AssetKey, result: Result<Vec<u8>, String>) {
        // A missing file leaves its picture out; the text stays.
        let Ok(bytes) = result else { return };
        match key {
            AssetKey::Slide(deck, page) => {
                job.decks.insert(deck, bytes);
                job.todo.push_front(AssetKey::Slide(deck, page));
            }
            AssetKey::Image(blob) => {
                let made = images::decode_for_display(&bytes, PHOTO_MAX_SIDE).and_then(|image| {
                    let [w, h] = image.size;
                    ink::page_png(&image, &[]).map(|png| shown(png, w as u32, h as u32, PICTURE_WIDTH))
                });
                if let Ok(picture) = made {
                    job.pictures.insert(AssetKey::Image(blob), picture);
                }
            }
            AssetKey::Formula(_) => {}
        }
    }

    fn finish_export(&mut self, job: ExportJob) {
        let fonts = Fonts { regular: fonts::INTER_REGULAR.to_vec(), bold: fonts::INTER_SEMIBOLD.to_vec() };
        let bytes = match job.format {
            DocFormat::Markdown => Ok(markdown(&job.blocks, &job.options).into_bytes()),
            DocFormat::Html => Ok(lectio_export::html(&job.blocks, &job.options, &job.pictures).into_bytes()),
            DocFormat::Pdf => lectio_export::pdf(&job.blocks, &job.options, &job.pictures, &fonts),
            DocFormat::Docx => lectio_export::docx(&job.blocks, &job.options, &job.pictures),
        };
        let Some(platform) = self.platform.as_ref() else { return };
        let done = bytes.and_then(|bytes| match job.target {
            ExportTarget::File => platform.save_file(&file_name(&job.title, job.format), &bytes, job.format).map(|_| ()),
            ExportTarget::Print => platform.print(&job.title, bytes),
        });
        if let Err(e) = done {
            self.show_error(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_drop_characters_no_file_system_allows() {
        assert_eq!(file_name("Week 3: graphs/trees?", DocFormat::Pdf), "Week 3- graphs-trees-.pdf");
        assert_eq!(file_name("Notes", DocFormat::Docx), "Notes.docx");
    }

    #[test]
    fn pictures_are_shown_at_most_the_column_width_keeping_their_shape() {
        let p = shown(vec![], 1_400, 1_050, 640);
        assert_eq!((p.width, p.height), (640, 480));
        let small = shown(vec![], 300, 200, 640);
        assert_eq!((small.width, small.height), (300, 200));
    }
}
