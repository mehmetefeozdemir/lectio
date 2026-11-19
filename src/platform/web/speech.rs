//! Browser speech service (ADR-016): the page side of the speech worker.
//!
//! The page downloads a model once (`fetch`, read chunk by chunk for the
//! progress bar, checked against its SHA-256 checksum) into IndexedDB's
//! `models` table, then hands the three files to the worker
//! (`crates/lectio-worker`), which runs Whisper off the main thread.
//! Audio for transcription is read from IndexedDB and decoded here, then
//! transferred to the worker.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::io::Cursor;
use std::rc::Rc;

use js_sys::{Array, Float32Array, Promise, Reflect, Uint8Array};
use lectio_core::SessionId;
use lectio_core::prefs::SpeechModel;
use lectio_speech::models::{self, ModelInfo};
use lectio_speech::protocol::{FromWorker, ToWorker};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{IdbDatabase, MessageEvent, Worker};

use super::{fetch, idb, js_err};
use crate::platform::speech::{SpeechEvent, SpeechRequest, SpeechService};
use crate::platform::wav::decode_mono_i16;

/// The worker's start-up script, written by trunk (`data-loader-shim`).
pub(super) const WORKER_URL: &str = "./lectio-worker_loader.js";
const WAIT_MS: i32 = 100;

struct Inner {
    db: IdbDatabase,
    worker: Worker,
    /// Shared with the worker's message handler.
    events: Rc<RefCell<Vec<SpeechEvent>>>,
    /// The model the worker has (or is about to have) loaded.
    loaded: Rc<Cell<Option<SpeechModel>>>,
    downloaded: RefCell<HashSet<SpeechModel>>,
    preparing: Cell<bool>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
}

pub struct WebSpeech {
    inner: Rc<Inner>,
}

fn marker(model: SpeechModel) -> String {
    format!("whisper-{}/complete", model.name())
}

fn key(info: &ModelInfo, file: &models::ModelFile) -> String {
    format!("whisper-{}/{}", info.model.name(), file.name)
}

impl WebSpeech {
    pub fn start(db: IdbDatabase) -> Result<Self, String> {
        let worker = Worker::new(WORKER_URL).map_err(js_err)?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let loaded = Rc::new(Cell::new(None));
        let sink = (Rc::clone(&events), Rc::clone(&loaded));
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let Some(json) = event.data().as_string() else { return };
            let Ok(message) = serde_json::from_str::<FromWorker>(&json) else { return };
            if let FromWorker::Loaded { model } = message {
                sink.1.set(Some(model));
            }
            sink.0.borrow_mut().push(to_event(message));
        });
        worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        let inner = Rc::new(Inner {
            db,
            worker,
            events,
            loaded,
            downloaded: RefCell::new(HashSet::new()),
            preparing: Cell::new(false),
            _on_message: on_message,
        });
        let speech = Self { inner };
        speech.check_downloads();
        Ok(speech)
    }

    fn check_downloads(&self) {
        let inner = Rc::clone(&self.inner);
        wasm_bindgen_futures::spawn_local(async move {
            for model in SpeechModel::ALL {
                if matches!(idb::get(&inner.db, idb::MODELS, &marker(model)).await, Ok(Some(_))) {
                    inner.downloaded.borrow_mut().insert(model);
                }
            }
        });
    }
}

fn to_event(message: FromWorker) -> SpeechEvent {
    match message {
        FromWorker::Loaded { model } => SpeechEvent::Ready { model },
        FromWorker::Segments { session, segments, language } => SpeechEvent::Segments { session, segments, language },
        FromWorker::Progress { session, done_ms, total_ms } => SpeechEvent::Progress { session, done_ms, total_ms },
        FromWorker::Finished { session, language, complete } => SpeechEvent::Finished { session, language, complete },
        FromWorker::Caption { text } => SpeechEvent::Caption(text),
        FromWorker::Error { session, message } => SpeechEvent::Failed { session, message },
    }
}

fn post(worker: &Worker, command: &ToWorker, buffers: &[JsValue]) -> Result<(), String> {
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

pub(super) fn sleep(ms: i32) -> JsFuture {
    JsFuture::from(Promise::new(&mut |resolve, _| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    }))
}

impl Inner {
    fn emit(&self, event: SpeechEvent) {
        self.events.borrow_mut().push(event);
    }

    /// Downloads the model if needed and has the worker load it.
    async fn prepare(self: &Rc<Self>, model: SpeechModel) -> Result<(), String> {
        while self.preparing.get() {
            let _ = sleep(WAIT_MS).await;
        }
        if self.loaded.get() == Some(model) {
            return Ok(());
        }
        self.preparing.set(true);
        let result = self.download_and_load(model).await;
        self.preparing.set(false);
        result
    }

    async fn download_and_load(self: &Rc<Self>, model: SpeechModel) -> Result<(), String> {
        let info = models::info(model);
        if !self.downloaded.borrow().contains(&model) {
            let mut done = 0;
            for file in &info.files {
                self.download(info, file, done).await?;
                done += file.bytes;
            }
            idb::put(&self.db, idb::MODELS, &marker(model), &JsValue::TRUE).await?;
            self.downloaded.borrow_mut().insert(model);
        }
        let mut files = Vec::new();
        for file in &info.files {
            let value = idb::get(&self.db, idb::MODELS, &key(info, file))
                .await?
                .ok_or("A model file is missing; download the model again.")?;
            files.push(value);
        }
        post(&self.worker, &ToWorker::Load { model }, &files)?;
        self.loaded.set(Some(model));
        Ok(())
    }

    /// Fetches one file chunk by chunk, checking its checksum as it arrives.
    async fn download(&self, info: &ModelInfo, file: &models::ModelFile, done_before: u64) -> Result<(), String> {
        let (model, total) = (info.model, info.total_bytes());
        let bytes = fetch::download(&info.url(file), file, |received| {
            self.emit(SpeechEvent::Downloading { model, done: done_before + received, total });
        })
        .await?;
        idb::put(&self.db, idb::MODELS, &key(info, file), &bytes).await
    }

    async fn transcribe(
        self: &Rc<Self>,
        session: SessionId,
        file_name: &str,
        model: SpeechModel,
        language: Option<String>,
        start_ms: u64,
    ) -> Result<(), String> {
        self.prepare(model).await?;
        let value = idb::get(&self.db, idb::RECORDINGS, file_name)
            .await?
            .ok_or("The audio for this session is not in this browser's storage.")?;
        let wav = Uint8Array::new(&value).to_vec();
        let (pcm, _) = decode_mono_i16(Cursor::new(wav)).map_err(|e| e.to_string())?;
        let floats: Vec<f32> = pcm.iter().map(|&s| f32::from(s) / f32::from(i16::MAX)).collect();
        let audio = Float32Array::from(floats.as_slice());
        post(&self.worker, &ToWorker::Transcribe { session, language, start_ms }, &[audio.into()])
    }
}

impl SpeechService for WebSpeech {
    fn send(&self, request: SpeechRequest) {
        let inner = Rc::clone(&self.inner);
        match request {
            SpeechRequest::Cancel => {
                let _ = post(&inner.worker, &ToWorker::Cancel, &[]);
            }
            SpeechRequest::Prepare { model } => wasm_bindgen_futures::spawn_local(async move {
                if let Err(message) = inner.prepare(model).await {
                    inner.emit(SpeechEvent::Failed { session: None, message });
                }
            }),
            SpeechRequest::Transcribe { session, file_name, model, language, start_ms } => {
                wasm_bindgen_futures::spawn_local(async move {
                    if let Err(message) = inner.transcribe(session, &file_name, model, language, start_ms).await {
                        inner.emit(SpeechEvent::Failed { session: Some(session), message });
                    }
                });
            }
            SpeechRequest::Caption { pcm, model, language } => {
                if inner.loaded.get() != Some(model) && !inner.downloaded.borrow().contains(&model) {
                    let message = "Live captions need the speech model. Download it in the Transcript tab first.";
                    return inner.emit(SpeechEvent::Failed { session: None, message: message.into() });
                }
                wasm_bindgen_futures::spawn_local(async move {
                    let audio = Float32Array::from(pcm.as_slice());
                    let sent = match inner.prepare(model).await {
                        Ok(()) => post(&inner.worker, &ToWorker::Caption { language }, &[audio.into()]),
                        Err(e) => Err(e),
                    };
                    if let Err(message) = sent {
                        inner.emit(SpeechEvent::Failed { session: None, message });
                    }
                });
            }
        }
    }

    fn poll(&self) -> Vec<SpeechEvent> {
        std::mem::take(&mut *self.inner.events.borrow_mut())
    }

    fn is_downloaded(&self, model: SpeechModel) -> bool {
        self.inner.downloaded.borrow().contains(&model)
    }

    fn unavailable(&self, model: SpeechModel) -> Option<&'static str> {
        (!models::info(model).in_browser).then_some(
            "The small model needs more memory than a browser tab allows. Use tiny or base here, or the desktop app.",
        )
    }

    fn supports_captions(&self) -> bool {
        true
    }
}
