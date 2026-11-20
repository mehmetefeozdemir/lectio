//! Phase 2 "material": PDF slide decks, links from notes to slide pages, and
//! photo/sketch notes (ADR-012).
//!
//! The library only stores *references* to big files: a `BlobKey` like
//! `3f2a…-….pdf`. The bytes live in a separate blob store (files on native,
//! IndexedDB on the web), so `library.json` stays small and fast to save.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::new_note;
use crate::{DeckId, LabelKind, Library, MAX_NAME_CHARS, ModelError, NoteId, SessionId};

const MB: u64 = 1024 * 1024;
/// Largest PDF we import. Lecture decks with photos are often 20–60 MB.
pub const MAX_PDF_BYTES: u64 = 100 * MB;
/// Largest photo we import. A 12-megapixel phone JPEG is 3–6 MB.
pub const MAX_IMAGE_BYTES: u64 = 25 * MB;
const DEFAULT_DECK_TITLE: &str = "Slides";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Pdf,
    Png,
    Jpeg,
}

impl MediaKind {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }

    fn from_extension(ext: &str) -> Option<Self> {
        [Self::Pdf, Self::Png, Self::Jpeg].into_iter().find(|k| k.extension() == ext)
    }

    /// Recognises a file by its first bytes ("magic number"), never by its
    /// name, so a renamed file can't pretend to be something else.
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"%PDF-") {
            Some(Self::Pdf)
        } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if bytes.starts_with(b"\xFF\xD8\xFF") {
            Some(Self::Jpeg)
        } else {
            None
        }
    }

    pub fn max_bytes(self) -> u64 {
        match self {
            Self::Pdf => MAX_PDF_BYTES,
            Self::Png | Self::Jpeg => MAX_IMAGE_BYTES,
        }
    }

    pub fn is_image(self) -> bool {
        matches!(self, Self::Png | Self::Jpeg)
    }
}

/// Checks an imported file: its real type must be one of `allowed`, and it
/// must be under that type's size limit.
pub fn check_file(bytes: &[u8], allowed: &[MediaKind]) -> Result<MediaKind, ModelError> {
    let kind = MediaKind::sniff(bytes).filter(|k| allowed.contains(k)).ok_or(ModelError::UnsupportedFile)?;
    if bytes.len() as u64 > kind.max_bytes() {
        return Err(ModelError::FileTooLarge { max_mb: kind.max_bytes() / MB });
    }
    Ok(kind)
}

/// The name of a stored file: `<uuid>.<pdf|png|jpg>`, nothing else.
///
/// Keys become file names on disk, so anything that could point elsewhere
/// (`../`, slashes, other extensions) is rejected when a key is created *and*
/// when one is read back from `library.json`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BlobKey(String);

impl BlobKey {
    pub fn new_random(kind: MediaKind) -> Self {
        Self(format!("{}.{}", Uuid::new_v4().hyphenated(), kind.extension()))
    }

    pub fn parse(text: &str) -> Result<Self, ModelError> {
        let (stem, ext) = text.rsplit_once('.').ok_or(ModelError::InvalidBlobKey)?;
        MediaKind::from_extension(ext).ok_or(ModelError::InvalidBlobKey)?;
        let canonical = Uuid::try_parse(stem).ok().map(|u| u.hyphenated().to_string());
        if canonical.as_deref() != Some(stem) {
            return Err(ModelError::InvalidBlobKey);
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn kind(&self) -> MediaKind {
        self.0.rsplit_once('.').and_then(|(_, ext)| MediaKind::from_extension(ext)).unwrap_or(MediaKind::Pdf)
    }
}

impl TryFrom<String> for BlobKey {
    type Error = ModelError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text)
    }
}

impl From<BlobKey> for String {
    fn from(key: BlobKey) -> Self {
        key.0
    }
}

impl std::fmt::Display for BlobKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// An imported PDF. `page_count` is read once at import time.
/// `PartialEq` only: its ink has float coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlideDeck {
    pub id: DeckId,
    pub title: String,
    pub blob: BlobKey,
    pub page_count: u32,
    /// The text on each page (index 0 = page 1), for search. Filled after
    /// import; empty for decks imported before Phase 3 until it is extracted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub page_text: Vec<String>,
    /// Drawing on the pages (Phase 4b, ADR-024); only pages with strokes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ink: Vec<crate::ink::PageInk>,
}

/// "This note is about page `page` (1-based) of deck `deck`."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlideLink {
    pub deck: DeckId,
    pub page: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Photo,
    Sketch,
}

/// The image of a photo or sketch note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub kind: AttachmentKind,
    pub blob: BlobKey,
    pub width: u32,
    pub height: u32,
}

/// The default title of a slide item.
pub fn slide_title(page: u32) -> String {
    format!("Slide {page}")
}

/// A readable deck title from a file name: `Week 3.pdf` → `Week 3`.
pub fn deck_title(file_name: &str) -> String {
    let trimmed = file_name.trim();
    let without_ext = match trimmed.len().checked_sub(4) {
        Some(cut) if trimmed.is_char_boundary(cut) && trimmed[cut..].eq_ignore_ascii_case(".pdf") => {
            &trimmed[..cut]
        }
        _ => trimmed,
    };
    let title: String = without_ext.trim().chars().take(MAX_NAME_CHARS).collect();
    if title.is_empty() { DEFAULT_DECK_TITLE.to_owned() } else { title }
}

/// Blobs referenced by `before` but no longer by `after`: safe to delete.
pub fn orphaned_blobs(before: &Library, after: &Library) -> Vec<BlobKey> {
    before.blob_keys().difference(&after.blob_keys()).cloned().collect()
}

impl Library {
    /// Every blob this library refers to (slide decks and note images).
    pub fn blob_keys(&self) -> BTreeSet<BlobKey> {
        let sessions = self.courses().iter().flat_map(|c| c.sessions.iter());
        sessions
            .flat_map(|s| {
                let decks = s.decks.iter().map(|d| d.blob.clone());
                let images = s.notes.iter().filter_map(|n| n.attachment.as_ref().map(|a| a.blob.clone()));
                decks.chain(images).collect::<Vec<_>>()
            })
            .collect()
    }

    pub fn with_deck(
        &self,
        session: SessionId,
        file_name: &str,
        blob: BlobKey,
        page_count: u32,
    ) -> Result<(Library, DeckId), ModelError> {
        if blob.kind() != MediaKind::Pdf {
            return Err(ModelError::UnsupportedFile);
        }
        if page_count == 0 {
            return Err(ModelError::PageOutOfRange { page: 1, page_count });
        }
        let deck = SlideDeck {
            id: DeckId::new_random(),
            title: deck_title(file_name),
            blob,
            page_count,
            page_text: Vec::new(),
            ink: Vec::new(),
        };
        let id = deck.id;
        let next = self.map_session(session, |s| {
            let mut changed = s.clone();
            changed.decks.push(deck);
            Ok(changed)
        })?;
        Ok((next, id))
    }

    /// Removes a deck and every note's link to it (the notes stay).
    pub fn without_deck(&self, session: SessionId, deck: DeckId) -> Result<Library, ModelError> {
        self.map_session(session, |s| {
            if !s.decks.iter().any(|d| d.id == deck) {
                return Err(ModelError::DeckNotFound);
            }
            let mut changed = s.clone();
            changed.decks.retain(|d| d.id != deck);
            for note in &mut changed.notes {
                if note.slide.is_some_and(|link| link.deck == deck) {
                    note.slide = None;
                }
            }
            Ok(changed)
        })
    }

    /// Links a note to a slide page of a deck in the same session, or
    /// removes the link (`None`). The note keeps its place on the timeline.
    pub fn with_slide_link(
        &self,
        session: SessionId,
        note: NoteId,
        link: Option<SlideLink>,
    ) -> Result<Library, ModelError> {
        if let Some(link) = link {
            let deck = self
                .session(session)
                .ok_or(ModelError::SessionNotFound)?
                .decks
                .iter()
                .find(|d| d.id == link.deck)
                .ok_or(ModelError::DeckNotFound)?;
            if link.page == 0 || link.page > deck.page_count {
                return Err(ModelError::PageOutOfRange { page: link.page, page_count: deck.page_count });
            }
        }
        self.map_note(session, note, |n| Ok(crate::Note { slide: link, ..n.clone() }))
    }

    /// Posts a slide to the feed at `offset_ms` (a slide item, titled
    /// "Slide N" until the user renames it).
    pub fn with_slide_item(&self, session: SessionId, offset_ms: u64, link: SlideLink) -> Result<(Library, NoteId), ModelError> {
        let mut item = new_note(offset_ms, &slide_title(link.page), &[], None);
        item.kind = crate::NoteKind::Slide;
        let (library, id) = self.insert_note(session, item)?;
        // Validates the page and deck the same way as any other link.
        Ok((library.with_slide_link(session, id, Some(link))?, id))
    }

    /// Edits a note's text (a slide item's title). A slide item with an empty
    /// title goes back to "Slide N"; a text note can't become empty.
    pub fn with_note_text(&self, session: SessionId, note: NoteId, text: &str) -> Result<Library, ModelError> {
        self.map_note(session, note, |n| {
            let trimmed = text.trim();
            let body = match (n.kind, n.slide) {
                (crate::NoteKind::Slide, Some(link)) if trimmed.is_empty() => slide_title(link.page),
                _ => trimmed.to_owned(),
            };
            if n.kind == crate::NoteKind::Math {
                crate::math::check(&body)?;
            }
            let changed = crate::Note { body, ..n.clone() };
            if changed.has_content() { Ok(changed) } else { Err(ModelError::EmptyNote) }
        })
    }

    /// Adds a photo or sketch note at `offset_ms`, with an optional caption.
    pub fn with_attachment_note(
        &self,
        session: SessionId,
        offset_ms: u64,
        attachment: Attachment,
        caption: &str,
        labels: &[LabelKind],
    ) -> Result<(Library, NoteId), ModelError> {
        let valid = attachment.blob.kind().is_image() && attachment.width > 0 && attachment.height > 0;
        if !valid {
            return Err(ModelError::UnsupportedFile);
        }
        self.insert_note(session, new_note(offset_ms, caption, labels, Some(attachment)))
    }
}
