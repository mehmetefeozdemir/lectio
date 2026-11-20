//! Phase 4a study actions on `LectioApp` (ADR-021): AI settings, AI jobs
//! (outline, quiz, flashcards) run chunk by chunk through the provider
//! layer, quizzes, and flashcard reviews.

use lectio_core::cards::{Grade, parse_cards};
use lectio_core::outline::{Outline, OutlineItem, merge_outlines, parse_outline};
use lectio_core::prefs::AiProvider;
use lectio_core::quiz::{
    Answer, Question, QuizAttempt, cloze_questions, grade_text, parse_questions, sources_in_topics, spread, topics,
};
use lectio_core::sources::{Chunk, Citation, chunk, gather, sample_chunks};
use lectio_core::{CardId, prompts};

use super::*;
use crate::action::{QuizScope, Response};
use crate::platform::ai::{AiEvent, AiRequest, StudyModel};

/// Name of the endpoint key in the platform's secret store.
pub const KEY_NAME: &str = "openai-compatible";

#[derive(Debug, Clone, PartialEq)]
pub enum JobKind {
    Outline(SessionId),
    Quiz { scope: QuizScope, count: usize, topics: Vec<String> },
    Cards(SessionId),
}

/// An AI job: one request per chunk, one at a time.
pub struct Job {
    pub kind: JobKind,
    /// Each chunk with the session its sources come from.
    chunks: Vec<(SessionId, Chunk)>,
    /// Questions or cards to ask for in each chunk.
    counts: Vec<usize>,
    pub next: usize,
    waiting: Option<u64>,
    outline: Vec<Vec<OutlineItem>>,
    questions: Vec<Question>,
    cards: Vec<(String, String, Vec<Citation>)>,
}

impl Job {
    pub fn total(&self) -> usize {
        self.chunks.len()
    }
}

/// A quiz in progress.
pub struct QuizRun {
    pub questions: Vec<Question>,
    pub index: usize,
    /// Whether each answered question was right.
    pub results: Vec<bool>,
    /// The answer to the current question, once given.
    pub given: Option<Response>,
    pub topics: Vec<String>,
    pub offline: bool,
}

/// A flashcard review in progress.
pub struct ReviewRun {
    pub queue: Vec<CardId>,
    pub index: usize,
    pub revealed: bool,
    pub reviewed: usize,
}

#[derive(Default)]
pub struct StudyState {
    pub model: Option<Box<dyn StudyModel>>,
    pub settings_open: bool,
    /// Ollama's models (`Some(None)` = checked, not running).
    pub ollama: Option<Option<Vec<String>>>,
    pub download: Option<(u64, u64)>,
    pub ready: bool,
    /// The local model's last answer: tokens (prompt + reply) and seconds.
    pub last_timing: Option<(usize, f64)>,
    pub has_key: bool,
    pub job: Option<Job>,
    next_id: u64,
    pub quiz: Option<QuizRun>,
    pub review: Option<ReviewRun>,
}

impl LectioApp {
    pub(super) fn apply_study(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::SetAiSettingsOpen(open) => {
                self.study.settings_open = open;
                if open && self.study.model.is_some() {
                    self.send_ai(AiRequest::ProbeOllama);
                }
            }
            Action::SetAiProvider(provider) => {
                self.change_prefs(|p| p.ai = provider);
                self.study.ready = false;
            }
            Action::SaveAiKey(key) => self.save_key(&key),
            Action::ProbeOllama => self.send_ai(AiRequest::ProbeOllama),
            Action::PrepareAi => self.send_ai(AiRequest::Prepare { provider: self.prefs().ai.clone() }),
            Action::ShowOutline(on) => self.change_prefs(|p| p.show_outline_tab = on),
            Action::GenerateOutline => {
                if let Some(session) = self.selected {
                    self.start_job(JobKind::Outline(session));
                }
            }
            Action::OutlineToFeed => self.outline_to_feed(),
            Action::StartQuiz { scope, count, topics, use_ai } => self.start_quiz(scope, count, topics, use_ai),
            Action::Answer(response) => self.answer(response),
            Action::NextQuestion => self.next_question(),
            Action::EndQuiz => self.end_quiz(),
            Action::MakeDefinitionCards(session) => self.make_definition_cards(session),
            Action::GenerateCards(session) => self.start_job(JobKind::Cards(session)),
            Action::StartReview => self.start_review(),
            Action::RevealCard => {
                if let Some(review) = &mut self.study.review {
                    review.revealed = true;
                }
            }
            Action::GradeCard(grade) => self.grade_card(grade),
            Action::EndReview => self.study.review = None,
            Action::DeleteCard(card) => self.change_library(|lib| lib.without_card(card)),
            other => return Some(other),
        }
        None
    }

    pub(super) fn change_library(&mut self, change: impl FnOnce(&Library) -> Result<Library, ModelError>) {
        match change(&self.library) {
            Ok(library) => self.library = library,
            Err(e) => self.show_error(e),
        }
    }

    /// AI is usable: a provider is chosen and the platform has the layer.
    pub(super) fn ai_on(&self) -> bool {
        self.prefs().ai.is_set() && self.study.model.is_some()
    }

    fn send_ai(&mut self, request: AiRequest) {
        match &self.study.model {
            Some(model) => model.send(request),
            None => self.show_error("AI study tools are turned off in this window (sample data)."),
        }
    }

    fn save_key(&mut self, key: &str) {
        let Some(platform) = &self.platform else { return };
        let value = Some(key.trim()).filter(|k| !k.is_empty());
        match platform.save_secret(KEY_NAME, value) {
            Ok(()) => self.study.has_key = value.is_some(),
            Err(e) => self.show_error(e),
        }
    }

    fn start_job(&mut self, kind: JobKind) {
        if !self.ai_on() {
            return self.show_error("Choose an AI provider first (Settings › AI study tools).");
        }
        if self.study.job.is_some() {
            return self.show_error("An AI job is already running. Wait for it to finish.");
        }
        let (sessions, topic_filter, wanted) = match &kind {
            JobKind::Outline(s) | JobKind::Cards(s) => (vec![*s], Vec::new(), 0),
            JobKind::Quiz { scope, topics, count } => (self.scope_sessions(*scope), topics.clone(), *count),
        };
        let mut chunks = Vec::new();
        for id in sessions {
            let Some(session) = self.library.session(id) else { continue };
            let mut material = gather(session);
            if matches!(kind, JobKind::Outline(_)) && !material.is_empty() && !prompts::enough_for_outline(&material) {
                return self.show_error(
                    "There's too little here to outline yet (it would only repeat your notes). \
                     Add more notes or slides, or transcribe the recording.",
                );
            }
            if !topic_filter.is_empty() {
                let chosen: Vec<_> = topics(session).into_iter().filter(|t| topic_filter.contains(&t.title)).collect();
                material = sources_in_topics(&material, &chosen);
            }
            chunks.extend(chunk(&material, prompts::CHUNK_TOKENS).into_iter().map(|c| (id, c)));
        }
        let chunks = sample_pairs(chunks, prompts::MAX_CHUNKS);
        if chunks.is_empty() {
            return self.show_error("There is nothing to work from yet: add notes, slides or a transcript first.");
        }
        let counts = match &kind {
            JobKind::Quiz { .. } => spread(wanted, chunks.len()),
            JobKind::Cards(_) => vec![prompts::CARDS_PER_CHUNK; chunks.len()],
            JobKind::Outline(_) => vec![1; chunks.len()],
        };
        self.study.job = Some(Job {
            kind,
            chunks,
            counts,
            next: 0,
            waiting: None,
            outline: Vec::new(),
            questions: Vec::new(),
            cards: Vec::new(),
        });
        self.send_next_chunk();
    }

    fn scope_sessions(&self, scope: QuizScope) -> Vec<SessionId> {
        match scope {
            QuizScope::Session(s) => vec![s],
            QuizScope::Course(c) => {
                self.library.live_sessions().iter().filter(|(course, _)| course.id == c).map(|(_, s)| s.id).collect()
            }
            QuizScope::Collection(id) => self.library.collection_sessions(id),
        }
    }

    fn send_next_chunk(&mut self) {
        let key = self.platform.as_ref().and_then(|p| p.load_secret(KEY_NAME));
        let provider = self.prefs().ai.clone();
        let Some(job) = &mut self.study.job else { return };
        // A small quiz over many chunks asks some chunks for nothing.
        while job.next < job.chunks.len() && job.counts[job.next] == 0 {
            job.next += 1;
        }
        if job.next >= job.chunks.len() {
            return self.finish_job();
        }
        let chunk = &job.chunks[job.next].1;
        let count = job.counts[job.next];
        let ((system, user), max_tokens) = match &job.kind {
            JobKind::Outline(_) => (prompts::outline(chunk), prompts::OUTLINE_REPLY_TOKENS),
            JobKind::Quiz { topics, .. } => (prompts::quiz(chunk, count, topics), prompts::QUIZ_REPLY_TOKENS),
            JobKind::Cards(_) => (prompts::cards(chunk, count), prompts::CARDS_REPLY_TOKENS),
        };
        self.study.next_id += 1;
        let id = self.study.next_id;
        job.waiting = Some(id);
        let key = if matches!(provider, AiProvider::OpenAi { .. }) { key } else { None };
        if let Some(model) = &self.study.model {
            model.send(AiRequest::Complete { id, provider, key, system, user, max_tokens });
        }
    }

    fn on_reply(&mut self, id: u64, text: &str) {
        let Some(job) = self.study.job.as_mut().filter(|j| j.waiting == Some(id)) else { return };
        let (session, chunk) = &job.chunks[job.next];
        match &job.kind {
            JobKind::Outline(_) => job.outline.push(parse_outline(text, &chunk.sources)),
            JobKind::Quiz { .. } => {
                let mut found = parse_questions(text, &chunk.sources, *session);
                found.truncate(job.counts[job.next].max(1));
                job.questions.extend(found);
            }
            JobKind::Cards(_) => job.cards.extend(parse_cards(text, &chunk.sources)),
        }
        job.next += 1;
        job.waiting = None;
        self.send_next_chunk();
    }

    fn finish_job(&mut self) {
        let Some(job) = self.study.job.take() else { return };
        let model = describe(&self.prefs().ai);
        match job.kind {
            JobKind::Outline(session) => {
                let items = merge_outlines(job.outline);
                if items.is_empty() {
                    return self.show_error(
                        "The model's outline had no lines Lectio could check against your notes. Try again or a larger model.",
                    );
                }
                let outline = Outline { items, model, created_unix_ms: now_unix_ms() };
                self.change_library(|lib| lib.with_outline(session, outline));
            }
            JobKind::Quiz { topics, .. } => {
                if job.questions.is_empty() {
                    return self.show_error("The model wrote no usable questions. Try again, fewer topics, or a larger model.");
                }
                let run = QuizRun { questions: job.questions, index: 0, results: Vec::new(), given: None, topics, offline: false };
                self.study.quiz = Some(run);
            }
            JobKind::Cards(session) => {
                let count = job.cards.len();
                if count == 0 {
                    return self.show_error("The model wrote no usable flashcards. Try again or a larger model.");
                }
                self.change_library(|lib| lib.with_generated_cards(session, job.cards, now_unix_ms()));
                self.banner = Some(Banner::warning(format!("Added {count} flashcards. Each one links to where it came from.")));
            }
        }
        self.persist();
    }

    fn outline_to_feed(&mut self) {
        let Some(session) = self.selected else { return };
        match self.library.with_outline_headings(session) {
            Ok((library, added)) => {
                self.library = library;
                self.banner = Some(Banner::warning(format!("Added {added} headings to the feed.")));
            }
            Err(e) => self.show_error(e),
        }
    }

    fn start_quiz(&mut self, scope: QuizScope, count: usize, topics: Vec<String>, use_ai: bool) {
        if use_ai && self.ai_on() {
            return self.start_job(JobKind::Quiz { scope, count, topics });
        }
        let sessions = self.scope_sessions(scope);
        let mut questions = cloze_questions(&self.library, &sessions);
        questions.truncate(count.max(1));
        if questions.is_empty() {
            return self.show_error("No Definition notes to quiz on yet. Label notes like \"Term: meaning\" as Definition.");
        }
        self.study.quiz = Some(QuizRun { questions, index: 0, results: Vec::new(), given: None, topics, offline: true });
    }

    fn answer(&mut self, response: Response) {
        let Some(run) = self.study.quiz.as_mut().filter(|r| r.given.is_none()) else { return };
        let Some(question) = run.questions.get(run.index) else { return };
        let right = match (&question.answer, &response) {
            (Answer::Choice { correct, .. }, Response::Choice(picked)) => correct == picked,
            (Answer::Text { answer }, Response::Text(given)) => grade_text(given, answer),
            _ => false,
        };
        run.results.push(right);
        run.given = Some(response);
    }

    fn next_question(&mut self) {
        let Some(run) = &mut self.study.quiz else { return };
        if run.given.is_none() {
            run.results.push(false); // skipped
        }
        run.given = None;
        run.index += 1;
        if run.index >= run.questions.len() {
            self.end_quiz();
        }
    }

    /// Saves the score on each session the questions came from.
    fn end_quiz(&mut self) {
        let Some(run) = self.study.quiz.take() else { return };
        let mut per_session: Vec<(SessionId, u32, u32)> = Vec::new();
        for (question, right) in run.questions.iter().zip(&run.results) {
            match per_session.iter_mut().find(|(s, _, _)| *s == question.session) {
                Some(entry) => {
                    entry.1 += 1;
                    entry.2 += u32::from(*right);
                }
                None => per_session.push((question.session, 1, u32::from(*right))),
            }
        }
        let now = now_unix_ms();
        for (session, asked, correct) in per_session {
            let attempt = QuizAttempt { unix_ms: now, questions: asked, correct, topics: run.topics.clone() };
            self.change_library(|lib| lib.with_quiz_attempt(session, attempt));
        }
        let (right, asked) = (run.results.iter().filter(|r| **r).count(), run.results.len());
        if asked > 0 {
            self.banner = Some(Banner::warning(format!("Quiz done: {right} of {asked} right. The score is saved.")));
        }
        self.persist();
    }

    fn make_definition_cards(&mut self, session: Option<SessionId>) {
        let sessions: Vec<SessionId> = match session {
            Some(s) => vec![s],
            None => self.library.live_sessions().iter().map(|(_, s)| s.id).collect(),
        };
        let mut added = 0;
        for id in sessions {
            match self.library.with_definition_cards(id, now_unix_ms()) {
                Ok((library, n)) => {
                    self.library = library;
                    added += n;
                }
                Err(e) => return self.show_error(e),
            }
        }
        let message = if added == 0 {
            "No new Definition notes to make cards from (\"Term: meaning\", labelled Definition).".to_owned()
        } else {
            format!("Made {added} flashcards from Definition notes.")
        };
        self.banner = Some(Banner::warning(message));
    }

    fn start_review(&mut self) {
        let queue: Vec<CardId> = self.library.due_cards(now_unix_ms()).iter().map(|c| c.id).collect();
        if queue.is_empty() {
            return self.show_error("No cards are due. Come back later, or make cards from Definition notes.");
        }
        self.study.review = Some(ReviewRun { queue, index: 0, revealed: false, reviewed: 0 });
    }

    fn grade_card(&mut self, grade: Grade) {
        let Some(review) = &mut self.study.review else { return };
        let Some(&card) = review.queue.get(review.index) else { return };
        review.index += 1;
        review.revealed = false;
        review.reviewed += 1;
        let done = review.index >= review.queue.len();
        self.change_library(|lib| lib.with_card_reviewed(card, grade, now_unix_ms()));
        if done {
            let reviewed = self.study.review.take().map_or(0, |r| r.reviewed);
            self.banner = Some(Banner::warning(format!("Reviewed {reviewed} cards. They come back when they are due.")));
        }
    }

    /// Per-frame: folds AI events in. True while a job or download runs.
    pub(super) fn tick_study(&mut self) -> bool {
        let events = self.study.model.as_ref().map(|m| m.poll()).unwrap_or_default();
        let before = self.library.clone();
        for event in events {
            match event {
                AiEvent::Downloading { done, total, .. } => self.study.download = Some((done, total)),
                AiEvent::Ready { .. } => {
                    self.study.download = None;
                    self.study.ready = true;
                }
                AiEvent::Ollama(models) => self.study.ollama = Some(models),
                AiEvent::Timing { tokens, seconds } => self.study.last_timing = Some((tokens, seconds)),
                AiEvent::Reply { id, text } => self.on_reply(id, &text),
                AiEvent::Failed { id, message } => {
                    if id.is_some() {
                        self.study.job = None;
                    }
                    self.study.download = None;
                    self.show_error(message);
                }
            }
        }
        if self.library != before {
            self.persist();
        }
        self.study.job.is_some() || self.study.download.is_some()
    }
}

/// An even sample of (session, chunk) pairs, at most `max`.
fn sample_pairs(pairs: Vec<(SessionId, Chunk)>, max: usize) -> Vec<(SessionId, Chunk)> {
    let len = pairs.len();
    if len <= max {
        return pairs;
    }
    let sessions: Vec<SessionId> = pairs.iter().map(|(s, _)| *s).collect();
    let kept = sample_chunks(pairs.into_iter().map(|(_, c)| c).collect(), max);
    (0..max).map(|i| sessions[i * len / max]).zip(kept).collect()
}

/// "Qwen2.5-0.5B-Instruct", "Ollama llama3.2", … (stored with an outline).
pub fn describe(provider: &AiProvider) -> String {
    match provider {
        AiProvider::None => "none".into(),
        AiProvider::Local { model } => lectio_llm::models::info(*model).name.split(" (").next().unwrap_or("local").to_owned(),
        AiProvider::Ollama { model } => format!("Ollama {model}"),
        AiProvider::OpenAi { model, .. } => model.clone(),
    }
}
