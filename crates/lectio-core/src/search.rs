//! Search across every live session (Phase 3, ADR-018): notes, headings,
//! slide titles, the text on slides, and transcripts.
//!
//! Lectio's data is small (a term of lectures is a few MB of text), so the
//! search scans the library directly instead of keeping a separate index
//! that would have to be saved and kept in step. Matching ignores case and
//! accents; every query word must appear as a whole word, except the last,
//! which may be the start of a word (so results appear while typing).
//!
//! Ranking: each match scores the kind's weight (heading 3, slide title 2.5,
//! note 2, slide text 1.5, transcript 1) times the word matches (1 for a
//! whole word, 0.5 for a prefix), plus one more weight if the whole query
//! appears as a phrase. Ties go to the newer session, then the earlier moment.

use crate::material::{SlideDeck, SlideLink};
use crate::model::{NoteKind, Session};
use crate::{Library, ModelError, SessionId};

/// Characters shown around a match.
const SNIPPET_CHARS: usize = 80;
/// How much of the snippet comes before the match.
const SNIPPET_LEAD: usize = 30;
/// A snippet start moves back at most this far to reach a word's start.
const WORD_SNAP: usize = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    Heading,
    Note,
    SlideTitle,
    SlideText,
    Transcript,
}

impl HitKind {
    fn weight(self) -> f32 {
        match self {
            Self::Heading => 3.0,
            Self::SlideTitle => 2.5,
            Self::Note => 2.0,
            Self::SlideText => 1.5,
            Self::Transcript => 1.0,
        }
    }
}

/// One search result and where it leads.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub session: SessionId,
    pub kind: HitKind,
    /// The moment in the recording, if the text has one.
    pub offset_ms: Option<u64>,
    /// The slide to show, if the text is about one.
    pub slide: Option<SlideLink>,
    pub snippet: String,
    pub score: f32,
}

/// Lower case without accents, one output char per input char (so match
/// positions in the folded text are positions in the original too).
pub fn fold(text: &str) -> String {
    text.chars().map(fold_char).collect()
}

fn fold_char(c: char) -> char {
    let lower = match c {
        'İ' | 'ı' => 'i',
        _ => c.to_lowercase().next().unwrap_or(c),
    };
    match lower {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' => 'i',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'ş' | 'ś' | 'š' => 's',
        'ğ' => 'g',
        'ž' | 'ź' | 'ż' => 'z',
        'ł' => 'l',
        'ř' => 'r',
        'ť' => 't',
        'ď' => 'd',
        other => other,
    }
}

/// Words of `folded` text with their char positions.
fn words(folded: &[char]) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in folded.iter().enumerate() {
        match (c.is_alphanumeric(), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((s, folded[s..i].iter().collect()));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, folded[s..].iter().collect()));
    }
    out
}

/// A parsed query: folded words, and the folded phrase.
struct Query {
    terms: Vec<String>,
    phrase: String,
}

impl Query {
    fn parse(text: &str) -> Option<Query> {
        let folded: Vec<char> = fold(text).chars().collect();
        let terms: Vec<String> = words(&folded).into_iter().map(|(_, w)| w).collect();
        (!terms.is_empty()).then(|| Query { phrase: terms.join(" "), terms })
    }

    /// (score before weight, char position of the first match), or `None`
    /// if a word is missing.
    fn matches(&self, text: &str) -> Option<(f32, usize)> {
        let folded: Vec<char> = fold(text).chars().collect();
        let text_words = words(&folded);
        let last = self.terms.len() - 1;
        let mut score = 0.0;
        let mut first = usize::MAX;
        for (i, term) in self.terms.iter().enumerate() {
            let exact = text_words.iter().find(|(_, w)| w == term);
            let found = match exact {
                Some((pos, _)) => Some((1.0, *pos)),
                None if i == last => text_words.iter().find(|(_, w)| w.starts_with(term.as_str())).map(|(p, _)| (0.5, *p)),
                None => None,
            };
            let (points, pos) = found?;
            score += points;
            first = first.min(pos);
        }
        let joined: String = text_words.iter().map(|(_, w)| w.as_str()).collect::<Vec<_>>().join(" ");
        if self.terms.len() > 1 && joined.contains(&self.phrase) {
            score += 1.0;
        }
        Some((score, first))
    }
}

/// About `SNIPPET_CHARS` characters around `position`, with "…" where cut.
fn snippet(text: &str, position: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut start = position.saturating_sub(SNIPPET_LEAD);
    // Don't start in the middle of a word: step back to its first letter.
    let word_floor = start.saturating_sub(WORD_SNAP);
    while start > word_floor && !chars[start - 1].is_whitespace() {
        start -= 1;
    }
    let end = (start + SNIPPET_CHARS).min(chars.len());
    let start = end.saturating_sub(SNIPPET_CHARS).min(start);
    let body: String = chars[start..end].iter().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    let lead = if start > 0 { "…" } else { "" };
    let trail = if end < chars.len() { "…" } else { "" };
    format!("{lead}{body}{trail}")
}

/// Every searchable piece of one session: (kind, text, moment, slide).
fn pieces(session: &Session) -> Vec<(HitKind, &str, Option<u64>, Option<SlideLink>)> {
    let mut out = Vec::new();
    for note in session.notes.iter().filter(|n| !n.body.is_empty()) {
        let kind = match note.kind {
            NoteKind::Slide => HitKind::SlideTitle,
            NoteKind::Text if note.is_heading() => HitKind::Heading,
            NoteKind::Text => HitKind::Note,
        };
        out.push((kind, note.body.as_str(), Some(note.offset_ms), note.slide));
    }
    for deck in &session.decks {
        for (index, text) in deck.page_text.iter().enumerate() {
            let link = SlideLink { deck: deck.id, page: index as u32 + 1 };
            let moment = session.notes.iter().find(|n| n.slide == Some(link)).map(|n| n.offset_ms);
            out.push((HitKind::SlideText, text.as_str(), moment, Some(link)));
        }
    }
    for segment in session.transcript.iter().flat_map(|t| &t.segments) {
        out.push((HitKind::Transcript, segment.text.as_str(), Some(segment.start_ms), None));
    }
    out
}

/// The best `limit` hits for `query` across all live sessions.
pub fn search(library: &Library, query: &str, limit: usize) -> Vec<Hit> {
    let Some(query) = Query::parse(query) else { return Vec::new() };
    let mut hits: Vec<(Hit, u64)> = Vec::new();
    for (_, session) in library.live_sessions() {
        for (kind, text, offset_ms, slide) in pieces(session) {
            if let Some((points, position)) = query.matches(text) {
                let hit = Hit {
                    session: session.id,
                    kind,
                    offset_ms,
                    slide,
                    snippet: snippet(text, position),
                    score: points * kind.weight(),
                };
                hits.push((hit, session.created_unix_ms));
            }
        }
    }
    hits.sort_by(|(a, a_created), (b, b_created)| {
        b.score
            .total_cmp(&a.score)
            .then(b_created.cmp(a_created))
            .then(a.offset_ms.unwrap_or(u64::MAX).cmp(&b.offset_ms.unwrap_or(u64::MAX)))
    });
    hits.into_iter().take(limit).map(|(hit, _)| hit).collect()
}

/// Pages (1-based, in order) of one deck whose text matches `query`.
pub fn search_deck(deck: &SlideDeck, query: &str) -> Vec<u32> {
    let Some(query) = Query::parse(query) else { return Vec::new() };
    deck.page_text
        .iter()
        .enumerate()
        .filter(|(_, text)| query.matches(text).is_some())
        .map(|(i, _)| i as u32 + 1)
        .collect()
}

impl Library {
    /// Stores the text found on each page of a deck (one entry per page).
    pub fn with_slide_text(
        &self,
        session: SessionId,
        deck: crate::DeckId,
        page_text: Vec<String>,
    ) -> Result<Library, ModelError> {
        self.map_session(session, |s| {
            let index = s.decks.iter().position(|d| d.id == deck).ok_or(ModelError::DeckNotFound)?;
            let page_count = s.decks[index].page_count;
            if page_text.len() != page_count as usize {
                return Err(ModelError::PageOutOfRange { page: page_text.len() as u32, page_count });
            }
            let mut decks = s.decks.clone();
            decks[index] = SlideDeck { page_text, ..decks[index].clone() };
            Ok(Session { decks, ..s.clone() })
        })
    }
}
