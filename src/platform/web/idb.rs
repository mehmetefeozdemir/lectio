//! A small async wrapper over IndexedDB, the browser's built-in database.
//!
//! IndexedDB reports results through `onsuccess`/`onerror` callbacks. Each
//! helper here wraps those callbacks in a JavaScript Promise and awaits it,
//! so callers can write `idb::put(...).await?` like normal async Rust.
//!
//! Layout: database `lectio`, version 2, three object stores (tables):
//! `library` (key `library.json` → the JSON text),
//! `recordings` (key `<session-id>.wav` → the WAV bytes) and, since version 2
//! (Phase 2, ADR-012), `blobs` (key `<uuid>.pdf|png|jpg` → the file's bytes),
//! and since version 3 (Phase 3, ADR-016) `models` (key
//! `whisper-<size>/<file>` → a downloaded speech-model file).
//! Opening with a higher version makes the browser run `onupgradeneeded`,
//! which adds any missing table; an older database keeps its data.

use js_sys::{Function, Promise};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{IdbDatabase, IdbRequest, IdbTransaction, IdbTransactionMode};

use super::js_err;

const DB_NAME: &str = "lectio";
const DB_VERSION: u32 = 3;
pub const LIBRARY: &str = "library";
pub const RECORDINGS: &str = "recordings";
pub const BLOBS: &str = "blobs";
pub const MODELS: &str = "models";

pub async fn open() -> Result<IdbDatabase, String> {
    let factory = web_sys::window()
        .ok_or("no browser window")?
        .indexed_db()
        .map_err(js_err)?
        .ok_or("this browser does not offer IndexedDB storage")?;
    let request = factory.open_with_u32(DB_NAME, DB_VERSION).map_err(js_err)?;

    // Runs the first time (or when DB_VERSION goes up): create the tables.
    let upgrading = request.clone();
    let on_upgrade = Closure::<dyn FnMut(JsValue)>::new(move |_| {
        if let Some(db) = upgrading.result().ok().and_then(|r| r.dyn_into::<IdbDatabase>().ok()) {
            create_stores(&db);
        }
    });
    request.set_onupgradeneeded(Some(on_upgrade.as_ref().unchecked_ref()));
    let db = wait_request(&request).await?;
    request.set_onupgradeneeded(None);
    drop(on_upgrade);
    db.dyn_into::<IdbDatabase>().map_err(|_| "IndexedDB did not return a database".to_string())
}

fn create_stores(db: &IdbDatabase) {
    for name in [LIBRARY, RECORDINGS, BLOBS, MODELS] {
        if !db.object_store_names().contains(name) {
            // An error here surfaces as a failed open, reported by `open`.
            let _ = db.create_object_store(name);
        }
    }
}

/// The value under `key`, or `None` if there is none.
pub async fn get(db: &IdbDatabase, store: &str, key: &str) -> Result<Option<JsValue>, String> {
    let tx = db.transaction_with_str_and_mode(store, IdbTransactionMode::Readonly).map_err(js_err)?;
    let request = tx.object_store(store).map_err(js_err)?.get(&JsValue::from_str(key)).map_err(js_err)?;
    let value = wait_request(&request).await?;
    Ok((!value.is_undefined()).then_some(value))
}

/// Stores `value` under `key` and waits until the write is committed.
pub async fn put(db: &IdbDatabase, store: &str, key: &str, value: &JsValue) -> Result<(), String> {
    let tx = db.transaction_with_str_and_mode(store, IdbTransactionMode::Readwrite).map_err(js_err)?;
    tx.object_store(store).map_err(js_err)?.put_with_key(value, &JsValue::from_str(key)).map_err(js_err)?;
    wait_transaction(&tx).await
}

pub async fn delete(db: &IdbDatabase, store: &str, key: &str) -> Result<(), String> {
    let tx = db.transaction_with_str_and_mode(store, IdbTransactionMode::Readwrite).map_err(js_err)?;
    tx.object_store(store).map_err(js_err)?.delete(&JsValue::from_str(key)).map_err(js_err)?;
    wait_transaction(&tx).await
}

/// Resolves with the request's result when `onsuccess` fires.
async fn wait_request(request: &IdbRequest) -> Result<JsValue, String> {
    let promise = Promise::new(&mut |resolve: Function, reject: Function| {
        let done = request.clone();
        let on_success = Closure::once_into_js(move |_: JsValue| {
            let value = done.result().unwrap_or(JsValue::UNDEFINED);
            let _ = resolve.call1(&JsValue::NULL, &value);
        });
        let failed = request.clone();
        let on_error = Closure::once_into_js(move |_: JsValue| {
            let error = failed.error().ok().flatten().map(JsValue::from).unwrap_or(JsValue::NULL);
            let _ = reject.call1(&JsValue::NULL, &error);
        });
        request.set_onsuccess(Some(on_success.unchecked_ref()));
        request.set_onerror(Some(on_error.unchecked_ref()));
    });
    JsFuture::from(promise).await.map_err(js_err)
}

/// Resolves when the transaction commits; rejects if it fails or aborts
/// (for example when the browser's storage quota is full).
async fn wait_transaction(tx: &IdbTransaction) -> Result<(), String> {
    let promise = Promise::new(&mut |resolve: Function, reject: Function| {
        let on_complete = Closure::once_into_js(move |_: JsValue| {
            let _ = resolve.call0(&JsValue::NULL);
        });
        let (failed, aborted) = (tx.clone(), tx.clone());
        let reject_abort = reject.clone();
        let on_error = Closure::once_into_js(move |_: JsValue| {
            let error = failed.error().map(JsValue::from).unwrap_or(JsValue::NULL);
            let _ = reject.call1(&JsValue::NULL, &error);
        });
        let on_abort = Closure::once_into_js(move |_: JsValue| {
            let error = aborted.error().map(JsValue::from).unwrap_or(JsValue::from_str("the write was aborted"));
            let _ = reject_abort.call1(&JsValue::NULL, &error);
        });
        tx.set_oncomplete(Some(on_complete.unchecked_ref()));
        tx.set_onerror(Some(on_error.unchecked_ref()));
        tx.set_onabort(Some(on_abort.unchecked_ref()));
    });
    JsFuture::from(promise).await.map(|_| ()).map_err(js_err)
}
