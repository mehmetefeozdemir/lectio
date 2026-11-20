//! Windows, macOS and Linux: files on disk, `cpal` audio, `rfd` dialogs.

mod ai;
mod dictionary;
mod fetch;
mod secrets;
mod player;
mod recorder;
mod speech;

use std::path::PathBuf;

use lectio_core::material::BlobKey;
use lectio_core::storage::{FileBlobStore, JsonFileStore, LibraryStore};

pub use player::Player;
pub use recorder::Recorder;
pub use ai::run_benchmark as ai_benchmark;
pub use speech::run_cli as transcribe_cli;

use eframe::egui;

use super::{AudioCapture, AudioError, AudioPlayback, CaptureSource, DocFormat, FileKind, Pending, PickedFile, Platform};

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

    fn save_file(&self, suggested_name: &str, bytes: &[u8], format: DocFormat) -> Result<bool, String> {
        let dialog = rfd::FileDialog::new().add_filter(format.name(), &[format.extension()]).set_file_name(suggested_name);
        let Some(path) = dialog.save_file() else {
            return Ok(false);
        };
        std::fs::write(&path, bytes).map(|()| true).map_err(|e| format!("Could not save {}: {e}", path.display()))
    }

    /// Writes the PDF to a temporary folder and opens it in the system's
    /// viewer, where the user prints it (each OS has its own print dialog).
    fn print(&self, title: &str, document: Vec<u8>) -> Result<(), String> {
        let folder = std::env::temp_dir().join("lectio-print");
        std::fs::create_dir_all(&folder).map_err(|e| format!("Could not prepare printing: {e}"))?;
        let safe: String = title.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
        let path = folder.join(format!("{}.pdf", safe.trim()));
        std::fs::write(&path, document).map_err(|e| format!("Could not prepare printing: {e}"))?;
        open_in_viewer(&path)
    }

    fn delete_blob(&self, key: &BlobKey) -> Result<(), String> {
        self.blobs.delete(key).map_err(|e| e.to_string())
    }

    fn speech(&self) -> Option<Box<dyn crate::platform::speech::SpeechService>> {
        let dir = data_dir();
        let store = JsonFileStore::new(&dir);
        Some(Box::new(speech::NativeSpeech::start(models_dir(), move |name| store.audio_path(name))))
    }

    fn study_model(&self) -> Option<Box<dyn crate::platform::ai::StudyModel>> {
        Some(Box::new(ai::NativeStudyModel::start(models_dir())))
    }

    fn dictionary(&self) -> Option<Box<dyn crate::platform::dictionary::DictionaryService>> {
        Some(Box::new(dictionary::NativeDictionary::start(data_dir().join("dictionary"))))
    }

    /// Keys live in `<data>/secrets.json`, apart from the library (which a
    /// user may share), readable only by the user on macOS and Linux.
    fn load_secret(&self, name: &str) -> Option<String> {
        secrets::load(&data_dir().join(secrets::FILE_NAME), name)
    }

    fn save_secret(&self, name: &str, value: Option<&str>) -> Result<(), String> {
        secrets::save(&data_dir().join(secrets::FILE_NAME), name, value)
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
/// Opens a file with the system's default app (the PDF viewer for printing).
fn open_in_viewer(path: &std::path::Path) -> Result<(), String> {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    command.arg(path).spawn().map(|_| ()).map_err(|e| format!("Could not open the PDF viewer ({e}). The file is at {}.", path.display()))
}

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
