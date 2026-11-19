//! Study commands (`lectio_llm::protocol`): load the local study model,
//! answer one prompt, or build the dictionary index. Each runs to the end
//! before the next message is read; the page waits for the answer.

use std::cell::RefCell;

use js_sys::{Array, Uint8Array};
use lectio_llm::LlmEngine;
use lectio_llm::protocol::{FromStudy, ToStudy};
use wasm_bindgen::JsValue;

use crate::worker::{bytes, scope};

thread_local! {
    static ENGINE: RefCell<Option<LlmEngine>> = const { RefCell::new(None) };
}

/// Posts `[json, ...buffers]`, moving the buffers to the page.
fn post(message: &FromStudy, buffers: &[Uint8Array]) {
    let Ok(json) = serde_json::to_string(message) else { return };
    let parts = Array::of1(&JsValue::from_str(&json));
    let transfer = Array::new();
    for buffer in buffers {
        parts.push(buffer);
        transfer.push(&buffer.buffer());
    }
    let _ = scope().post_message_with_transfer(&parts, &transfer);
}

fn fail(id: Option<u64>, message: String) {
    post(&FromStudy::Failed { id, message }, &[]);
}

/// Tells the page the worker is listening (the speech side ignores it).
pub fn started() {
    post(&FromStudy::Started, &[]);
}

pub fn handle(command: ToStudy, buffers: &[JsValue]) {
    match command {
        ToStudy::Load { model } => {
            // Drop the old model first so two never sit in memory at once.
            ENGINE.with(|e| *e.borrow_mut() = None);
            match LlmEngine::load(bytes(buffers, 0), &bytes(buffers, 1)) {
                Ok(engine) => {
                    ENGINE.with(|e| *e.borrow_mut() = Some(engine));
                    post(&FromStudy::Loaded { model }, &[]);
                }
                Err(message) => fail(None, message),
            }
        }
        ToStudy::Complete { id, system, user, max_tokens } => {
            // The engine can't read a clock in WebAssembly, so time it here.
            let started = js_sys::Date::now();
            let result = ENGINE.with(|e| match e.borrow_mut().as_mut() {
                Some(engine) => engine.generate(&system, &user, max_tokens, |_| true),
                None => Err("The study model is not loaded.".into()),
            });
            let seconds = (js_sys::Date::now() - started) / 1000.0;
            match result {
                Ok((text, stats)) => {
                    let (prompt_tokens, generated_tokens) = (stats.prompt_tokens, stats.generated_tokens);
                    post(&FromStudy::Reply { id, text, prompt_tokens, generated_tokens, seconds }, &[]);
                }
                Err(message) => fail(Some(id), message),
            }
        }
        ToStudy::BuildIndex => {
            let jsonl = String::from_utf8_lossy(&bytes(buffers, 0)).into_owned();
            let tsv = lectio_core::dictionary::index_from_jsonl(&jsonl);
            post(&FromStudy::Index, &[Uint8Array::from(tsv.as_bytes())]);
        }
    }
}
