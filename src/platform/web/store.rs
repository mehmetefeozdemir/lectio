//! `LibraryStore` over IndexedDB. Same rules as `JsonFileStore`
//! (`lectio_core::storage::interpret_saved`): a newer format is never
//! touched, damaged data is moved aside under a `library.json.broken-<time>` key.
//!
//! IndexedDB is asynchronous, so the library is read once *before* the app
//! starts (`open`), and `save` queues the write in the background. Writes run
//! in the order they were queued, because IndexedDB runs read-write
//! transactions on the same table in the order they were created.

use std::cell::RefCell;

use lectio_core::Library;
use lectio_core::storage::{LibraryStore, LoadOutcome, SavedData, StoreError, interpret_saved};
use wasm_bindgen::JsValue;
use web_sys::IdbDatabase;

use super::{ErrorSink, idb, report};
use crate::platform::now_unix_ms;

const LIBRARY_KEY: &str = "library.json";

pub struct IdbLibraryStore {
    db: IdbDatabase,
    initial: RefCell<Option<Result<LoadOutcome, StoreError>>>,
    errors: ErrorSink,
}

impl IdbLibraryStore {
    pub async fn open(db: IdbDatabase, errors: ErrorSink) -> Result<Self, String> {
        let initial = read_saved(&db).await?;
        Ok(Self { db, initial: RefCell::new(Some(initial)), errors })
    }
}

async fn read_saved(db: &IdbDatabase) -> Result<Result<LoadOutcome, StoreError>, String> {
    let Some(value) = idb::get(db, idb::LIBRARY, LIBRARY_KEY).await? else {
        return Ok(Ok(LoadOutcome { library: Library::default(), recovered_from: None }));
    };
    let text = value.as_string().unwrap_or_default();
    Ok(match interpret_saved(&text) {
        SavedData::Valid(library) => Ok(LoadOutcome { library: *library, recovered_from: None }),
        SavedData::Newer(e) => Err(StoreError::Model(e)),
        SavedData::Unreadable => {
            let backup_key = format!("{LIBRARY_KEY}.broken-{}", now_unix_ms());
            idb::put(db, idb::LIBRARY, &backup_key, &value).await?;
            idb::delete(db, idb::LIBRARY, LIBRARY_KEY).await?;
            let place = format!("this browser's storage (IndexedDB \"lectio\", key \"{backup_key}\")");
            Ok(LoadOutcome { library: Library::default(), recovered_from: Some(place) })
        }
    })
}

impl LibraryStore for IdbLibraryStore {
    fn load(&self) -> Result<LoadOutcome, StoreError> {
        self.initial
            .borrow_mut()
            .take()
            .unwrap_or_else(|| Err(StoreError::Backend("the library was already loaded".into())))
    }

    fn save(&self, library: &Library) -> Result<(), StoreError> {
        let json = library.to_json().map_err(StoreError::Model)?;
        let (db, errors) = (self.db.clone(), self.errors.clone());
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = idb::put(&db, idb::LIBRARY, LIBRARY_KEY, &JsValue::from_str(&json)).await {
                report(&errors, e);
            }
        });
        Ok(())
    }

    fn take_background_error(&self) -> Option<StoreError> {
        self.errors.borrow_mut().take().map(StoreError::Backend)
    }
}
