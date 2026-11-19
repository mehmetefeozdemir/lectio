//! Windows, macOS and Linux: files on disk, `cpal` audio, `rfd` dialogs.

mod player;
mod recorder;
mod speech;

use std::path::PathBuf;

use lectio_core::material::BlobKey;
use lectio_core::storage::{FileBlobStore, JsonFileStore, LibraryStore};

pub use player::Player;
pub use recorder::Recorder;
pub use speech::run_cli as transcribe_cli;

use eframe::egui;

use super::{AudioCapture, AudioError, AudioPlayback, CaptureSource, FileKind, Pending, PickedFile, Platform};

pub struct NativePlatform {
    store: JsonFileStore,
    blobs: FileBlobStore,
}

impl NativePlatform {
    /// Uses `LECTIO_DATA_DIR` if set, else the OS app-data folder.
    pub fn open() -> Self {
        let dir = data_dir();
        Self { store: JsonFileStore::new(&dir), blobs: FileBlobStore::new(&dir) }
    }
}

impl Platform for NativePlatform {
    fn store(&self) -> &dyn LibraryStore {
        &self.store
    }

    fn start_capture(&self, file_name: &str, source: CaptureSource) -> Result<Box<dyn AudioCapture>, AudioError> {
        Ok(Box::new(Recorder::start(&self.store.audio_path(file_name), source)?))
    }

    fn open_playback(&self, file_name: &str) -> Result<Box<dyn AudioPlayback>, AudioError> {
        Ok(Box::new(Player::open(&self.store.audio_path(file_name))?))
    }

    fn mic_help(&self) -> &'static str {
        if cfg!(target_os = "macos") {
            "Allow microphone access for the app you started Lectio from \
             (System Settings > Privacy & Security > Microphone)."
        } else if cfg!(target_os = "windows") {
            "Allow desktop apps to use the microphone (Settings > Privacy & security > Microphone)."
        } else {
            "Check that a microphone is connected and set as the default input (PulseAudio/PipeWire settings)."
        }
    }

    /// Opens the OS dialog and waits for it. That pauses drawing while the
    /// dialog is open, but recording keeps going: audio runs on its own
    /// threads (ADR-009). macOS requires dialogs on the main thread, which
    /// is where egui calls us.
    fn pick_file(&self, kind: FileKind) -> Pending<Option<PickedFile>> {
        let (name, extensions) = kind.filter();
        let Some(path) = rfd::FileDialog::new().add_filter(name, extensions).pick_file() else {
            return Pending::ready(Ok(None));
        };
        let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Pending::ready(
            std::fs::read(&path)
                .map(|bytes| Some(PickedFile { name: file_name, bytes }))
                .map_err(|e| format!("Could not read {}: {e}", path.display())),
        )
    }

    fn put_blob(&self, key: &BlobKey, bytes: Vec<u8>) -> Result<(), String> {
        self.blobs.put(key, &bytes).map_err(|e| e.to_string())
    }

    fn get_blob(&self, key: &BlobKey) -> Pending<Vec<u8>> {
        Pending::ready(self.blobs.get(key).map_err(|e| e.to_string()))
    }

    fn delete_recording(&self, file_name: &str) -> Result<(), String> {
        match std::fs::remove_file(self.store.audio_path(file_name)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }

    fn save_text_file(&self, suggested_name: &str, text: &str) -> Result<bool, String> {
        let Some(path) = rfd::FileDialog::new().add_filter("Markdown", &["md"]).set_file_name(suggested_name).save_file()
        else {
            return Ok(false);
        };
        std::fs::write(&path, text).map(|()| true).map_err(|e| format!("Could not save {}: {e}", path.display()))
    }

    fn delete_blob(&self, key: &BlobKey) -> Result<(), String> {
        self.blobs.delete(key).map_err(|e| e.to_string())
    }

    fn speech(&self) -> Option<Box<dyn crate::platform::speech::SpeechService>> {
        let dir = data_dir();
        let store = JsonFileStore::new(&dir);
        Some(Box::new(speech::NativeSpeech::start(models_dir(), move |name| store.audio_path(name))))
    }

    /// Computer audio is offered on every desktop; if the OS refuses (old
    /// macOS, no permission, no monitor source) starting explains why.
    fn source_support(&self, _source: CaptureSource) -> Result<(), &'static str> {
        Ok(())
    }

    fn read_recording(&self, file_name: &str) -> Pending<Vec<u8>> {
        let path = self.store.audio_path(file_name);
        Pending::ready(std::fs::read(&path).map_err(|e| format!("Could not read {}: {e}", path.display())))
    }

    /// Written next to the original, via a temporary file and a rename, so a
    /// crash never leaves a half-written copy under the real name.
    fn write_recording(&self, file_name: &str, bytes: Vec<u8>) -> Result<(), String> {
        let path = self.store.audio_path(file_name);
        let temporary = path.with_extension("wav.part");
        std::fs::write(&temporary, bytes).map_err(|e| format!("Could not save {}: {e}", path.display()))?;
        std::fs::rename(&temporary, &path).map_err(|e| format!("Could not save {}: {e}", path.display()))
    }

    /// The main display, via `xcap` (macOS asks for Screen Recording
    /// permission the first time).
    fn capture_screen(&self) -> Pending<egui::ColorImage> {
        Pending::ready(capture_main_screen())
    }

    fn supports_screenshot(&self) -> bool {
        true
    }
}

fn capture_main_screen() -> Result<egui::ColorImage, String> {
    let monitors = xcap::Monitor::all().map_err(|e| format!("Could not see the screen: {e}"))?;
    let monitor = monitors
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or_else(|| monitors.first())
        .ok_or("No screen was found.")?;
    let image = monitor.capture_image().map_err(|e| {
        format!("Could not take a screenshot ({e}). On macOS, allow Screen Recording for the app you started Lectio from.")
    })?;
    let size = [image.width() as usize, image.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()))
}


/// Where downloaded speech models are kept.
pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("LECTIO_DATA_DIR") {
        return PathBuf::from(dir);
    }
    directories::ProjectDirs::from("org", "Lectio", "Lectio")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("lectio-data"))
}
