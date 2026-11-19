//! The platform boundary (ADR-010).
//!
//! The app talks only to these traits. Each target supplies its own
//! implementation, chosen at compile time:
//!
//! | Part       | Windows / macOS / Linux (`native`)   | Browser (`web`)                          |
//! |------------|--------------------------------------|------------------------------------------|
//! | storage    | `JsonFileStore`: JSON + WAV files    | IndexedDB (`web::store`)                 |
//! | recording  | `cpal` + lock-free ring (ADR-009)    | `getUserMedia` + `AudioWorklet`          |
//! | playback   | `cpal` output stream                 | Web Audio `AudioBufferSourceNode`        |
//! | blobs      | `FileBlobStore`: `<data>/blobs/`     | IndexedDB table `blobs` (ADR-012)        |
//! | file picker| `rfd` native dialog (blocking)       | `rfd` `<input type=file>` (async)        |
//! | camera     | none (file picker instead)           | `getUserMedia({video})` (ADR-013)        |
//!
//! The rules (model, clock, seek maths, resampling) stay in `lectio-core`,
//! which has no platform code and compiles for every target.

pub mod ai;
pub mod dictionary;
pub mod speech;
pub mod wav;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;
#[cfg(target_arch = "wasm32")]
pub mod web;

use std::cell::RefCell;
use std::rc::Rc;

use eframe::egui;
use lectio_core::dsp::FilterSettings;
use lectio_core::material::BlobKey;
use lectio_core::storage::LibraryStore;

/// Lectio stores speech as 16 kHz mono 16-bit WAV on every platform (ADR-005).
pub const STORE_SAMPLE_RATE: u32 = 16_000;

/// Everything the app needs from the machine it runs on.
pub trait Platform {
    fn store(&self) -> &dyn LibraryStore;
    /// Starts recording `source` into the audio item called `file_name`.
    fn start_capture(&self, file_name: &str, source: CaptureSource) -> Result<Box<dyn AudioCapture>, AudioError>;
    /// Opens the audio item called `file_name` for playback.
    fn open_playback(&self, file_name: &str) -> Result<Box<dyn AudioPlayback>, AudioError>;
    /// What to tell the student when the microphone can't be used.
    fn mic_help(&self) -> &'static str;

    // ---- Phase 2: material (ADR-012, ADR-013) ----

    /// Asks the user to choose a file. `Ok(None)` = they cancelled.
    fn pick_file(&self, kind: FileKind) -> Pending<Option<PickedFile>>;
    /// Stores a PDF/photo/sketch. In the browser the write finishes in the
    /// background; failures arrive via `LibraryStore::take_background_error`.
    fn put_blob(&self, key: &BlobKey, bytes: Vec<u8>) -> Result<(), String>;
    fn get_blob(&self, key: &BlobKey) -> Pending<Vec<u8>>;
    fn delete_blob(&self, key: &BlobKey) -> Result<(), String>;
    /// Deletes a session's audio (when its trash period ends).
    fn delete_recording(&self, file_name: &str) -> Result<(), String>;
    /// Saves text (Markdown export): a save dialog on native, a download in
    /// the browser. `Ok(false)` = the user cancelled.
    fn save_text_file(&self, suggested_name: &str, text: &str) -> Result<bool, String>;
    /// A live camera, where the platform offers one (the browser); `None`
    /// on native, where photos come from the file picker (ADR-013).
    fn open_camera(&self) -> Option<Box<dyn CameraSession>> {
        None
    }
    /// Whether to offer "Take photo" at all.
    fn supports_camera(&self) -> bool {
        false
    }

    // ---- Phase 3: text and audio (ADR-016, ADR-019) ----

    /// Starts the speech-to-text service (call once; it owns a thread or worker).
    fn speech(&self) -> Option<Box<dyn speech::SpeechService>> {
        None
    }
    /// `Err(why)` if this source can't be recorded here (shown as a tooltip).
    fn source_support(&self, source: CaptureSource) -> Result<(), &'static str> {
        match source {
            CaptureSource::Microphone => Ok(()),
            _ => Err("Recording computer audio isn't possible here."),
        }
    }
    /// A stored recording's WAV bytes (for the cleaned copy).
    fn read_recording(&self, file_name: &str) -> Pending<Vec<u8>>;
    /// Stores WAV bytes as a recording (the cleaned copy).
    fn write_recording(&self, file_name: &str, bytes: Vec<u8>) -> Result<(), String>;
    /// A picture of the whole screen, for "Screenshot to feed" (ADR-019).
    fn capture_screen(&self) -> Pending<egui::ColorImage> {
        Pending::ready(Err("Screenshots aren't possible here.".into()))
    }
    fn supports_screenshot(&self) -> bool {
        false
    }

    // ---- Phase 4a: study (ADR-021, ADR-022) ----

    /// Starts the AI provider layer (call once; it owns a thread or worker).
    fn study_model(&self) -> Option<Box<dyn ai::StudyModel>> {
        None
    }
    /// Starts the dictionary service (call once).
    fn dictionary(&self) -> Option<Box<dyn dictionary::DictionaryService>> {
        None
    }
    /// A secret (an API key) kept apart from the library; never logged.
    fn load_secret(&self, _name: &str) -> Option<String> {
        None
    }
    /// Stores (or with `None` forgets) a secret.
    fn save_secret(&self, _name: &str, _value: Option<&str>) -> Result<(), String> {
        Err("Keys can't be stored in this window.".into())
    }
}

/// What a recording listens to (ADR-019).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptureSource {
    #[default]
    Microphone,
    /// What the computer plays (an online class).
    Computer,
    /// Microphone and computer audio mixed.
    Both,
}

impl CaptureSource {
    pub const ALL: [CaptureSource; 3] = [CaptureSource::Microphone, CaptureSource::Computer, CaptureSource::Both];

    pub fn label(self) -> &'static str {
        match self {
            Self::Microphone => "Microphone",
            Self::Computer => "Computer audio",
            Self::Both => "Microphone + computer",
        }
    }
}

/// What kind of file to ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Slides,
    Photo,
}

impl FileKind {
    /// Name and extensions for the dialog's filter.
    pub fn filter(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Slides => ("PDF slides", &["pdf"]),
            Self::Photo => ("Photos", &["png", "jpg", "jpeg"]),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PickedFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// A live camera preview (the browser's `getUserMedia`). Closing = dropping.
pub trait CameraSession {
    /// A new preview frame, if one is due (about 15 per second).
    fn preview(&mut self) -> Option<egui::ColorImage>;
    fn take_error(&self) -> Option<String>;
    /// Takes a full-resolution photo now.
    fn snapshot(&mut self) -> Result<CapturedPhoto, String>;
}

pub struct CapturedPhoto {
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A result that may arrive later. Native file and blob calls finish at once
/// (`Pending::ready`); browser calls finish on a later frame. The app checks
/// with `take()` each frame. Single-threaded on purpose (`Rc`): it is only
/// used on the UI thread.
pub struct Pending<T>(Rc<RefCell<Option<Result<T, String>>>>);

impl<T> Clone for Pending<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T> Pending<T> {
    /// Only the browser finishes work later; native results are `ready` at once.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn waiting() -> Self {
        Self(Rc::new(RefCell::new(None)))
    }

    pub fn ready(result: Result<T, String>) -> Self {
        Self(Rc::new(RefCell::new(Some(result))))
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn complete(&self, result: Result<T, String>) {
        *self.0.borrow_mut() = Some(result);
    }

    /// The result, once; `None` while still waiting.
    pub fn take(&self) -> Option<Result<T, String>> {
        self.0.borrow_mut().take()
    }
}

/// A recording in progress.
pub trait AudioCapture {
    fn set_paused(&self, paused: bool);
    /// 16 kHz samples recorded so far: the recording clock (ADR-006).
    fn samples_written(&self) -> u64;
    /// Loudness of the latest audio, 0.0..=1.0, for the level meter.
    fn level(&self) -> f32;
    fn stats(&self) -> RecorderStats;
    fn take_error(&self) -> Option<String>;
    /// The last `ms` of 16 kHz audio (for live captions).
    fn recent_audio(&self, ms: u64) -> Vec<f32>;
    /// Stops and saves the audio; returns the total 16 kHz samples.
    fn finish(self: Box<Self>) -> Result<u64, AudioError>;
}

/// An open recording that can be played and seeked.
pub trait AudioPlayback {
    /// False while the audio is still loading (the browser loads it asynchronously).
    fn is_ready(&self) -> bool;
    fn take_error(&self) -> Option<String>;
    fn position_ms(&self) -> u64;
    fn is_playing(&self) -> bool;
    fn set_playing(&self, playing: bool);
    fn seek_ms(&self, offset_ms: u64);
    /// Playback speed in percent (50–200), keeping the pitch (ADR-017).
    fn set_speed(&self, percent: u16);
    /// Noise reduction / voice boost / normalise while playing (ADR-017).
    fn set_filters(&self, settings: FilterSettings);
}

/// A snapshot of what a recorder has done so far.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecorderStats {
    pub device_name: String,
    pub device_sample_rate: u32,
    pub channels: u16,
    pub sample_format: String,
    pub frames_captured: u64,
    pub frames_dropped: u64,
    pub overruns: u64,
    pub callbacks: u64,
    pub max_callback_us: u64,
    pub max_callback_frames: u64,
    pub samples_written: u64,
    /// Loudest sample seen, 0.0..=1.0. Exactly 0 usually means the OS gave
    /// us silence because microphone permission was denied.
    pub peak: f32,
}

#[derive(Debug)]
pub struct AudioError(pub String);

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AudioError {}

pub(crate) fn err(context: &str, e: impl std::fmt::Display) -> AudioError {
    AudioError(format!("{context}: {e}"))
}

/// Milliseconds since 1970. `std::time::SystemTime::now()` panics in the
/// browser, so the web build asks JavaScript's `Date` instead.
pub fn now_unix_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
    }
}
