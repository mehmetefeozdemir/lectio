//! Flashcards with spaced repetition (Phase 4a).
//!
//! *Spaced repetition* shows a card again just before you would forget it:
//! each correct answer makes the next gap longer, a miss starts it over. The
//! schedule is **SM-2** (the SuperMemo 2 algorithm): every card has an
//! "ease" (start 2.5, never below 1.3); after a pass the gap goes 1 day,
//! 6 days, then the last gap × ease; the ease rises after easy answers and
//! falls after hard ones.

use serde::{Deserialize, Serialize};

use crate::quiz::split_definition;
use crate::sources::{Citation, Source, checked_ids, resolve, split_citations};
use crate::{CardId, LabelKind, Library, ModelError, NoteId, SessionId};

const DAY_MS: u64 = 86_400_000;
const START_EASE: f32 = 2.5;
const MIN_EASE: f32 = 1.3;
/// SM-2: answers graded below this count as a miss.
const PASS: u8 = 3;

/// How well the answer was remembered (the four review buttons).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    Again,
    Hard,
    Good,
    Easy,
}

impl Grade {
    /// SM-2's 0–5 quality score.
    fn quality(self) -> u8 {
        match self {
            Self::Again => 1,
            Self::Hard => 3,
            Self::Good => 4,
            Self::Easy => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Review {
    pub ease: f32,
    pub interval_days: u32,
    pub repetitions: u32,
    pub lapses: u32,
    pub due_unix_ms: u64,
}

impl Review {
    /// A new card, due now.
    pub fn new(now_ms: u64) -> Self {
        Self { ease: START_EASE, interval_days: 0, repetitions: 0, lapses: 0, due_unix_ms: now_ms }
    }
}

/// The schedule after answering a card with `grade` at `now_ms`.
pub fn review(r: Review, grade: Grade, now_ms: u64) -> Review {
    let q = f32::from(grade.quality());
    let miss = 5.0 - q;
    let ease = (r.ease + 0.1 - miss * (0.08 + miss * 0.02)).max(MIN_EASE);
    let (repetitions, interval_days, lapses) = if grade.quality() < PASS {
        (0, 1, r.lapses + 1)
    } else {
        let interval = match r.repetitions {
            0 => 1,
            1 => 6,
            _ => (r.interval_days as f32 * r.ease).round() as u32,
        };
        (r.repetitions + 1, interval, r.lapses)
    };
    Review { ease, interval_days, repetitions, lapses, due_unix_ms: now_ms + u64::from(interval_days) * DAY_MS }
}

/// Cards as the prompt asks for them: `Front:`, `Back:`, `Sources:` blocks
/// (sources may also be `[S2]` at the end of a line). Cards without a valid
/// source or with an empty side are dropped.
pub fn parse_cards(text: &str, sources: &[Source]) -> Vec<(String, String, Vec<Citation>)> {
    let mut out = Vec::new();
    let mut card: Option<(String, String, Vec<u32>)> = None;
    let finish = |card: Option<(String, String, Vec<u32>)>, out: &mut Vec<_>| {
        if let Some((front, back, ids)) = card {
            let ids = checked_ids(&format!("{front} {back}"), &ids, sources);
            let cites = resolve(&ids, sources);
            if !front.is_empty() && !back.is_empty() && !cites.is_empty() {
                out.push((front, back, cites));
            }
        }
    };
    for line in text.lines().map(str::trim) {
        let lower = line.to_lowercase();
        let value = || split_citations(line.split_once(':').map_or("", |(_, v)| v));
        if lower.starts_with("front:") {
            finish(card.take(), &mut out);
            let (front, ids) = value();
            card = Some((front, String::new(), ids));
        } else if let Some((_, back, ids)) = card.as_mut().filter(|_| lower.starts_with("back:")) {
            let (text, cited) = value();
            *back = text;
            ids.extend(cited);
        } else if let Some((_, _, ids)) = card.as_mut().filter(|_| lower.starts_with("source")) {
            ids.extend(split_citations(line).1);
        }
    }
    finish(card, &mut out);
    out
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub id: CardId,
    pub session: SessionId,
    pub front: String,
    pub back: String,
    /// The Definition note it was made from (none = written by AI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<NoteId>,
    #[serde(default)]
    pub cites: Vec<Citation>,
    pub review: Review,
}

impl Library {
    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    fn with_more_cards(&self, cards: Vec<Card>) -> Library {
        let mut all = self.cards.clone();
        all.extend(cards);
        self.with_card_list(all)
    }

    /// A card for every Definition note ("Term: meaning") of the session
    /// that doesn't have one yet. Returns how many were added.
    pub fn with_definition_cards(&self, session: SessionId, now_ms: u64) -> Result<(Library, usize), ModelError> {
        let s = self.session(session).ok_or(ModelError::SessionNotFound)?;
        let new: Vec<Card> = s
            .notes
            .iter()
            .filter(|n| n.labels.contains(&LabelKind::Definition))
            .filter(|n| !self.cards.iter().any(|c| c.note == Some(n.id)))
            .filter_map(|n| {
                let (front, back) = split_definition(&n.body)?;
                let cites = vec![Citation { offset_ms: Some(n.offset_ms), slide: n.slide }];
                Some(Card { id: CardId::new_random(), session, front, back, note: Some(n.id), cites, review: Review::new(now_ms) })
            })
            .collect();
        let count = new.len();
        Ok((self.with_more_cards(new), count))
    }

    /// Cards written by an AI provider: (front, back, citations).
    pub fn with_generated_cards(
        &self,
        session: SessionId,
        cards: Vec<(String, String, Vec<Citation>)>,
        now_ms: u64,
    ) -> Result<Library, ModelError> {
        self.session(session).ok_or(ModelError::SessionNotFound)?;
        let mut new = Vec::new();
        for (front, back, cites) in cards {
            let (front, back) = (front.trim().to_owned(), back.trim().to_owned());
            if front.is_empty() || back.is_empty() {
                return Err(ModelError::InvalidStudyItem("A flashcard needs both a front and a back.".into()));
            }
            new.push(Card { id: CardId::new_random(), session, front, back, note: None, cites, review: Review::new(now_ms) });
        }
        Ok(self.with_more_cards(new))
    }

    pub fn with_card_reviewed(&self, card: CardId, grade: Grade, now_ms: u64) -> Result<Library, ModelError> {
        let index = self.cards.iter().position(|c| c.id == card).ok_or(ModelError::InvalidStudyItem("That card no longer exists.".into()))?;
        let mut cards = self.cards.clone();
        cards[index].review = review(cards[index].review, grade, now_ms);
        Ok(self.with_card_list(cards))
    }

    pub fn without_card(&self, card: CardId) -> Result<Library, ModelError> {
        let cards: Vec<Card> = self.cards.iter().filter(|c| c.id != card).cloned().collect();
        if cards.len() == self.cards.len() {
            return Err(ModelError::InvalidStudyItem("That card no longer exists.".into()));
        }
        Ok(self.with_card_list(cards))
    }

    /// Cards due by `now_ms` whose session is still live (not in the trash).
    pub fn due_cards(&self, now_ms: u64) -> Vec<&Card> {
        let live: Vec<SessionId> = self.live_sessions().iter().map(|(_, s)| s.id).collect();
        self.cards.iter().filter(|c| c.review.due_unix_ms <= now_ms && live.contains(&c.session)).collect()
    }
}
