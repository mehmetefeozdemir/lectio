//! The material an AI study tool reads, and how its answers point back to
//! it (Phase 4a, ADR-021).
//!
//! A session becomes a numbered list of *sources*: notes, headings, posted
//! slides and their text, and transcript segments, in time order. Each is
//! sent to the model as one line, e.g. `[S12] (17:05) BFS visits …`, and the
//! model ends what it writes with the numbers it used (`[S12]`). Those
//! numbers are mapped back to moments and slides here, so every generated
//! item can be checked by clicking it.
//!
//! Long sessions don't fit one request, so the list is cut into *chunks*
//! under a token budget (a token is roughly 4 characters of English).

use serde::{Deserialize, Serialize};

use crate::material::SlideLink;
use crate::model::{NoteKind, Session};
use crate::timeline::format_offset;

/// Characters per token, a common estimate for English text.
pub const CHARS_PER_TOKEN: usize = 4;
const ELLIPSIS: char = '…';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Heading,
    Note,
    /// A slide posted to the feed (its title).
    Slide,
    /// The text on a slide page.
    SlideText,
    Transcript,
}

/// One numbered piece of material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// `S<id>` in prompts; 1-based, in order.
    pub id: u32,
    pub kind: SourceKind,
    pub offset_ms: Option<u64>,
    pub slide: Option<SlideLink>,
    pub text: String,
}

/// Where a generated item came from: a moment, a slide, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slide: Option<SlideLink>,
}

/// A request-sized part of the material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub sources: Vec<Source>,
    /// The numbered lines sent to the model.
    pub text: String,
    pub tokens: usize,
    /// A source was too long and was cut.
    pub truncated: bool,
}

impl Chunk {
    fn empty() -> Self {
        Self { sources: Vec::new(), text: String::new(), tokens: 0, truncated: false }
    }
}

pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(CHARS_PER_TOKEN)
}

fn source(kind: SourceKind, offset_ms: Option<u64>, slide: Option<SlideLink>, text: String) -> Source {
    Source { id: 0, kind, offset_ms, slide, text }
}

/// Everything in a session, numbered in time order. A posted slide's text
/// comes right after the slide; text of slides never posted comes last.
pub fn gather(session: &Session) -> Vec<Source> {
    let page_text = |link: &SlideLink| {
        let deck = session.decks.iter().find(|d| d.id == link.deck)?;
        let index = (link.page as usize).checked_sub(1)?;
        deck.page_text.get(index).filter(|t| !t.trim().is_empty()).cloned()
    };
    let mut items: Vec<Source> = Vec::new();
    let mut posted: Vec<SlideLink> = Vec::new();
    for note in session.notes.iter().filter(|n| !n.body.trim().is_empty()) {
        let kind = match note.kind {
            NoteKind::Slide => SourceKind::Slide,
            NoteKind::Text if note.is_heading() => SourceKind::Heading,
            NoteKind::Text | NoteKind::Math => SourceKind::Note,
        };
        items.push(source(kind, Some(note.offset_ms), note.slide, note.body.clone()));
        if let (NoteKind::Slide, Some(link)) = (note.kind, note.slide)
            && !posted.contains(&link)
            && let Some(text) = page_text(&link)
        {
            posted.push(link);
            items.push(source(SourceKind::SlideText, Some(note.offset_ms), Some(link), text));
        }
    }
    for segment in session.transcript.iter().flat_map(|t| &t.segments) {
        items.push(source(SourceKind::Transcript, Some(segment.start_ms), None, segment.text.clone()));
    }
    // Stable: at the same moment, notes stay before the transcript.
    items.sort_by_key(|s| s.offset_ms.unwrap_or(0));
    for deck in &session.decks {
        for page in 1..=deck.page_count {
            let link = SlideLink { deck: deck.id, page };
            if !posted.contains(&link)
                && let Some(text) = page_text(&link)
            {
                items.push(source(SourceKind::SlideText, None, Some(link), text));
            }
        }
    }
    for (index, item) in items.iter_mut().enumerate() {
        item.id = index as u32 + 1;
    }
    items
}

/// `[S3] (17:05) ` or `[S9] (slide 2) `.
fn line_prefix(source: &Source) -> String {
    let place = match (source.offset_ms, source.slide) {
        (Some(offset), Some(link)) if source.kind == SourceKind::SlideText => {
            format!("{}, slide {}", format_offset(offset), link.page)
        }
        (Some(offset), _) => format_offset(offset),
        (None, Some(link)) => format!("slide {}", link.page),
        (None, None) => "-".to_owned(),
    };
    format!("[S{}] ({place}) ", source.id)
}

/// `text` cut to at most `max_chars` at a word, ending in "…" (or unchanged).
fn cut_at_word(text: &str, max_chars: usize) -> (String, bool) {
    if text.chars().count() <= max_chars {
        return (text.to_owned(), false);
    }
    let room: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    let cut = match room.rfind(char::is_whitespace) {
        Some(at) => room[..at].trim_end().to_owned(),
        None => room,
    };
    (format!("{cut}{ELLIPSIS}"), true)
}

/// Packs `sources` (in order) into chunks of at most `budget_tokens`. A
/// source too long for one chunk is cut at a word and marked with "…".
pub fn chunk(sources: &[Source], budget_tokens: usize) -> Vec<Chunk> {
    let budget_chars = budget_tokens * CHARS_PER_TOKEN;
    let mut chunks = Vec::new();
    let mut current = Chunk::empty();
    for source in sources {
        let prefix = line_prefix(source);
        let room = budget_chars.saturating_sub(prefix.chars().count() + 1);
        let (text, truncated) = cut_at_word(source.text.trim(), room);
        let line = format!("{prefix}{text}\n");
        let joined = current.text.chars().count() + line.chars().count();
        if !current.sources.is_empty() && joined.div_ceil(CHARS_PER_TOKEN) > budget_tokens {
            chunks.push(std::mem::replace(&mut current, Chunk::empty()));
        }
        current.text.push_str(&line);
        current.tokens = estimate_tokens(&current.text);
        current.truncated |= truncated;
        current.sources.push(source.clone());
    }
    if !current.sources.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// At most `max` chunks, picked evenly from start to end (a whole
/// collection can be far more than the requests should read).
pub fn sample_chunks(chunks: Vec<Chunk>, max: usize) -> Vec<Chunk> {
    let len = chunks.len();
    if len <= max {
        return chunks;
    }
    let wanted: Vec<usize> = (0..max).map(|i| i * len / max).collect();
    chunks.into_iter().enumerate().filter(|(i, _)| wanted.contains(i)).map(|(_, c)| c).collect()
}

/// Parses `S3` / `s3` / `3` into 3.
fn source_number(token: &str) -> Option<u32> {
    let token = token.trim();
    let digits = token.strip_prefix(['S', 's']).unwrap_or(token);
    if digits.is_empty() { None } else { digits.parse().ok() }
}

/// Splits `[S3, S5]` markers off a line: (the text without them, the
/// numbers in order). A line starting with "Sources:" is all numbers.
pub fn split_citations(line: &str) -> (String, Vec<u32>) {
    let mut ids = Vec::new();
    let mut text = String::new();
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find(']').map(|c| open + c) else { break };
        let inner = &rest[open + 1..close];
        let numbers: Option<Vec<u32>> =
            inner.split([',', ' ', ';']).filter(|t| !t.trim().is_empty()).map(source_number).collect();
        text.push_str(&rest[..open]);
        match numbers.filter(|n| !n.is_empty()) {
            Some(numbers) => ids.extend(numbers),
            None => text.push_str(&rest[open..=close]),
        }
        rest = &rest[close + 1..];
    }
    text.push_str(rest);
    let trimmed = text.trim();
    if trimmed.to_lowercase().starts_with("source") {
        let after = trimmed.split_once(':').map_or("", |(_, a)| a);
        ids.extend(after.split(|c: char| !c.is_alphanumeric()).filter_map(source_number));
        return (String::new(), ids);
    }
    (trimmed.to_owned(), ids)
}

/// Shorter words ("a", "is") say little about which line is meant; common
/// longer ones are listed below.
const MIN_WORD_CHARS: usize = 3;
/// A match needs at least this many shared distinctive words.
const MIN_SHARED_WORDS: usize = 2;
const COMMON: [&str; 36] = [
    "that", "this", "with", "from", "what", "which", "when", "where", "they", "them", "have", "does", "into", "your", "their",
    "about", "the", "and", "for", "are", "was", "use", "uses", "its", "how", "why", "who", "not", "but", "can", "has", "had",
    "one", "two", "you", "all",
];

fn distinctive_words(text: &str) -> Vec<String> {
    crate::search::fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= MIN_WORD_CHARS && !COMMON.contains(w))
        .map(str::to_owned)
        .collect()
}

/// The source sharing the most distinctive words with `text` (at least
/// two), used when a model forgets to write source numbers. `None` if no
/// line matches well enough: then the item is dropped.
pub fn best_match(text: &str, sources: &[Source]) -> Option<u32> {
    let wanted = distinctive_words(text);
    sources
        .iter()
        .map(|s| {
            let words = distinctive_words(&s.text);
            (s.id, wanted.iter().filter(|w| words.contains(w)).count())
        })
        .filter(|(_, shared)| *shared >= MIN_SHARED_WORDS)
        .max_by_key(|(id, shared)| (*shared, std::cmp::Reverse(*id)))
        .map(|(id, _)| id)
}

/// The source numbers a model wrote, checked against the item's own words.
/// A cited line that shares fewer than two distinctive words with the item
/// is taken as a wrong number (small models often number their items 1, 2,
/// 3… instead of citing). Returns the cited lines that fit; if none do (or
/// none were given), the best-matching line; if nothing matches well
/// either, the model's numbers unchanged, since a title in its own words
/// may share no words at all.
pub fn checked_ids(text: &str, ids: &[u32], sources: &[Source]) -> Vec<u32> {
    let wanted = distinctive_words(text);
    let shared = |source: &Source| {
        let words = distinctive_words(&source.text);
        wanted.iter().filter(|w| words.contains(w)).count()
    };
    let fits: Vec<u32> = ids
        .iter()
        .copied()
        .filter(|id| sources.iter().find(|s| s.id == *id).is_some_and(|s| shared(s) >= MIN_SHARED_WORDS))
        .collect();
    if !fits.is_empty() {
        return fits;
    }
    best_match(text, sources).map_or_else(|| ids.to_vec(), |id| vec![id])
}

/// The moments and slides of the cited sources (unknown numbers dropped,
/// each source once, in the order cited).
pub fn resolve(ids: &[u32], sources: &[Source]) -> Vec<Citation> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for id in ids {
        if seen.contains(id) {
            continue;
        }
        seen.push(*id);
        if let Some(source) = sources.iter().find(|s| s.id == *id) {
            out.push(Citation { offset_ms: source.offset_ms, slide: source.slide });
        }
    }
    out
}
