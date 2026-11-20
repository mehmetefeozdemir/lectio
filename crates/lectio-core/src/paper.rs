//! The paper view's document model (Phase 4b, ADR-025): a session as a list
//! of blocks (title, headings, notes, formulas, slides, pictures and, if
//! asked, the transcript). The paper view draws these blocks, and every
//! export writes them (Markdown here; HTML, PDF and DOCX in `lectio-export`),
//! so all outputs show the same thing in the same order.
//!
//! Slides appear inline: a slide posted to the feed is shown where it was
//! posted, and a slide that notes refer to is shown just before the first
//! of those notes. Each page is shown at most once.

use std::collections::HashSet;

use crate::material::{AttachmentKind, BlobKey, slide_title};
use crate::timeline::format_offset;
use crate::{DeckId, Library, ModelError, NoteKind, SessionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaperOptions {
    /// Show each note's time in the recording.
    pub times: bool,
    /// Add the transcript at the end.
    pub transcript: bool,
}

impl Default for PaperOptions {
    fn default() -> Self {
        Self { times: true, transcript: false }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Title { title: String, subtitle: String },
    Heading { text: String, offset_ms: u64 },
    /// A text note; empty `text` = a moment marked with labels only.
    Note { offset_ms: u64, text: String, labels: Vec<String> },
    Formula { offset_ms: u64, latex: String, labels: Vec<String> },
    Slide { deck: DeckId, page: u32, deck_title: String, caption: String, offset_ms: Option<u64> },
    Image { blob: BlobKey, kind: AttachmentKind, width: u32, height: u32, offset_ms: u64, caption: String },
    Transcript { lines: Vec<(u64, String)> },
}

/// A picture an export needs, rendered by the app as PNG.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AssetKey {
    /// A slide page with its ink.
    Slide(DeckId, u32),
    Formula(String),
    Image(BlobKey),
}

pub fn document(library: &Library, session: SessionId, options: &PaperOptions) -> Result<Vec<Block>, ModelError> {
    let s = library.session(session).ok_or(ModelError::SessionNotFound)?;
    let course = library.course_of(session).map(|c| c.name.clone()).unwrap_or_default();
    let subtitle = match &s.recording {
        Some(r) => format!("{course} · {}", format_offset(r.duration_ms)),
        None => course,
    };
    let mut blocks = vec![Block::Title { title: s.title.clone(), subtitle }];
    let mut shown: HashSet<(DeckId, u32)> = HashSet::new();
    let deck_title = |deck: DeckId| s.decks.iter().find(|d| d.id == deck).map(|d| d.title.clone()).unwrap_or_default();
    for note in &s.notes {
        let labels: Vec<String> = note.labels.iter().map(|&k| library.labels().name(k).to_owned()).collect();
        if note.is_heading() {
            blocks.push(Block::Heading { text: note.body.clone(), offset_ms: note.offset_ms });
            continue;
        }
        if let Some(link) = note.slide
            && shown.insert((link.deck, link.page))
        {
            let (caption, offset_ms) = match note.kind {
                NoteKind::Slide => (note.body.clone(), Some(note.offset_ms)),
                _ => (slide_title(link.page), None),
            };
            blocks.push(Block::Slide { deck: link.deck, page: link.page, deck_title: deck_title(link.deck), caption, offset_ms });
        }
        let block = match (note.kind, &note.attachment) {
            (NoteKind::Slide, _) => continue,
            (NoteKind::Math, _) => Block::Formula { offset_ms: note.offset_ms, latex: note.body.clone(), labels },
            (_, Some(a)) => Block::Image {
                blob: a.blob.clone(),
                kind: a.kind,
                width: a.width,
                height: a.height,
                offset_ms: note.offset_ms,
                caption: note.body.clone(),
            },
            _ => Block::Note { offset_ms: note.offset_ms, text: note.body.clone(), labels },
        };
        blocks.push(block);
    }
    if options.transcript
        && let Some(t) = s.transcript.as_ref().filter(|t| !t.segments.is_empty())
    {
        blocks.push(Block::Transcript { lines: t.segments.iter().map(|seg| (seg.start_ms, seg.text.clone())).collect() });
    }
    Ok(blocks)
}

/// The pictures the blocks need, each once, in order.
pub fn assets(blocks: &[Block]) -> Vec<AssetKey> {
    let mut out: Vec<AssetKey> = Vec::new();
    for block in blocks {
        let key = match block {
            Block::Slide { deck, page, .. } => AssetKey::Slide(*deck, *page),
            Block::Formula { latex, .. } => AssetKey::Formula(latex.clone()),
            Block::Image { blob, .. } => AssetKey::Image(blob.clone()),
            _ => continue,
        };
        if !out.contains(&key) {
            out.push(key);
        }
    }
    out
}

/// "0:59 " when times are shown, else nothing.
pub fn time_prefix(offset_ms: u64, options: &PaperOptions) -> String {
    if options.times { format!("{} ", format_offset(offset_ms)) } else { String::new() }
}

/// " *(Important, Review)*" after a note, or nothing.
fn labels_suffix(labels: &[String]) -> String {
    if labels.is_empty() { String::new() } else { format!(" *({})*", labels.join(", ")) }
}

/// The note's words, or a stand-in for a label-only moment.
pub fn note_text(text: &str) -> &str {
    if text.is_empty() { "(marked moment)" } else { text }
}

pub fn photo_word(kind: AttachmentKind) -> &'static str {
    match kind {
        AttachmentKind::Photo => "[photo]",
        AttachmentKind::Sketch => "[sketch]",
    }
}

/// Markdown: formulas stay as LaTeX between `$$`, slides and pictures as
/// references (Markdown can't carry the pictures themselves).
pub fn markdown(blocks: &[Block], options: &PaperOptions) -> String {
    let mut out = String::new();
    for block in blocks {
        match block {
            Block::Title { title, subtitle } => out.push_str(&format!("# {title}\n\n*{subtitle}*\n\n")),
            Block::Heading { text, .. } => out.push_str(&format!("\n## {text}\n\n")),
            Block::Note { offset_ms, text, labels } => {
                let time = time_prefix(*offset_ms, options);
                out.push_str(&format!("- {time}{}{}\n", note_text(text), labels_suffix(labels)));
            }
            Block::Formula { offset_ms, latex, labels } => {
                let time = time_prefix(*offset_ms, options);
                out.push_str(&format!("- {time}formula{}\n\n$$\n{latex}\n$$\n\n", labels_suffix(labels)));
            }
            Block::Slide { page, deck_title, caption, offset_ms, .. } => {
                let time = match offset_ms {
                    Some(ms) if options.times => format!(" · {}", format_offset(*ms)),
                    _ => String::new(),
                };
                out.push_str(&format!("\n**{caption}** ({deck_title}, page {page}){time}\n\n"));
            }
            Block::Image { kind, offset_ms, caption, .. } => {
                let time = time_prefix(*offset_ms, options);
                out.push_str(&format!("- {time}{} {caption}\n", photo_word(*kind)));
            }
            Block::Transcript { lines } => {
                out.push_str("\n## Transcript\n\n");
                for (ms, text) in lines {
                    out.push_str(&format!("{}{text}\n\n", time_prefix(*ms, options)));
                }
            }
        }
    }
    out
}
