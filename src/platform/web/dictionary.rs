//! Browser dictionary (ADR-022). The Open English WordNet download (checked
//! by SHA-256) is turned into the compact index by a short-lived copy of
//! the worker program, so the page never freezes or keeps the 47 MB text in
//! its own memory. The index is kept in IndexedDB's `models` table and read
//! into memory on the first look-up.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use js_sys::{Array, Promise, Uint8Array};
use lectio_core::dictionary::{ATTRIBUTION, DefinitionProvider, DictIndex, FreeDictionary};
use lectio_llm::protocol::{FromStudy, ToStudy};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{IdbDatabase, MessageEvent, Worker};

use super::speech::WORKER_URL;
use super::{fetch, idb, js_err};
use crate::platform::dictionary::{DictEvent, DictRequest, DictionaryService, INDEX_NAME, WORDNET_FILE, WORDNET_URL};

struct Inner {
    db: IdbDatabase,
    index: RefCell<Option<Rc<DictIndex>>>,
    /// The index is stored in this browser.
    stored: Cell<bool>,
    events: RefCell<Vec<DictEvent>>,
}

pub struct WebDictionary {
    inner: Rc<Inner>,
}

impl WebDictionary {
    pub fn start(db: IdbDatabase) -> Self {
        let inner = Rc::new(Inner { db, index: RefCell::new(None), stored: Cell::new(false), events: RefCell::new(Vec::new()) });
        let checker = Rc::clone(&inner);
        wasm_bindgen_futures::spawn_local(async move {
            if matches!(idb::get(&checker.db, idb::MODELS, INDEX_NAME).await, Ok(Some(_))) {
                checker.stored.set(true);
            }
        });
        Self { inner }
    }
}

/// Waits for the worker's next message and returns its parts.
async fn next_message(worker: &Worker) -> Result<Array, String> {
    let promise = Promise::new(&mut |resolve, reject| {
        worker.set_onmessage(Some(&resolve));
        worker.set_onerror(Some(&reject));
    });
    let event: MessageEvent = JsFuture::from(promise)
        .await
        .map_err(|_| "The dictionary could not be prepared (the browser may be out of memory).".to_string())?
        .unchecked_into();
    Ok(event.data().unchecked_into())
}

fn study_event(parts: &Array) -> Option<FromStudy> {
    parts.get(0).as_string().and_then(|json| serde_json::from_str(&json).ok())
}

/// Has a fresh worker turn the JSON Lines into the index text.
async fn build_in_worker(jsonl: Uint8Array) -> Result<Uint8Array, String> {
    let worker = Worker::new(WORKER_URL).map_err(js_err)?;
    let result = async {
        // Wait until it listens, or the message could be lost.
        while study_event(&next_message(&worker).await?) != Some(FromStudy::Started) {}
        let json = serde_json::to_string(&ToStudy::BuildIndex).map_err(|e| e.to_string())?;
        let message = Array::of2(&JsValue::from_str(&json), &jsonl);
        worker.post_message_with_transfer(&message, &Array::of1(&jsonl.buffer())).map_err(js_err)?;
        let reply = next_message(&worker).await?;
        match study_event(&reply) {
            Some(FromStudy::Index) => Ok(reply.get(1).unchecked_into::<Uint8Array>()),
            Some(FromStudy::Failed { message, .. }) => Err(message),
            _ => Err("The dictionary could not be prepared.".to_string()),
        }
    }
    .await;
    worker.terminate();
    result
}

impl Inner {
    fn emit(&self, event: DictEvent) {
        self.events.borrow_mut().push(event);
    }

    /// Downloads and indexes the dictionary (if it isn't stored yet).
    async fn prepare(&self) -> Result<(), String> {
        if self.stored.get() {
            return Ok(());
        }
        let total = WORDNET_FILE.bytes;
        let jsonl = fetch::download(WORDNET_URL, &WORDNET_FILE, |done| self.emit(DictEvent::Downloading { done, total })).await?;
        self.emit(DictEvent::Indexing);
        let tsv = build_in_worker(jsonl).await?;
        idb::put(&self.db, idb::MODELS, INDEX_NAME, &tsv).await?;
        self.stored.set(true);
        Ok(())
    }

    /// The index in memory, read from storage on first use.
    async fn index(&self) -> Option<Rc<DictIndex>> {
        if let Some(index) = self.index.borrow().as_ref() {
            return Some(Rc::clone(index));
        }
        if !self.stored.get() {
            return None;
        }
        let value = idb::get(&self.db, idb::MODELS, INDEX_NAME).await.ok().flatten()?;
        let tsv = String::from_utf8_lossy(&Uint8Array::new(&value).to_vec()).into_owned();
        let index = Rc::new(DictIndex::from_tsv(&tsv));
        *self.index.borrow_mut() = Some(Rc::clone(&index));
        Some(index)
    }

    async fn lookup(&self, word: String, online: bool) {
        let index = self.index().await;
        if let Some(index) = &index {
            let senses = index.lookup(&word);
            if !senses.is_empty() {
                return self.emit(DictEvent::Found { word, senses, source: ATTRIBUTION.to_owned() });
            }
        }
        if !online {
            let message = if index.is_some() {
                format!("\"{word}\" isn't in the offline dictionary.")
            } else {
                "Download the offline dictionary first (or allow online look-ups in Settings).".to_owned()
            };
            return self.emit(DictEvent::Failed { message });
        }
        let provider = FreeDictionary;
        let found = fetch::get_text(&provider.url(&word)).await.map(|body| body.map(|b| provider.parse(&b)).unwrap_or_default());
        self.emit(match found {
            Ok(senses) if !senses.is_empty() => DictEvent::Found { word, senses, source: provider.name().to_owned() },
            Ok(_) => DictEvent::Failed { message: format!("No definition found for \"{word}\".") },
            Err(message) => DictEvent::Failed { message },
        });
    }
}

impl DictionaryService for WebDictionary {
    fn send(&self, request: DictRequest) {
        let inner = Rc::clone(&self.inner);
        wasm_bindgen_futures::spawn_local(async move {
            match request {
                DictRequest::Prepare => match inner.prepare().await {
                    Ok(()) => inner.emit(DictEvent::Ready),
                    Err(message) => inner.emit(DictEvent::Failed { message }),
                },
                DictRequest::Lookup { word, online } => inner.lookup(word, online).await,
            }
        });
    }

    fn poll(&self) -> Vec<DictEvent> {
        std::mem::take(&mut *self.inner.events.borrow_mut())
    }

    fn is_ready(&self) -> bool {
        self.inner.stored.get()
    }
}
