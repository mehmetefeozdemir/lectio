//! Desktop dictionary (ADR-022): a background thread that downloads Open
//! English WordNet once (checked by SHA-256), turns it into a compact index
//! file in `<data>/dictionary/`, keeps the index in memory, and answers
//! look-ups, asking the online provider only when allowed.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use lectio_core::dictionary::{ATTRIBUTION, DefinitionProvider, DictIndex, FreeDictionary, index_from_jsonl};

use super::fetch;
use crate::platform::dictionary::{
    DictEvent, DictRequest, DictionaryService, INDEX_NAME, WORDNET_FILE, WORDNET_URL,
};

pub struct NativeDictionary {
    requests: Sender<DictRequest>,
    events: Mutex<Receiver<DictEvent>>,
    ready: Arc<AtomicBool>,
}

impl NativeDictionary {
    pub fn start(dir: PathBuf) -> Self {
        let (requests, inbox) = mpsc::channel();
        let (outbox, events) = mpsc::channel();
        let ready = Arc::new(AtomicBool::new(dir.join(INDEX_NAME).exists()));
        let worker = Worker { dir, index: None, events: outbox, ready: Arc::clone(&ready) };
        std::thread::Builder::new()
            .name("lectio-dictionary".into())
            .spawn(move || worker.run(inbox))
            .expect("the dictionary thread could not start");
        Self { requests, events: Mutex::new(events), ready }
    }
}

impl DictionaryService for NativeDictionary {
    fn send(&self, request: DictRequest) {
        let _ = self.requests.send(request);
    }

    fn poll(&self) -> Vec<DictEvent> {
        self.events.lock().map(|rx| rx.try_iter().collect()).unwrap_or_default()
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }
}

struct Worker {
    dir: PathBuf,
    index: Option<DictIndex>,
    events: Sender<DictEvent>,
    ready: Arc<AtomicBool>,
}

impl Worker {
    fn run(mut self, inbox: Receiver<DictRequest>) {
        while let Ok(request) = inbox.recv() {
            match request {
                DictRequest::Prepare => match self.ensure_index(true) {
                    Ok(()) => self.emit(DictEvent::Ready),
                    Err(message) => self.emit(DictEvent::Failed { message }),
                },
                DictRequest::Lookup { word, online } => self.lookup(&word, online),
            }
        }
    }

    fn emit(&self, event: DictEvent) {
        let _ = self.events.send(event);
    }

    fn lookup(&mut self, word: &str, online: bool) {
        // Offline first (without downloading anything unasked).
        if self.ensure_index(false).is_ok()
            && let Some(index) = &self.index
        {
            let senses = index.lookup(word);
            if !senses.is_empty() {
                return self.emit(DictEvent::Found { word: word.to_owned(), senses, source: ATTRIBUTION.to_owned() });
            }
        }
        if !online {
            let message = if self.index.is_some() {
                format!("\"{word}\" isn't in the offline dictionary.")
            } else {
                "Download the offline dictionary first (or allow online look-ups in Settings).".to_owned()
            };
            return self.emit(DictEvent::Failed { message });
        }
        let provider = FreeDictionary;
        let found = fetch::get_text(&provider.url(word)).map(|body| body.map(|b| provider.parse(&b)).unwrap_or_default());
        self.emit(match found {
            Ok(senses) if !senses.is_empty() => DictEvent::Found { word: word.to_owned(), senses, source: provider.name().to_owned() },
            Ok(_) => DictEvent::Failed { message: format!("No definition found for \"{word}\".") },
            Err(message) => DictEvent::Failed { message },
        });
    }

    /// Loads the index file; with `download`, makes it first if missing.
    fn ensure_index(&mut self, download: bool) -> Result<(), String> {
        if self.index.is_some() {
            return Ok(());
        }
        let index_path = self.dir.join(INDEX_NAME);
        if !index_path.exists() {
            if !download {
                return Err("not downloaded".into());
            }
            let raw = self.dir.join(WORDNET_FILE.name);
            let events = self.events.clone();
            fetch::download(WORDNET_URL, &WORDNET_FILE, &raw, 0, WORDNET_FILE.bytes, &mut |done, total| {
                let _ = events.send(DictEvent::Downloading { done, total });
            })?;
            self.emit(DictEvent::Indexing);
            let jsonl = std::fs::read_to_string(&raw).map_err(|e| format!("Could not read the dictionary: {e}"))?;
            std::fs::write(&index_path, index_from_jsonl(&jsonl)).map_err(|e| format!("Could not save the dictionary: {e}"))?;
            // The index is all that is needed from now on.
            let _ = std::fs::remove_file(&raw);
        }
        let tsv = std::fs::read_to_string(&index_path).map_err(|e| format!("Could not read the dictionary: {e}"))?;
        self.index = Some(DictIndex::from_tsv(&tsv));
        self.ready.store(true, Ordering::Relaxed);
        Ok(())
    }
}
