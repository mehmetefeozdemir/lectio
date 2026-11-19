//! Quizzes (Phase 4a, ADR-021): questions written by an AI provider from a
//! session's sources, or, with no AI, cloze questions ("fill in the blank")
//! made from Definition notes. Every question cites where its answer is.

use serde::{Deserialize, Serialize};

use crate::model::Session;
use crate::search::fold;
use crate::sources::{Citation, Source, checked_ids, resolve, split_citations};
use crate::{LabelKind, Library, ModelError, SessionId};

/// What a definition note's term is replaced with in a cloze question.
pub const BLANK: &str = "_____";
/// A definition's term is at most this many words ("BFS", "Depth-first search").
const MAX_TERM_WORDS: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Answer {
    Choice { choices: Vec<String>, correct: usize },
    Text { answer: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub prompt: String,
    pub answer: Answer,
    pub explanation: String,
    pub cites: Vec<Citation>,
    pub session: SessionId,
}

/// One finished quiz, kept on the session it was about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuizAttempt {
    pub unix_ms: u64,
    pub questions: u32,
    pub correct: u32,
    #[serde(default)]
    pub topics: Vec<String>,
}

/// A section of a session, from one heading to the next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topic {
    pub title: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// Question blocks as the prompt asks for them:
/// `Q:` …, `A)`…`D)` (multiple choice), `Answer:`, `Why:`, `Sources:`.
/// Blocks with a bad answer letter, no answer, or no valid source are dropped.
pub fn parse_questions(text: &str, sources: &[Source], session: SessionId) -> Vec<Question> {
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if starts_question(line) {
            blocks.push(Vec::new());
        }
        if let Some(block) = blocks.last_mut() {
            block.push(line);
        }
    }
    blocks.iter().filter_map(|block| parse_block(block, sources, session)).collect()
}

/// `Q:`, `Question:` or a numbered `Q3:`.
fn starts_question(line: &str) -> bool {
    let lower = line.to_lowercase();
    if lower.starts_with("question:") {
        return true;
    }
    let Some(rest) = lower.strip_prefix('q') else { return false };
    rest.split_once(':').is_some_and(|(number, _)| number.chars().all(|c| c.is_ascii_digit()))
}

fn after_colon(line: &str) -> &str {
    line.split_once(':').map_or("", |(_, rest)| rest.trim())
}

/// `B) Queue` / `b. Queue` → (1, "Queue").
fn choice(line: &str) -> Option<(usize, String)> {
    let mut chars = line.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    let rest = chars.as_str().strip_prefix([')', '.'])?;
    ('A'..='F').contains(&letter).then(|| ((letter as u8 - b'A') as usize, rest.trim().to_owned()))
}

fn parse_block(block: &[&str], sources: &[Source], session: SessionId) -> Option<Question> {
    let prompt = after_colon(block.first()?).to_owned();
    let mut choices: Vec<String> = Vec::new();
    let (mut answer, mut explanation, mut ids) = (String::new(), String::new(), Vec::new());
    for line in &block[1..] {
        let lower = line.to_lowercase();
        if lower.starts_with("answer:") {
            answer = after_colon(line).to_owned();
        } else if lower.starts_with("why:") || lower.starts_with("explanation:") {
            let (text, cited) = split_citations(after_colon(line));
            explanation = text;
            ids.extend(cited);
        } else if lower.starts_with("source") {
            ids.extend(split_citations(line).1);
        } else if let Some((_, text)) = choice(line).filter(|(i, _)| *i == choices.len()) {
            choices.push(text);
        }
    }
    // Numbers missing or wrong: cite the line the question and answer come from.
    let ids = checked_ids(&format!("{prompt} {answer} {explanation} {}", choices.join(" ")), &ids, sources);
    let cites = resolve(&ids, sources);
    if prompt.is_empty() || answer.is_empty() || cites.is_empty() || is_placeholder(&prompt) || is_placeholder(&answer) {
        return None;
    }
    let distinct = choices.iter().enumerate().all(|(i, c)| !choices[..i].iter().any(|d| d.eq_ignore_ascii_case(c)));
    if !distinct || choices.iter().any(|c| is_placeholder(c)) {
        return None;
    }
    let answer = if choices.len() >= 2 {
        let by_letter = choice(&format!("{answer})")).map(|(i, _)| i).filter(|_| answer.trim().len() == 1);
        let by_text = choices.iter().position(|c| c.eq_ignore_ascii_case(answer.trim()));
        let correct = by_letter.or(by_text).filter(|i| *i < choices.len())?;
        Answer::Choice { choices, correct }
    } else {
        Answer::Text { answer }
    };
    Some(Question { prompt, answer, explanation, cites, session })
}

/// Words a small model copies from the format example instead of writing
/// its own ("A) choice", "Answer: short answer").
const PLACEHOLDERS: [&str; 6] = ["choice", "option", "question", "short answer", "one sentence", "answer"];

fn is_placeholder(text: &str) -> bool {
    let text = text.trim().trim_end_matches('.').to_lowercase();
    text.is_empty() || PLACEHOLDERS.contains(&text.as_str())
}

/// `Term: meaning`, `Term — meaning`, `Term - meaning` or `Term = meaning`.
pub fn split_definition(body: &str) -> Option<(String, String)> {
    for separator in [":", " — ", " – ", " - ", " = "] {
        if let Some((term, meaning)) = body.split_once(separator) {
            let (term, meaning) = (term.trim(), meaning.trim());
            let words = term.split_whitespace().count();
            if (1..=MAX_TERM_WORDS).contains(&words) && !meaning.is_empty() {
                return Some((term.to_owned(), meaning.to_owned()));
            }
        }
    }
    None
}

/// The offline quiz: one fill-in-the-blank question per Definition note.
pub fn cloze_questions(library: &Library, sessions: &[SessionId]) -> Vec<Question> {
    let mut out = Vec::new();
    for &id in sessions {
        let Some(session) = library.session(id) else { continue };
        for note in session.notes.iter().filter(|n| n.labels.contains(&LabelKind::Definition)) {
            let Some((term, meaning)) = split_definition(&note.body) else { continue };
            out.push(Question {
                prompt: format!("{BLANK}: {meaning}"),
                answer: Answer::Text { answer: term },
                explanation: note.body.clone(),
                cites: vec![Citation { offset_ms: Some(note.offset_ms), slide: note.slide }],
                session: id,
            });
        }
    }
    out
}

/// Words of a folded text, punctuation as spaces ("Depth-first" → "depth first").
fn words(text: &str) -> Vec<String> {
    fold(text).split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_owned).collect()
}

/// A typed answer is right if it contains every word of the expected one
/// (case, accents and punctuation ignored).
pub fn grade_text(given: &str, expected: &str) -> bool {
    let given = words(given);
    let expected = words(expected);
    !given.is_empty() && !expected.is_empty() && expected.iter().all(|w| given.contains(w))
}

/// The session's sections, from its headings (or its outline if it has no
/// headings); each lasts until the next one.
pub fn topics(session: &Session) -> Vec<Topic> {
    let mut starts: Vec<(String, u64)> =
        session.notes.iter().filter(|n| n.is_heading()).map(|n| (n.body.clone(), n.offset_ms)).collect();
    if starts.is_empty()
        && let Some(outline) = &session.outline
    {
        starts = outline.items.iter().filter(|i| i.level == 1).filter_map(|i| Some((i.title.clone(), i.start_ms()?))).collect();
    }
    starts.sort_by_key(|(_, start)| *start);
    let last = session.recording.as_ref().map_or(u64::MAX, |r| r.duration_ms.max(1));
    let ends: Vec<u64> = starts.iter().skip(1).map(|(_, s)| *s).chain(std::iter::once(last)).collect();
    starts.into_iter().zip(ends).map(|((title, start_ms), end_ms)| Topic { title, start_ms, end_ms }).collect()
}

/// The sources that fall inside the chosen topics.
pub fn sources_in_topics(sources: &[Source], topics: &[Topic]) -> Vec<Source> {
    sources
        .iter()
        .filter(|s| s.offset_ms.is_some_and(|o| topics.iter().any(|t| (t.start_ms..t.end_ms).contains(&o))))
        .cloned()
        .collect()
}

/// `total` questions shared over `parts` chunks, as evenly as possible.
pub fn spread(total: usize, parts: usize) -> Vec<usize> {
    if parts == 0 {
        return Vec::new();
    }
    (0..parts).map(|i| total / parts + usize::from(i < total % parts)).collect()
}

impl Library {
    pub fn with_quiz_attempt(&self, session: SessionId, attempt: QuizAttempt) -> Result<Library, ModelError> {
        if attempt.questions == 0 || attempt.correct > attempt.questions {
            return Err(ModelError::InvalidStudyItem("A quiz score needs questions, and at most one point each.".into()));
        }
        self.map_session(session, |s| {
            let mut attempts = s.quiz_attempts.clone();
            attempts.push(attempt);
            Ok(Session { quiz_attempts: attempts, ..s.clone() })
        })
    }
}
