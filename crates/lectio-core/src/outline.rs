//! A session's outline (Phase 4a, ADR-021): section titles with the moments
//! they start at, made by an AI provider from the session's sources. Lines
//! the model writes without a valid source number are dropped.

use serde::{Deserialize, Serialize};

use crate::model::Session;
use crate::sources::{Citation, Source, checked_ids, resolve, split_citations};
use crate::{LabelKind, Library, ModelError, SessionId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineItem {
    pub title: String,
    /// 1 = section, 2 = sub-point.
    pub level: u8,
    pub cites: Vec<Citation>,
}

impl OutlineItem {
    /// The earliest cited moment.
    pub fn start_ms(&self) -> Option<u64> {
        self.cites.iter().filter_map(|c| c.offset_ms).min()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outline {
    pub items: Vec<OutlineItem>,
    /// Which provider/model wrote it (shown to the user).
    pub model: String,
    pub created_unix_ms: u64,
}

/// Reads outline lines such as `- [S12] Breadth-first search`,
/// `  - Queue order [S12]` (indented = level 2) or `## Title [S3]`.
pub fn parse_outline(text: &str, sources: &[Source]) -> Vec<OutlineItem> {
    let mut items = Vec::new();
    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        let (rest, level) = if let Some(rest) = trimmed.strip_prefix("###") {
            (rest, 2)
        } else if let Some(rest) = trimmed.strip_prefix("##").or_else(|| trimmed.strip_prefix('#')) {
            (rest, 1)
        } else if let Some(rest) = trimmed.strip_prefix(['-', '*', '•']) {
            (rest, if indent >= 2 { 2 } else { 1 })
        } else if trimmed.starts_with('[') {
            (trimmed, if indent >= 2 { 2 } else { 1 })
        } else if let Some(rest) = numbered(trimmed) {
            (rest, if indent >= 2 { 2 } else { 1 })
        } else {
            continue;
        };
        let (title, ids) = split_citations(rest);
        let title = without_marks(without_time(title.trim())).trim_end_matches([':', '.', '-']).trim().to_owned();
        let ids = checked_ids(&title, &ids, sources);
        let cites = resolve(&ids, sources);
        if !title.is_empty() && !cites.is_empty() && !copies_a_source(&title, &ids, sources) {
            items.push(OutlineItem { title, level, cites });
        }
    }
    items
}

/// `- - Title` or `1. - Title` → `Title`: list markers a model doubled up
/// (the line's own marker is already removed).
fn without_marks(title: &str) -> &str {
    let mut rest = title.trim_start();
    while let Some(next) = rest.strip_prefix(['-', '*', '•']).or_else(|| numbered(rest)) {
        rest = next.trim_start();
    }
    rest
}

/// `(17:04) text` or `(17:04, slide 2) text` → `text` (a model echoing the
/// material's line format).
fn without_time(title: &str) -> &str {
    match title.strip_prefix('(').and_then(|rest| rest.split_once(')')) {
        Some((inside, rest)) if inside.chars().next().is_some_and(|c| c.is_ascii_digit()) || inside.starts_with("slide") => {
            rest.trim_start()
        }
        _ => title,
    }
}

/// A section title is a short summary; a long title that is just one of
/// the cited lines copied out (a small model's habit) is not one.
const MAX_COPIED_WORDS: usize = 6;

fn copies_a_source(title: &str, ids: &[u32], sources: &[Source]) -> bool {
    let normal = |t: &str| t.trim().trim_end_matches(['.', '!', '?']).to_lowercase();
    title.split_whitespace().count() > MAX_COPIED_WORDS
        && sources.iter().filter(|s| ids.contains(&s.id)).any(|s| normal(&s.text) == normal(title))
}

/// `3. Title` → `Title`.
fn numbered(line: &str) -> Option<&str> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    line[digits..].strip_prefix(['.', ')'])
}

/// Joins the outlines of several chunks: in time order, each title once.
pub fn merge_outlines(parts: Vec<Vec<OutlineItem>>) -> Vec<OutlineItem> {
    let mut all: Vec<OutlineItem> = parts.into_iter().flatten().collect();
    all.sort_by_key(|item| item.start_ms().unwrap_or(u64::MAX));
    let mut seen: Vec<String> = Vec::new();
    all.retain(|item| {
        let key = item.title.to_lowercase();
        let fresh = !seen.contains(&key);
        seen.push(key);
        fresh
    });
    all
}

impl Library {
    pub fn with_outline(&self, session: SessionId, outline: Outline) -> Result<Library, ModelError> {
        self.map_session(session, |s| Ok(Session { outline: Some(outline), ..s.clone() }))
    }

    pub fn without_outline(&self, session: SessionId) -> Result<Library, ModelError> {
        self.map_session(session, |s| Ok(Session { outline: None, ..s.clone() }))
    }

    /// Adds the outline's sections to the feed as Heading notes at their
    /// moments, skipping any heading already there. Returns how many.
    pub fn with_outline_headings(&self, session: SessionId) -> Result<(Library, usize), ModelError> {
        let s = self.session(session).ok_or(ModelError::SessionNotFound)?;
        let Some(outline) = &s.outline else { return Ok((self.clone(), 0)) };
        let mut library = self.clone();
        let mut added = 0;
        for item in outline.items.iter().filter(|i| i.level == 1) {
            let Some(offset) = item.start_ms() else { continue };
            let exists = library.session(session).is_some_and(|s| {
                s.notes.iter().any(|n| n.is_heading() && n.offset_ms == offset && n.body.eq_ignore_ascii_case(&item.title))
            });
            if exists {
                continue;
            }
            library = library.with_note(session, offset, &item.title, &[LabelKind::Heading])?.0;
            added += 1;
        }
        Ok((library, added))
    }
}
