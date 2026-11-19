//! The worker's message loop (see `main.rs`).

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use js_sys::{Array, Float32Array, Promise, Uint8Array};
use lectio_speech::Engine;
use lectio_speech::job::{Transcription, caption};
use lectio_speech::protocol::{FromWorker, ToWorker};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

/// A command and the buffers that came with it.
type Job = (ToWorker, Vec<JsValue>);

#[derive(Default)]
struct State {
    engine: RefCell<Option<Engine>>,
    queue: RefCell<VecDeque<Job>>,
    running: Cell<bool>,
    cancel: Cell<bool>,
}

fn scope() -> DedicatedWorkerGlobalScope {
    js_sys::global().unchecked_into()
}

fn post(message: &FromWorker) {
    if let Ok(json) = serde_json::to_string(message) {
        let _ = scope().post_message(&JsValue::from_str(&json));
    }
}

fn fail(session: Option<lectio_core::SessionId>, message: String) {
    post(&FromWorker::Error { session, message });
}

/// Lets queued messages (such as "cancel") arrive before going on.
async fn yield_now() {
    let promise = Promise::new(&mut |resolve, _| {
        let _ = scope().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 0);
    });
    let _ = JsFuture::from(promise).await;
}

pub fn start() {
    let state = Rc::new(State::default());
    let receiver = Rc::clone(&state);
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let parts: Array = event.data().unchecked_into();
        let Some(json) = parts.get(0).as_string() else { return };
        let command = match serde_json::from_str::<ToWorker>(&json) {
            Ok(command) => command,
            Err(e) => return fail(None, format!("speech worker: bad message ({e})")),
        };
        if command == ToWorker::Cancel {
            receiver.cancel.set(true);
            return;
        }
        let buffers = parts.iter().skip(1).collect();
        receiver.queue.borrow_mut().push_back((command, buffers));
        if !receiver.running.get() {
            receiver.running.set(true);
            wasm_bindgen_futures::spawn_local(run(Rc::clone(&receiver)));
        }
    });
    scope().set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    // The handler lives as long as the worker does.
    on_message.forget();
}

async fn run(state: Rc<State>) {
    loop {
        let next = state.queue.borrow_mut().pop_front();
        let Some((command, buffers)) = next else { break };
        match command {
            ToWorker::Load { model } => load(&state, model, &buffers),
            ToWorker::Transcribe { session, language, start_ms } => {
                transcribe(&state, session, language.as_deref(), start_ms, &buffers).await;
            }
            ToWorker::Caption { language } => live_caption(&state, language.as_deref(), &buffers),
            ToWorker::Cancel => {}
        }
    }
    state.running.set(false);
}

fn bytes(buffers: &[JsValue], index: usize) -> Vec<u8> {
    buffers.get(index).map(|b| Uint8Array::new(b).to_vec()).unwrap_or_default()
}

fn samples(buffers: &[JsValue]) -> Vec<f32> {
    buffers.first().map(|b| Float32Array::new(b).to_vec()).unwrap_or_default()
}

fn load(state: &State, model: lectio_core::prefs::SpeechModel, buffers: &[JsValue]) {
    *state.engine.borrow_mut() = None;
    match Engine::load(&bytes(buffers, 0), &bytes(buffers, 1), bytes(buffers, 2)) {
        Ok(engine) => {
            *state.engine.borrow_mut() = Some(engine);
            post(&FromWorker::Loaded { model });
        }
        Err(message) => fail(None, message),
    }
}

async fn transcribe(
    state: &State,
    session: lectio_core::SessionId,
    language: Option<&str>,
    start_ms: u64,
    buffers: &[JsValue],
) {
    let pcm = samples(buffers);
    state.cancel.set(false);
    let mut job = Transcription::new(pcm.len(), start_ms, language);
    loop {
        let step = match state.engine.borrow_mut().as_mut() {
            Some(engine) => job.next(engine, &pcm),
            None => Err("The speech model is not loaded.".into()),
        };
        match step {
            Ok(Some(window)) => {
                let (done_ms, total_ms) = (window.done_ms, window.total_ms);
                post(&FromWorker::Segments { session, segments: window.segments, language: window.language });
                post(&FromWorker::Progress { session, done_ms, total_ms });
            }
            Ok(None) => {
                let language = job.language().unwrap_or("en").to_owned();
                return post(&FromWorker::Finished { session, language, complete: true });
            }
            Err(message) => return fail(Some(session), message),
        }
        yield_now().await;
        if state.cancel.get() {
            let language = job.language().unwrap_or("en").to_owned();
            return post(&FromWorker::Finished { session, language, complete: false });
        }
    }
}

fn live_caption(state: &State, language: Option<&str>, buffers: &[JsValue]) {
    let pcm = samples(buffers);
    let result = match state.engine.borrow_mut().as_mut() {
        Some(engine) => caption(engine, &pcm, language),
        None => Err("The speech model is not loaded.".into()),
    };
    match result {
        Ok(text) => post(&FromWorker::Caption { text }),
        Err(message) => fail(None, message),
    }
}
