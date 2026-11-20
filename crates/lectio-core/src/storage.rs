//! Local-first storage (see ADR-004).
//!
//! `LibraryStore` is the boundary between the app and wherever data lives
//! (ADR-010). Native builds use `JsonFileStore` (a JSON file plus a folder of
//! WAV files); the web build implements the same trait over IndexedDB in the
//! app crate. Both use `interpret_saved` to decide what a saved text means.
//!
//! `JsonFileStore` only uses `std::fs`, which compiles on every target, so
//! this crate still has no platform-specific code.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::material::BlobKey;
use crate::{Library, ModelError};

const LIBRARY_FILE: &str = "library.json";
const RECORDINGS_DIR: &str = "recordings";
const BLOBS_DIR: &str = "blobs";

#[derive(Debug)]
pub enum StoreError {
    Io { path: PathBuf, source: io::Error },
    Model(ModelError),
    /// A store with its own storage engine (e.g. the browser's IndexedDB).
    Backend(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "Could not use {}: {source}", path.display()),
            Self::Model(e) => e.fmt(f),
            Self::Backend(message) => write!(f, "Could not use the browser's storage: {message}"),
        }
    }
}

impl std::error::Error for StoreError {}

/// What `load` found. `recovered_from` is set when the saved data was
/// unreadable and was moved aside; it says where, in words the UI can show
/// (a file path on native, a storage key on the web).
#[derive(Debug)]
pub struct LoadOutcome {
    pub library: Library,
    pub recovered_from: Option<String>,
}

pub trait LibraryStore {
    fn load(&self) -> Result<LoadOutcome, StoreError>;
    fn save(&self, library: &Library) -> Result<(), StoreError>;

    /// Stores that finish saving in the background (the browser's storage
    /// is asynchronous) report failures here; the app polls it every frame.
    fn take_background_error(&self) -> Option<StoreError> {
        None
    }
}

/// What a saved library text turned out to be.
#[derive(Debug, PartialEq)]
pub enum SavedData {
    /// Boxed: a library is much bigger than the other variants.
    Valid(Box<Library>),
    /// Written by a newer Lectio: valid data we can't read. Never overwrite it.
    Newer(ModelError),
    /// Damaged: move it aside and start fresh.
    Unreadable,
}

/// The one rule every store follows for saved text.
pub fn interpret_saved(text: &str) -> SavedData {
    match Library::from_json(text) {
        Ok(library) => SavedData::Valid(Box::new(library)),
        Err(e @ ModelError::UnsupportedSchema { .. }) => SavedData::Newer(e),
        Err(_) => SavedData::Unreadable,
    }
}

pub struct JsonFileStore {
    dir: PathBuf,
}

impl JsonFileStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn audio_path(&self, file_name: &str) -> PathBuf {
        self.dir.join(RECORDINGS_DIR).join(file_name)
    }

    pub fn recordings_dir(&self) -> PathBuf {
        self.dir.join(RECORDINGS_DIR)
    }

    fn library_path(&self) -> PathBuf {
        self.dir.join(LIBRARY_FILE)
    }

    /// Renames an unreadable file to `library.json.broken-<unix ms>`.
    fn move_aside(&self, path: &Path) -> Result<PathBuf, StoreError> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let backup = self.dir.join(format!("{LIBRARY_FILE}.broken-{stamp}"));
        fs::rename(path, &backup).map_err(|source| StoreError::Io { path: path.to_owned(), source })?;
        Ok(backup)
    }
}

impl LibraryStore for JsonFileStore {
    fn load(&self) -> Result<LoadOutcome, StoreError> {
        let path = self.library_path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(LoadOutcome { library: Library::default(), recovered_from: None });
            }
            Err(source) => return Err(StoreError::Io { path, source }),
        };
        match interpret_saved(&text) {
            SavedData::Valid(library) => Ok(LoadOutcome { library: *library, recovered_from: None }),
            // A newer app's file is valid data we can't read: never touch it.
            SavedData::Newer(e) => Err(StoreError::Model(e)),
            SavedData::Unreadable => {
                let backup = self.move_aside(&path)?;
                let recovered_from = Some(backup.display().to_string());
                Ok(LoadOutcome { library: Library::default(), recovered_from })
            }
        }
    }

    /// Writes to a temporary file, then renames it over `library.json`.
    /// A rename is atomic on the same disk, so a crash mid-save leaves either
    /// the old file or the new one, never half of each.
    fn save(&self, library: &Library) -> Result<(), StoreError> {
        let json = library.to_json().map_err(StoreError::Model)?;
        write_atomically(&self.library_path(), json.as_bytes())
    }
}

/// Writes `bytes` to `<path>.tmp`, then renames it over `path`, creating the
/// folder if needed. Readers see the old file or the new one, never half.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let io_err = |at: &Path| {
        let at = at.to_owned();
        move |source| StoreError::Io { path: at, source }
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io_err(dir))?;
    }
    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp = PathBuf::from(tmp_name);
    fs::write(&tmp, bytes).map_err(io_err(&tmp))?;
    fs::rename(&tmp, path).map_err(io_err(path))
}

/// Big files (PDFs, photos, sketches) on native: one file per blob in
/// `<data>/blobs/`, named by its validated `BlobKey` (ADR-012). The web build
/// keeps the same keys in IndexedDB instead.
pub struct FileBlobStore {
    dir: PathBuf,
}

impl FileBlobStore {
    /// `data_dir` is the same folder `JsonFileStore` uses.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self { dir: data_dir.into().join(BLOBS_DIR) }
    }

    /// Safe by construction: a `BlobKey` can only be `<uuid>.<pdf|png|jpg>`.
    pub fn path(&self, key: &BlobKey) -> PathBuf {
        self.dir.join(key.as_str())
    }

    pub fn put(&self, key: &BlobKey, bytes: &[u8]) -> Result<(), StoreError> {
        write_atomically(&self.path(key), bytes)
    }

    pub fn get(&self, key: &BlobKey) -> Result<Vec<u8>, StoreError> {
        let path = self.path(key);
        fs::read(&path).map_err(|source| StoreError::Io { path, source })
    }

    /// Deleting a blob that is already gone is not an error.
    pub fn delete(&self, key: &BlobKey) -> Result<(), StoreError> {
        let path = self.path(key);
        match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(StoreError::Io { path, source: e }),
            _ => Ok(()),
        }
    }
}
