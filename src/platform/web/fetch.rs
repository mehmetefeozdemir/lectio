//! HTTP in the browser (ADR-016, ADR-021, ADR-022), shared by the speech
//! model, the study model and the dictionary: checked downloads with
//! progress, and the small JSON requests the study providers make.
//!
//! Downloads are assembled in one JavaScript `Uint8Array` of the known size
//! rather than in WebAssembly memory: WebAssembly memory can grow but never
//! shrinks, so a 470 MB model copied through it would stay allocated for
//! the life of the tab.

use js_sys::{Reflect, Uint8Array};
use lectio_speech::models::{Checksum, ModelFile};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, ReadableStreamDefaultReader, RequestInit, Response};

use super::js_err;
use crate::platform::ai::host;

/// Report download progress about every this many bytes.
const PROGRESS_STEP: u64 = 1 << 20;

async fn send(url: &str, init: Option<&RequestInit>) -> Result<Response, String> {
    let window = web_sys::window().ok_or("no browser window")?;
    let promise = match init {
        Some(init) => window.fetch_with_str_and_init(url, init),
        None => window.fetch_with_str(url),
    };
    let unreachable = |e: JsValue| format!("Could not reach {} ({}).", host(url), js_err(e));
    JsFuture::from(promise).await.map_err(unreachable)?.dyn_into().map_err(js_err)
}

/// Fetches `file` from `url` chunk by chunk, checking its SHA-256 as it
/// arrives. `progress` gets the bytes received so far.
pub async fn download(url: &str, file: &ModelFile, mut progress: impl FnMut(u64)) -> Result<Uint8Array, String> {
    let response = send(url, None).await?;
    if !response.ok() {
        return Err(format!("{} answered {} for {}.", host(url), response.status(), file.name));
    }
    let reader: ReadableStreamDefaultReader =
        response.body().ok_or("empty download")?.get_reader().dyn_into().map_err(|_| "no stream reader".to_string())?;
    let target = Uint8Array::new_with_length(u32::try_from(file.bytes).map_err(|_| "file too large")?);
    let mut checksum = Checksum::default();
    let (mut received, mut reported) = (0u64, 0u64);
    loop {
        let chunk = JsFuture::from(reader.read()).await.map_err(|e| format!("The download stopped ({}).", js_err(e)))?;
        if Reflect::get(&chunk, &"done".into()).ok().and_then(|d| d.as_bool()).unwrap_or(true) {
            break;
        }
        let value: Uint8Array = Reflect::get(&chunk, &"value".into()).map_err(js_err)?.dyn_into().map_err(js_err)?;
        let length = u64::from(value.length());
        if received + length > file.bytes {
            return Err(format!("{} is larger than expected; the download was stopped.", file.name));
        }
        checksum.update(&value.to_vec());
        target.set(&value, received as u32);
        received += length;
        if received - reported >= PROGRESS_STEP {
            reported = received;
            progress(received);
        }
    }
    checksum.check(file)?;
    Ok(target)
}

/// GETs text; `None` when the server answers 404 (e.g. an unknown word).
pub async fn get_text(url: &str) -> Result<Option<String>, String> {
    let response = send(url, None).await?;
    if response.status() == 404 {
        return Ok(None);
    }
    if !response.ok() {
        return Err(format!("{} answered {}.", host(url), response.status()));
    }
    text(&response).await.map(Some)
}

async fn text(response: &Response) -> Result<String, String> {
    let promise = response.text().map_err(js_err)?;
    let value = JsFuture::from(promise).await.map_err(|e| format!("Could not read the answer ({}).", js_err(e)))?;
    Ok(value.as_string().unwrap_or_default())
}

/// POSTs JSON; `bearer` is an API key sent as `Authorization: Bearer …`.
/// Errors name the host only: never the key, never the request body.
pub async fn post_json(url: &str, bearer: Option<&str>, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let headers = Headers::new().map_err(js_err)?;
    headers.set("Content-Type", "application/json").map_err(js_err)?;
    if let Some(key) = bearer {
        headers.set("Authorization", &format!("Bearer {key}")).map_err(|_| "The key has characters a header can't hold.")?;
    }
    let init = RequestInit::new();
    init.set_method("POST");
    init.set_headers(&headers);
    init.set_body(&JsValue::from_str(&body.to_string()));
    let response = send(url, Some(&init)).await?;
    let answer = text(&response).await?;
    if !response.ok() {
        let detail: String = answer.chars().take(200).collect();
        return Err(format!("{} answered {}: {detail}", host(url), response.status()));
    }
    serde_json::from_str(&answer).map_err(|e| format!("{} sent an answer Lectio can't read ({e}).", host(url)))
}
