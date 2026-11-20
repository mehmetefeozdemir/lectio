//! The two on-device models for the mobile apps: Whisper (speech-to-text,
//! ADR-016) and the study model (outline, quiz and flashcards, ADR-021).
//! Loading and running them is slow, so Swift calls these from a background
//! task and hears about progress through `ProgressListener`. The steps are
//! the desktop's: the same chunking, prompts, parsers and citation checks.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use lectio_core::outline::{Outline, merge_outlines, parse_outline};
use lectio_core::quiz::{parse_questions, spread};
use lectio_core::sources::{chunk, gather, sample_chunks};
use lectio_core::{ModelError, SessionId, prompts};
use lectio_llm::LlmEngine;
use lectio_speech::Engine;

use crate::library::LectioLibrary;
use crate::study::{QuizQuestionInfo, question_info};
use crate::{LectioError, OutlineItemInfo, Result, SegmentInfo, id, now_ms};

/// Swift implements this to show progress and to stop a long job.
#[uniffi::export(with_foreign)]
pub trait ProgressListener: Send + Sync {
    /// `done` of `total` (milliseconds of audio, or parts of a job).
    fn progress(&self, done: u64, total: u64);
    /// Returning false stops the job after the current step.
    fn should_continue(&self) -> bool;
}

fn read(path: &std::path::Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| LectioError::Failed(format!("A model file is missing ({}): {e}. Download the model again.", path.display())))
}

/// Mono 16 kHz samples (-1..1) of a Lectio recording.
fn read_pcm(path: &str) -> Result<Vec<f32>> {
    let reader = hound::WavReader::open(path).map_err(|e| LectioError::Failed(format!("Can't open the recording: {e}")))?;
    let samples: Vec<i16> = reader.into_samples::<i16>().collect::<std::result::Result<_, _>>().map_err(|e| LectioError::Failed(format!("Can't read the recording: {e}")))?;
    Ok(samples.into_iter().map(|s| f32::from(s) / f32::from(i16::MAX)).collect())
}

/// How fast a transcription ran (for the measurements in the docs).
#[derive(Debug, Clone, uniffi::Record)]
pub struct TranscriptionResult {
    pub text: String,
    pub language: String,
    pub audio_seconds: f64,
    pub seconds: f64,
}

/// Whisper, loaded from `models/whisper-<size>/` (files checked when downloaded).
#[derive(uniffi::Object)]
pub struct SpeechEngine {
    engine: Mutex<Engine>,
    model: String,
}

#[uniffi::export]
impl SpeechEngine {
    #[uniffi::constructor]
    pub fn load(model_dir: String, model_name: String) -> Result<Arc<Self>> {
        let dir = std::path::PathBuf::from(model_dir);
        let engine = Engine::load(&read(&dir.join("config.json"))?, &read(&dir.join("tokenizer.json"))?, read(&dir.join("model.safetensors"))?)?;
        Ok(Arc::new(Self { engine: Mutex::new(engine), model: model_name }))
    }

    /// Transcribes the session's recording, storing segments as each
    /// 30-second window finishes; returns how many segments were stored.
    pub fn transcribe_session(
        &self,
        library: Arc<LectioLibrary>,
        session_id: String,
        language: Option<String>,
        listener: Arc<dyn ProgressListener>,
    ) -> Result<u32> {
        let detail = library.session(session_id.clone())?;
        let path = detail.recording_path.ok_or(ModelError::NoRecording)?;
        let pcm = read_pcm(&path)?;
        let mut engine = self.engine.lock().map_err(|_| LectioError::Failed("The speech model stopped after an earlier error.".into()))?;
        let mut stored = 0u32;
        let mut failure = None;
        let mut stopped = false;
        let language = lectio_speech::job::transcribe(&mut engine, &pcm, 0, language.as_deref(), |window| {
            let segments: Vec<SegmentInfo> = window
                .segments
                .iter()
                .map(|s| SegmentInfo { start_ms: s.start_ms, end_ms: s.end_ms, text: s.text.clone() })
                .collect();
            stored += segments.len() as u32;
            if let Err(e) = library.store_transcript(session_id.clone(), segments, window.language.clone(), self.model.clone(), false) {
                failure = Some(e);
                return false;
            }
            listener.progress(window.done_ms, window.total_ms);
            stopped = !listener.should_continue();
            !stopped
        })?;
        if let Some(e) = failure {
            return Err(e);
        }
        library.store_transcript(session_id, Vec::new(), language, self.model.clone(), !stopped)?;
        Ok(stored)
    }

    /// Transcribes a WAV file and times it (benchmarks).
    pub fn transcribe_wav(&self, path: String, language: Option<String>) -> Result<TranscriptionResult> {
        let pcm = read_pcm(&path)?;
        let mut engine = self.engine.lock().map_err(|_| LectioError::Failed("The speech model stopped after an earlier error.".into()))?;
        let started = Instant::now();
        let mut text = Vec::new();
        let language = lectio_speech::job::transcribe(&mut engine, &pcm, 0, language.as_deref(), |window| {
            text.extend(window.segments.iter().map(|s| s.text.clone()));
            true
        })?;
        Ok(TranscriptionResult {
            text: text.join(" "),
            language,
            audio_seconds: pcm.len() as f64 / 16_000.0,
            seconds: started.elapsed().as_secs_f64(),
        })
    }
}

/// The local study model (Qwen2.5), loaded from `models/<dir>/`.
#[derive(uniffi::Object)]
pub struct StudyEngine {
    engine: Mutex<LlmEngine>,
    name: String,
}

impl StudyEngine {
    fn generate(&self, system: &str, user: &str, max_tokens: usize) -> Result<String> {
        let mut engine = self.engine.lock().map_err(|_| LectioError::Failed("The study model stopped after an earlier error.".into()))?;
        Ok(engine.generate(system, user, max_tokens, |_| true)?.0)
    }
}

#[uniffi::export]
impl StudyEngine {
    #[uniffi::constructor]
    pub fn load(model_dir: String, model_name: String) -> Result<Arc<Self>> {
        let model = crate::models::local_model(&model_name).ok_or_else(|| LectioError::Failed(format!("Unknown study model {model_name}.")))?;
        let dir = std::path::PathBuf::from(model_dir);
        let info = lectio_llm::models::info(model);
        let gguf = read(&dir.join(info.files[0].file.name))?;
        let tokenizer = read(&dir.join(info.files[1].file.name))?;
        let engine = LlmEngine::load(gguf, &tokenizer)?;
        Ok(Arc::new(Self { engine: Mutex::new(engine), name: info.name.split(" (").next().unwrap_or("local").to_owned() }))
    }

    /// Writes the session's outline (stored with the session); every item
    /// cites a moment or slide, as on the desktop.
    pub fn write_outline(&self, library: Arc<LectioLibrary>, session_id: String, listener: Arc<dyn ProgressListener>) -> Result<Vec<OutlineItemInfo>> {
        let session = id::<SessionId>(&session_id)?;
        let sources = library.read(|inner| Ok(gather(inner.library.session(session).ok_or(ModelError::SessionNotFound)?)))?;
        if !prompts::enough_for_outline(&sources) {
            return Err(LectioError::Failed(
                "There's too little here to outline yet (it would only repeat your notes). Add more notes or slides, or transcribe the recording.".into(),
            ));
        }
        let chunks = sample_chunks(chunk(&sources, prompts::CHUNK_TOKENS), prompts::MAX_CHUNKS);
        let mut parts = Vec::new();
        for (i, part) in chunks.iter().enumerate() {
            listener.progress(i as u64, chunks.len() as u64);
            if !listener.should_continue() {
                break;
            }
            let (system, user) = prompts::outline(part);
            parts.push(parse_outline(&self.generate(&system, &user, prompts::OUTLINE_REPLY_TOKENS)?, &part.sources));
        }
        let items = merge_outlines(parts);
        if items.is_empty() {
            return Err(LectioError::Failed("The model's outline had no lines Lectio could check against your notes. Try again or a larger model.".into()));
        }
        let outline = Outline { items: items.clone(), model: self.name.clone(), created_unix_ms: now_ms() };
        library.update(|lib| lib.with_outline(session, outline))?;
        Ok(items.iter().map(|i| OutlineItemInfo { title: i.title.clone(), level: i.level, offset_ms: i.start_ms() }).collect())
    }

    /// About `count` questions (multiple choice and short answer) written
    /// from the session, each citing where its answer is.
    pub fn write_quiz(&self, library: Arc<LectioLibrary>, session_id: String, count: u32, listener: Arc<dyn ProgressListener>) -> Result<Vec<QuizQuestionInfo>> {
        let session = id::<SessionId>(&session_id)?;
        let sources = library.read(|inner| Ok(gather(inner.library.session(session).ok_or(ModelError::SessionNotFound)?)))?;
        let chunks = sample_chunks(chunk(&sources, prompts::CHUNK_TOKENS), prompts::MAX_CHUNKS);
        let counts = spread(count as usize, chunks.len());
        let mut questions = Vec::new();
        for (i, (part, wanted)) in chunks.iter().zip(counts).enumerate() {
            listener.progress(i as u64, chunks.len() as u64);
            if wanted == 0 || !listener.should_continue() {
                continue;
            }
            let (system, user) = prompts::quiz(part, wanted, &[]);
            let mut found = parse_questions(&self.generate(&system, &user, prompts::QUIZ_REPLY_TOKENS)?, &part.sources, session);
            found.truncate(wanted);
            questions.extend(found);
        }
        if questions.is_empty() {
            return Err(LectioError::Failed("The model wrote no usable questions. Try again or a larger model.".into()));
        }
        Ok(questions.iter().map(question_info).collect())
    }
}
