//! Browser AI provider layer (ADR-021).
//!
//! - Local: the Qwen model is downloaded once (checked by SHA-256) into
//!   IndexedDB's `models` table, then handed to a *second* copy of the
//!   worker program (`crates/lectio-worker`), started on first use, so the
//!   study model has its own WebAssembly memory apart from Whisper's.
//! - Ollama and OpenAI-compatible endpoints: `fetch` from the page. The
//!   browser only allows it when the server sends CORS headers for this
//!   page's address (Ollama does for localhost pages by default).

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use js_sys::{Array, Reflect};
use lectio_core::prefs::{AiProvider, LocalModel};
use lectio_llm::models::{self, LlmFile, LlmInfo};
use lectio_llm::protocol::{FromStudy, ToStudy};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Event, IdbDatabase, MessageEvent, Worker};

use super::speech::{WORKER_URL, sleep};
use super::{fetch, idb, js_err};
use crate::platform::ai::{
    AiEvent, AiRequest, OLLAMA_URL, StudyModel, ollama_body, ollama_models, openai_body, openai_url, reply_text,
};

const WAIT_MS: i32 = 100;
const CRASHED: &str = "The study model stopped (the browser may have run out of memory). Close other tabs and try again, \
                       or use the desktop app.";

fn marker(info: &LlmInfo) -> String {
    format!("{}/complete", info.dir)
}

fn key(info: &LlmInfo, file: &LlmFile) -> String {
    format!("{}/{}", info.dir, file.file.name)
}

/// The running worker and its handlers (kept alive with it).
struct StudyWorker {
    worker: Worker,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(Event)>,
}

/// State shared with the worker's handlers.
#[derive(Default)]
struct Shared {
    events: RefCell<Vec<AiEvent>>,
    started: Cell<bool>,
    loaded: Cell<Option<LocalModel>>,
    /// Requests the worker hasn't answered yet (failed if it crashes).
    pending: RefCell<Vec<u64>>,
    crashed: Cell<bool>,
}

impl Shared {
    fn emit(&self, event: AiEvent) {
        self.events.borrow_mut().push(event);
    }

    fn on_message(&self, data: &JsValue) {
        let parts: Array = data.clone().unchecked_into();
        let Some(json) = parts.get(0).as_string() else { return };
        let Ok(message) = serde_json::from_str::<FromStudy>(&json) else { return };
        match message {
            FromStudy::Started => self.started.set(true),
            FromStudy::Loaded { model } => {
                self.loaded.set(Some(model));
                self.emit(AiEvent::Ready { model });
            }
            FromStudy::Reply { id, text, prompt_tokens, generated_tokens, seconds } => {
                self.pending.borrow_mut().retain(|p| *p != id);
                self.emit(AiEvent::Timing { tokens: prompt_tokens + generated_tokens, seconds });
                self.emit(AiEvent::Reply { id, text });
            }
            FromStudy::Failed { id, message } => {
                if let Some(id) = id {
                    self.pending.borrow_mut().retain(|p| *p != id);
                } else {
                    self.loaded.set(None);
                }
                self.emit(AiEvent::Failed { id, message });
            }
            FromStudy::Index => {}
        }
    }

    /// The worker died (usually out of memory): fail what it was doing.
    fn on_crash(&self) {
        self.crashed.set(true);
        self.loaded.set(None);
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        if pending.is_empty() {
            self.emit(AiEvent::Failed { id: None, message: CRASHED.into() });
        }
        for id in pending {
            self.emit(AiEvent::Failed { id: Some(id), message: CRASHED.into() });
        }
    }
}

struct Inner {
    db: IdbDatabase,
    worker: RefCell<Option<StudyWorker>>,
    shared: Rc<Shared>,
    downloaded: RefCell<HashSet<LocalModel>>,
    preparing: Cell<bool>,
}

pub struct WebStudyModel {
    inner: Rc<Inner>,
}

impl WebStudyModel {
    pub fn start(db: IdbDatabase) -> Self {
        let inner = Rc::new(Inner {
            db,
            worker: RefCell::new(None),
            shared: Rc::default(),
            downloaded: RefCell::new(HashSet::new()),
            preparing: Cell::new(false),
        });
        let checker = Rc::clone(&inner);
        wasm_bindgen_futures::spawn_local(async move {
            for model in LocalModel::ALL {
                if matches!(idb::get(&checker.db, idb::MODELS, &marker(models::info(model))).await, Ok(Some(_))) {
                    checker.downloaded.borrow_mut().insert(model);
                }
            }
        });
        Self { inner }
    }
}

impl Inner {
    /// The study worker, started (or restarted after a crash) on demand.
    async fn worker(&self) -> Result<Worker, String> {
        if self.shared.crashed.replace(false) {
            if let Some(old) = self.worker.borrow_mut().take() {
                old.worker.terminate();
            }
            self.shared.started.set(false);
        }
        let existing = self.worker.borrow().as_ref().map(|w| w.worker.clone());
        let worker = match existing {
            Some(worker) => worker,
            None => self.spawn()?,
        };
        while !self.shared.started.get() && !self.shared.crashed.get() {
            let _ = sleep(WAIT_MS).await;
        }
        if self.shared.crashed.get() {
            return Err("The study worker could not start.".into());
        }
        Ok(worker)
    }

    fn spawn(&self) -> Result<Worker, String> {
        let worker = Worker::new(WORKER_URL).map_err(js_err)?;
        let shared = Rc::clone(&self.shared);
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| shared.on_message(&event.data()));
        let shared = Rc::clone(&self.shared);
        let on_error = Closure::<dyn FnMut(Event)>::new(move |_: Event| shared.on_crash());
        worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        *self.worker.borrow_mut() = Some(StudyWorker { worker: worker.clone(), _on_message: on_message, _on_error: on_error });
        Ok(worker)
    }

    fn post(worker: &Worker, command: &ToStudy, buffers: &[JsValue]) -> Result<(), String> {
        let json = serde_json::to_string(command).map_err(|e| e.to_string())?;
        let message = Array::of1(&JsValue::from_str(&json));
        let transfer = Array::new();
        for buffer in buffers {
            message.push(buffer);
            if let Ok(buffer_object) = Reflect::get(buffer, &"buffer".into()) {
                transfer.push(&buffer_object);
            }
        }
        worker.post_message_with_transfer(&message, &transfer).map_err(js_err)
    }

    /// Downloads the model if needed and has the worker load it.
    async fn prepare(&self, model: LocalModel) -> Result<Worker, String> {
        while self.preparing.get() {
            let _ = sleep(WAIT_MS).await;
        }
        let worker = self.worker().await?;
        if self.shared.loaded.get() == Some(model) {
            return Ok(worker);
        }
        self.preparing.set(true);
        let result = self.download_and_load(&worker, model).await;
        self.preparing.set(false);
        result.map(|()| worker)
    }

    async fn download_and_load(&self, worker: &Worker, model: LocalModel) -> Result<(), String> {
        let info = models::info(model);
        if !info.in_browser {
            return Err("This model is too big for a browser tab. Use the smaller one here, or the desktop app.".into());
        }
        if !self.downloaded.borrow().contains(&model) {
            let total = info.total_bytes();
            let mut done = 0;
            for file in &info.files {
                let shared = Rc::clone(&self.shared);
                let bytes = fetch::download(file.url, &file.file, |received| {
                    shared.emit(AiEvent::Downloading { model, done: done + received, total });
                })
                .await?;
                idb::put(&self.db, idb::MODELS, &key(info, file), &bytes).await?;
                done += file.file.bytes;
            }
            idb::put(&self.db, idb::MODELS, &marker(info), &JsValue::TRUE).await?;
            self.downloaded.borrow_mut().insert(model);
        }
        let mut files = Vec::new();
        for file in &info.files {
            let value = idb::get(&self.db, idb::MODELS, &key(info, file))
                .await?
                .ok_or("A study model file is missing; download it again in AI settings.")?;
            files.push(value);
        }
        Self::post(worker, &ToStudy::Load { model }, &files)?;
        // The worker answers `Loaded` (or `Failed`); requests sent after
        // this wait behind the load in the worker's queue.
        self.shared.loaded.set(Some(model));
        Ok(())
    }

    async fn complete(&self, id: u64, provider: AiProvider, key: Option<String>, system: String, user: String, max: usize) {
        let result = match provider {
            AiProvider::None => Err("No AI provider is set.".into()),
            AiProvider::Local { model } => match self.prepare(model).await {
                Ok(worker) => {
                    self.shared.pending.borrow_mut().push(id);
                    // The reply arrives through the worker's message handler.
                    return if let Err(message) =
                        Self::post(&worker, &ToStudy::Complete { id, system, user, max_tokens: max }, &[])
                    {
                        self.shared.pending.borrow_mut().retain(|p| *p != id);
                        self.shared.emit(AiEvent::Failed { id: Some(id), message });
                    };
                }
                Err(message) => Err(message),
            },
            AiProvider::Ollama { model } => {
                let answer = fetch::post_json(&format!("{OLLAMA_URL}/api/chat"), None, &ollama_body(&model, &system, &user, max)).await;
                answer.and_then(|a| reply_text(&a).ok_or_else(|| "Ollama sent no reply text.".into()))
            }
            AiProvider::OpenAi { base_url, model } => match key.filter(|k| !k.is_empty()) {
                None => Err("Add the endpoint's key in AI settings first.".into()),
                Some(key) => {
                    let body = openai_body(&model, &system, &user, max);
                    let answer = fetch::post_json(&openai_url(&base_url), Some(&key), &body).await;
                    answer.and_then(|a| reply_text(&a).ok_or_else(|| "The endpoint sent no reply text.".into()))
                }
            },
        };
        self.shared.emit(match result {
            Ok(text) => AiEvent::Reply { id, text },
            Err(message) => AiEvent::Failed { id: Some(id), message },
        });
    }
}

async fn probe_ollama() -> Option<Vec<String>> {
    let text = fetch::get_text(&format!("{OLLAMA_URL}/api/tags")).await.ok().flatten()?;
    let answer: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(ollama_models(&answer))
}

impl StudyModel for WebStudyModel {
    fn send(&self, request: AiRequest) {
        let inner = Rc::clone(&self.inner);
        wasm_bindgen_futures::spawn_local(async move {
            match request {
                AiRequest::Prepare { provider: AiProvider::Local { model } } => {
                    if let Err(message) = inner.prepare(model).await {
                        inner.shared.emit(AiEvent::Failed { id: None, message });
                    }
                }
                AiRequest::Prepare { .. } | AiRequest::ProbeOllama => inner.shared.emit(AiEvent::Ollama(probe_ollama().await)),
                AiRequest::Complete { id, provider, key, system, user, max_tokens } => {
                    inner.complete(id, provider, key, system, user, max_tokens).await;
                }
            }
        });
    }

    fn poll(&self) -> Vec<AiEvent> {
        std::mem::take(&mut *self.inner.shared.events.borrow_mut())
    }

    fn is_downloaded(&self, model: LocalModel) -> bool {
        self.inner.downloaded.borrow().contains(&model)
    }

    fn unavailable(&self, model: LocalModel) -> Option<&'static str> {
        (!models::info(model).in_browser)
            .then_some("This model needs more memory than a browser tab allows. Use the smaller one here, or the desktop app.")
    }
}
