//! Study tools for the mobile apps, from the desktop's rules: the offline
//! fill-in-the-blank quiz from Definition notes, quiz scores, flashcards
//! with SM-2 scheduling. AI-written outlines and questions are in `engines`.

use lectio_core::cards::Grade as CoreGrade;
use lectio_core::quiz::{Answer, Question, QuizAttempt, cloze_questions};
use lectio_core::{CardId, SessionId};

use crate::library::LectioLibrary;
use crate::{Result, id, now_ms};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Grade {
    Again,
    Hard,
    Good,
    Easy,
}

impl From<Grade> for CoreGrade {
    fn from(grade: Grade) -> Self {
        match grade {
            Grade::Again => Self::Again,
            Grade::Hard => Self::Hard,
            Grade::Good => Self::Good,
            Grade::Easy => Self::Easy,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct QuizQuestionInfo {
    pub prompt: String,
    /// The right answer (for multiple choice, the right choice's text).
    pub answer: String,
    /// Empty for a short-answer question.
    pub choices: Vec<String>,
    pub explanation: String,
    pub session_id: String,
    /// Where in the recording the answer comes from, if known.
    pub offset_ms: Option<u64>,
}

pub(crate) fn question_info(q: &Question) -> QuizQuestionInfo {
    let (answer, choices) = match &q.answer {
        Answer::Choice { choices, correct } => (choices.get(*correct).cloned().unwrap_or_default(), choices.clone()),
        Answer::Text { answer } => (answer.clone(), Vec::new()),
    };
    QuizQuestionInfo {
        prompt: q.prompt.clone(),
        answer,
        choices,
        explanation: q.explanation.clone(),
        session_id: q.session.to_string(),
        offset_ms: q.cites.iter().find_map(|c| c.offset_ms),
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct CardInfo {
    pub id: String,
    pub session_id: String,
    pub front: String,
    pub back: String,
    pub due_unix_ms: u64,
}

/// Is a typed answer right? (Every word of the expected answer, ignoring
/// case, accents and punctuation; the desktop's rule.)
#[uniffi::export]
pub fn grade_answer(given: String, expected: String) -> bool {
    lectio_core::quiz::grade_text(&given, &expected)
}

#[uniffi::export]
impl LectioLibrary {
    /// Up to `count` fill-in-the-blank questions from the sessions'
    /// Definition notes ("Term: meaning"). Works with no AI.
    pub fn offline_quiz(&self, session_ids: Vec<String>, count: u32) -> Result<Vec<QuizQuestionInfo>> {
        let sessions: Vec<SessionId> = session_ids.iter().map(|s| id(s)).collect::<Result<_>>()?;
        self.read(|inner| Ok(cloze_questions(&inner.library, &sessions).iter().take(count as usize).map(question_info).collect()))
    }

    pub fn record_quiz_attempt(&self, session_id: String, asked: u32, correct: u32) -> Result<()> {
        let session = id::<SessionId>(&session_id)?;
        let attempt = QuizAttempt { unix_ms: now_ms(), questions: asked, correct, topics: Vec::new() };
        self.update(|lib| lib.with_quiz_attempt(session, attempt))
    }

    /// A flashcard per Definition note that doesn't have one yet; returns how many.
    pub fn make_definition_cards(&self, session_id: String, now_unix_ms: u64) -> Result<u32> {
        let session = id::<SessionId>(&session_id)?;
        self.change(|lib| lib.with_definition_cards(session, now_unix_ms)).map(|n| n as u32)
    }

    pub fn due_cards(&self, now_unix_ms: u64) -> Result<Vec<CardInfo>> {
        self.read(|inner| {
            Ok(inner
                .library
                .due_cards(now_unix_ms)
                .into_iter()
                .map(|c| CardInfo {
                    id: c.id.to_string(),
                    session_id: c.session.to_string(),
                    front: c.front.clone(),
                    back: c.back.clone(),
                    due_unix_ms: c.review.due_unix_ms,
                })
                .collect())
        })
    }

    /// SM-2: schedules the card's next review from how well it was known.
    pub fn grade_card(&self, card_id: String, grade: Grade, now_unix_ms: u64) -> Result<()> {
        let card = id::<CardId>(&card_id)?;
        self.update(|lib| lib.with_card_reviewed(card, grade.into(), now_unix_ms))
    }
}
