//! The browser: IndexedDB storage, `getUserMedia` + `AudioWorklet`
//! recording, Web Audio playback (ADR-010).
//!
//! Browser APIs are asynchronous (they return Promises). The app's traits are
//! synchronous because egui redraws every frame, so the web side starts the
//! async work with `spawn_local` and the app polls for the result
//! (`is_ready`, `take_error`, `take_background_error`) on later frames.

mod camera;
mod files;
mod idb;
mod player;
mod recorder;
mod store;

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::Uint8Array;
use lectio_core::material::BlobKey;
use lectio_core::storage::LibraryStore;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::IdbDatabase;

use super::{AudioCapture, AudioError, AudioPlayback, CameraSession, FileKind, Pending, PickedFile, Platform};
use camera::WebCamera;
use player::WebPlayer;
use recorder::WebRecorder;
use store::IdbLibraryStore;

/// Where background tasks (saving, storing audio) report a failure; the app
/// shows it on the next frame.
pub(crate) type ErrorSink = Rc<RefCell<Option<String>>>;

pub(crate) fn report(sink: &ErrorSink, message: String) {
    *sink.borrow_mut() = Some(message);
}

pub struct WebPlatform {
    db: IdbDatabase,
    store: IdbLibraryStore,
    errors: ErrorSink,
}

impl WebPlatform {
    /// Opens the browser database and reads the saved library.
    pub async fn open() -> Result<Self, String> {
        let db = idb::open().await?;
        let errors = ErrorSink::default();
        let store = IdbLibraryStore::open(db.clone(), errors.clone()).await?;
        Ok(Self { db, store, errors })
    }
}

impl Platform for WebPlatform {
    fn store(&self) -> &dyn LibraryStore {
        &self.store
    }

    fn start_capture(&self, file_name: &str) -> Result<Box<dyn AudioCapture>, AudioError> {
        Ok(Box::new(WebRecorder::start(self.db.clone(), file_name, self.errors.clone())))
    }

    fn open_playback(&self, file_name: &str) -> Result<Box<dyn AudioPlayback>, AudioError> {
        Ok(Box::new(WebPlayer::open(self.db.clone(), file_name)))
    }

    fn mic_help(&self) -> &'static str {
        "Allow this page to use the microphone (the icon in the address bar). \
         Browsers only allow recording on https:// pages or on localhost."
    }

    fn pick_file(&self, kind: FileKind) -> Pending<Option<PickedFile>> {
        files::pick(kind)
    }

    /// Queued like library saves; the IndexedDB `blobs` table (ADR-012).
    fn put_blob(&self, key: &BlobKey, bytes: Vec<u8>) -> Result<(), String> {
        let (db, key, errors) = (self.db.clone(), key.as_str().to_owned(), self.errors.clone());
        let value = Uint8Array::from(bytes.as_slice());
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = idb::put(&db, idb::BLOBS, &key, &value).await {
                report(&errors, format!("a file could not be stored ({e})"));
            }
        });
        Ok(())
    }

    fn get_blob(&self, key: &BlobKey) -> Pending<Vec<u8>> {
        let pending = Pending::waiting();
        let (db, key, done) = (self.db.clone(), key.as_str().to_owned(), pending.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let result = idb::get(&db, idb::BLOBS, &key).await.and_then(|value| {
                value
                    .map(|v| Uint8Array::new(&v).to_vec())
                    .ok_or_else(|| "This file is not in this browser's storage.".to_string())
            });
            done.complete(result);
        });
        pending
    }

    fn delete_recording(&self, file_name: &str) -> Result<(), String> {
        let (db, key, errors) = (self.db.clone(), file_name.to_owned(), self.errors.clone());
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = idb::delete(&db, idb::RECORDINGS, &key).await {
                report(&errors, format!("an old recording could not be removed ({e})"));
            }
        });
        Ok(())
    }

    /// Offers the text as a download: a temporary `blob:` URL on an `<a download>`.
    fn save_text_file(&self, suggested_name: &str, text: &str) -> Result<bool, String> {
        let document = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
        let parts = js_sys::Array::of1(&JsValue::from_str(text));
        let options = web_sys::BlobPropertyBag::new();
        options.set_type("text/markdown");
        let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options).map_err(js_err)?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js_err)?;
        let link: web_sys::HtmlAnchorElement =
            document.create_element("a").map_err(js_err)?.dyn_into().map_err(|_| "no <a> element")?;
        link.set_href(&url);
        link.set_download(suggested_name);
        link.click();
        let _ = web_sys::Url::revoke_object_url(&url);
        Ok(true)
    }

    fn delete_blob(&self, key: &BlobKey) -> Result<(), String> {
        let (db, key, errors) = (self.db.clone(), key.as_str().to_owned(), self.errors.clone());
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = idb::delete(&db, idb::BLOBS, &key).await {
                report(&errors, format!("an unused file could not be removed ({e})"));
            }
        });
        Ok(())
    }

    fn open_camera(&self) -> Option<Box<dyn CameraSession>> {
        Some(Box::new(WebCamera::open()))
    }

    fn supports_camera(&self) -> bool {
        true
    }
}

/// A readable message from a JavaScript error value.
pub(crate) fn js_err(value: JsValue) -> String {
    if let Some(e) = value.dyn_ref::<web_sys::DomException>() {
        return format!("{}: {}", e.name(), e.message());
    }
    if let Some(e) = value.dyn_ref::<js_sys::Error>() {
        return String::from(e.message());
    }
    value.as_string().unwrap_or_else(|| format!("{value:?}"))
}
