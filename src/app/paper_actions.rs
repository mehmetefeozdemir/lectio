//! Phase 4b actions: formula notes (ADR-023) and the paper view's Copy,
//! Print and Export (ADR-025).
//!
//! Exporting is a small job: the session's blocks are fixed when it starts,
//! then each picture they need (formula, slide page with its ink, photo) is
//! made one per frame, so the window keeps responding on every platform
//! (the browser has no threads for this). Stored files are read with the
//! platform's `get_blob`, which may answer a few frames later.

use std::collections::{HashMap, VecDeque};

use lectio_core::material::BlobKey;
use lectio_core::paper::{AssetKey, Block, PaperOptions, assets, document, markdown};
use lectio_export::pictures::{blob_for, picture_for};
use lectio_export::{Fonts, Pictures};

use super::*;
use crate::media::fonts;
use crate::platform::{DocFormat, Pending};

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
    /// Stored files read so far (a deck is read once for all its pages).
    files: HashMap<BlobKey, Vec<u8>>,
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
            files: HashMap::new(),
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

    /// Makes a picture with `lectio_export::pictures` (shared with the mobile
    /// apps), or first asks for the stored file it needs. A picture that can't
    /// be made is left out; its text stays.
    fn make_picture(&mut self, job: &mut ExportJob, key: AssetKey) {
        let Some(session) = self.library.session(job.session) else { return };
        let made = match blob_for(&key, session) {
            None => picture_for(&key, session, None),
            Some(blob) => match job.files.get(&blob) {
                Some(bytes) => picture_for(&key, session, Some(bytes)),
                None => {
                    if let Some(platform) = self.platform.as_ref() {
                        job.waiting = Some((key, platform.get_blob(&blob)));
                    }
                    return;
                }
            },
        };
        if let Ok(picture) = made {
            job.pictures.insert(key, picture);
        }
    }

    /// A stored file arrived: keep it and make the picture next frame.
    fn loaded(&mut self, job: &mut ExportJob, key: AssetKey, result: Result<Vec<u8>, String>) {
        let Ok(bytes) = result else { return };
        let Some(blob) = self.library.session(job.session).and_then(|s| blob_for(&key, s)) else { return };
        job.files.insert(blob, bytes);
        job.todo.push_front(key);
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

}
