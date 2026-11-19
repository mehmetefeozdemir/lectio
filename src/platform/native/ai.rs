//! Desktop AI provider layer (ADR-021): one background thread serves every
//! request in order: the local Qwen model (downloaded on first use to
//! `<data>/models/<name>/`, checked by SHA-256), Ollama on this computer, or
//! an OpenAI-compatible endpoint. The API key only lives in the request.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, Sender};

use lectio_core::prefs::{AiProvider, LocalModel};
use lectio_llm::LlmEngine;
use lectio_llm::models::{self, LlmInfo};
use lectio_speech::models::verify;

use super::fetch::{self, file_has_size};
use crate::platform::ai::{
    AiEvent, AiRequest, OLLAMA_URL, StudyModel, ollama_body, ollama_models, openai_body, openai_url, reply_text,
};

pub struct NativeStudyModel {
    requests: Sender<AiRequest>,
    events: Mutex<Receiver<AiEvent>>,
    models_dir: PathBuf,
}

impl NativeStudyModel {
    pub fn start(models_dir: PathBuf) -> Self {
        let (requests, inbox) = mpsc::channel();
        let (outbox, events) = mpsc::channel();
        let worker = Worker { models_dir: models_dir.clone(), engine: None, events: outbox };
        std::thread::Builder::new()
            .name("lectio-study".into())
            .spawn(move || worker.run(inbox))
            .expect("the study thread could not start");
        Self { requests, events: Mutex::new(events), models_dir }
    }
}

impl StudyModel for NativeStudyModel {
    fn send(&self, request: AiRequest) {
        let _ = self.requests.send(request);
    }

    fn poll(&self) -> Vec<AiEvent> {
        self.events.lock().map(|rx| rx.try_iter().collect()).unwrap_or_default()
    }

    fn is_downloaded(&self, model: LocalModel) -> bool {
        let info = models::info(model);
        info.files.iter().all(|f| file_has_size(&file_path(&self.models_dir, info, f.file.name), f.file.bytes))
    }

    fn unavailable(&self, _model: LocalModel) -> Option<&'static str> {
        None
    }
}

fn file_path(dir: &Path, info: &LlmInfo, name: &str) -> PathBuf {
    dir.join(info.dir).join(name)
}

struct Worker {
    models_dir: PathBuf,
    engine: Option<(LocalModel, LlmEngine)>,
    events: Sender<AiEvent>,
}

impl Worker {
    fn run(mut self, inbox: Receiver<AiRequest>) {
        while let Ok(request) = inbox.recv() {
            match request {
                AiRequest::Prepare { provider: AiProvider::Local { model } } => {
                    if let Err(message) = self.ensure_engine(model) {
                        self.emit(AiEvent::Failed { id: None, message });
                    }
                }
                AiRequest::Prepare { .. } | AiRequest::ProbeOllama => self.emit(AiEvent::Ollama(probe_ollama())),
                AiRequest::Complete { id, provider, key, system, user, max_tokens } => {
                    let reply = self.complete(&provider, key.as_deref(), &system, &user, max_tokens);
                    self.emit(match reply {
                        Ok(text) => AiEvent::Reply { id, text },
                        Err(message) => AiEvent::Failed { id: Some(id), message },
                    });
                }
            }
        }
    }

    fn emit(&self, event: AiEvent) {
        let _ = self.events.send(event);
    }

    fn complete(&mut self, provider: &AiProvider, key: Option<&str>, system: &str, user: &str, max: usize) -> Result<String, String> {
        match provider {
            AiProvider::None => Err("No AI provider is set.".into()),
            AiProvider::Local { model } => {
                let engine = self.ensure_engine(*model)?;
                let (text, stats) = engine.generate(system, user, max, |_| true)?;
                let tokens = stats.prompt_tokens + stats.generated_tokens;
                self.emit(AiEvent::Timing { tokens, seconds: stats.prompt_seconds + stats.generate_seconds });
                Ok(text)
            }
            AiProvider::Ollama { model } => {
                let answer = fetch::post_json(&format!("{OLLAMA_URL}/api/chat"), None, &ollama_body(model, system, user, max))?;
                reply_text(&answer).ok_or_else(|| "Ollama sent no reply text.".into())
            }
            AiProvider::OpenAi { base_url, model } => {
                let key = key.filter(|k| !k.is_empty()).ok_or("Add the endpoint's key in AI settings first.")?;
                let answer = fetch::post_json(&openai_url(base_url), Some(key), &openai_body(model, system, user, max))?;
                reply_text(&answer).ok_or_else(|| "The endpoint sent no reply text.".into())
            }
        }
    }

    fn ensure_engine(&mut self, model: LocalModel) -> Result<&mut LlmEngine, String> {
        if self.engine.as_ref().is_none_or(|(m, _)| *m != model) {
            self.engine = None;
            let events = self.events.clone();
            let engine = load_model(&self.models_dir, model, &mut |done, total| {
                let _ = events.send(AiEvent::Downloading { model, done, total });
            })?;
            self.engine = Some((model, engine));
            self.emit(AiEvent::Ready { model });
        }
        self.engine.as_mut().map(|(_, e)| e).ok_or_else(|| "The study model could not be loaded.".into())
    }
}

fn probe_ollama() -> Option<Vec<String>> {
    let text = fetch::get_text(&format!("{OLLAMA_URL}/api/tags")).ok().flatten()?;
    let answer: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(ollama_models(&answer))
}

/// Downloads missing files, verifies both, and loads the model.
pub fn load_model(dir: &Path, model: LocalModel, progress: &mut dyn FnMut(u64, u64)) -> Result<LlmEngine, String> {
    let info = models::info(model);
    let total = info.total_bytes();
    let mut done = 0;
    for f in &info.files {
        let path = file_path(dir, info, f.file.name);
        if !file_has_size(&path, f.file.bytes) {
            fetch::download(f.url, &f.file, &path, done, total, progress)?;
        }
        done += f.file.bytes;
    }
    progress(total, total);
    let mut contents = Vec::new();
    for f in &info.files {
        let path = file_path(dir, info, f.file.name);
        let bytes = std::fs::read(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
        if let Err(problem) = verify(&f.file, &bytes) {
            let _ = std::fs::remove_file(&path);
            return Err(problem);
        }
        contents.push(bytes);
    }
    let tokenizer = contents.pop().unwrap_or_default();
    let gguf = contents.pop().unwrap_or_default();
    LlmEngine::load(gguf, &tokenizer)
}

/// `lectio --ai-benchmark [small|large]`: loads the local study model and
/// writes an outline and a quiz for the built-in sample lecture, printing
/// the replies and the speed.
pub fn run_benchmark(dir: &Path, model: LocalModel, library: &lectio_core::Library, session: lectio_core::SessionId) -> Result<(), String> {
    use lectio_core::{prompts, sources};
    let started = std::time::Instant::now();
    let mut last = u64::MAX;
    let mut engine = load_model(dir, model, &mut |done, total| {
        let percent = done * 100 / total.max(1);
        if percent / 10 != last / 10 {
            last = percent;
            eprintln!("study model: {percent}%");
        }
    })?;
    eprintln!("model ready in {:.1} s", started.elapsed().as_secs_f64());
    let session_ref = library.session(session).ok_or("no sample session")?;
    let gathered = sources::gather(session_ref);
    let chunks = sources::chunk(&gathered, prompts::CHUNK_TOKENS);
    let first = chunks.first().ok_or("the sample has no material")?;
    for (task, (system, user), max) in [
        ("outline", prompts::outline(first), prompts::OUTLINE_REPLY_TOKENS),
        ("quiz", prompts::quiz(first, 3, &[]), prompts::QUIZ_REPLY_TOKENS),
    ] {
        let (text, stats) = engine.generate(&system, &user, max, |_| true)?;
        println!("----- {task} -----\n{text}");
        let parsed = match task {
            "outline" => lectio_core::outline::parse_outline(&text, &first.sources).len(),
            _ => lectio_core::quiz::parse_questions(&text, &first.sources, session).len(),
        };
        println!(
            "{task}: {} prompt tokens in {:.2} s ({:.0} tok/s), {} new tokens in {:.2} s ({:.1} tok/s), {parsed} items kept",
            stats.prompt_tokens,
            stats.prompt_seconds,
            stats.prompt_tokens as f64 / stats.prompt_seconds.max(1e-9),
            stats.generated_tokens,
            stats.generate_seconds,
            stats.generated_tokens as f64 / stats.generate_seconds.max(1e-9),
        );
    }
    Ok(())
}
