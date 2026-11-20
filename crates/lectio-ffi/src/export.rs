//! Exports for the share sheet (ADR-025): the same paper-view blocks,
//! pictures and writers as the desktop, made in one call. Swift calls it from
//! a background task (a deck with many slides takes a few seconds).

use lectio_core::paper::{PaperOptions, document, markdown};
use lectio_core::{ModelError, SessionId};
use lectio_export::Fonts;
use lectio_export::fonts::{INTER_REGULAR, INTER_SEMIBOLD};
use lectio_export::pictures::make_all;

use crate::library::LectioLibrary;
use crate::{ExportFormat, Result, id};

fn extension(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Markdown => "md",
        ExportFormat::Pdf => "pdf",
        ExportFormat::Docx => "docx",
    }
}

#[uniffi::export]
impl LectioLibrary {
    /// The session as a document: Markdown, PDF or Word, with times shown and
    /// the transcript added at the end if asked.
    pub fn export(&self, session_id: String, format: ExportFormat, include_transcript: bool) -> Result<Vec<u8>> {
        let session = id::<SessionId>(&session_id)?;
        let options = PaperOptions { times: true, transcript: include_transcript };
        self.read(|inner| {
            let blocks = document(&inner.library, session, &options)?;
            if format == ExportFormat::Markdown {
                return Ok(markdown(&blocks, &options).into_bytes());
            }
            let s = inner.library.session(session).ok_or(ModelError::SessionNotFound)?;
            let pictures = make_all(&blocks, s, &mut |key| inner.blobs.get(key).map_err(|e| e.to_string()));
            let fonts = Fonts { regular: INTER_REGULAR.to_vec(), bold: INTER_SEMIBOLD.to_vec() };
            let bytes = match format {
                ExportFormat::Pdf => lectio_export::pdf(&blocks, &options, &pictures, &fonts)?,
                _ => lectio_export::docx(&blocks, &options, &pictures)?,
            };
            Ok(bytes)
        })
    }

    /// "<session title>.<ext>", without characters file systems refuse.
    pub fn export_file_name(&self, session_id: String, format: ExportFormat) -> Result<String> {
        let session = id::<SessionId>(&session_id)?;
        self.read(|inner| {
            let title = inner.library.session(session).ok_or(ModelError::SessionNotFound)?.title.clone();
            let safe = title.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "-");
            Ok(format!("{safe}.{}", extension(format)))
        })
    }
}
