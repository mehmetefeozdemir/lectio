//! The model files the mobile apps download (ADR-031): the same pinned URLs,
//! sizes and SHA-256 sums as the desktop, so URLSession fetches them and Rust
//! checks them. They go in `models/<dir_name>/<file>` in the data folder,
//! the desktop's layout.

use std::io::Read;

use lectio_core::prefs::{LocalModel, SpeechModel};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, uniffi::Record)]
pub struct FileInfo {
    pub name: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ModelInfo {
    /// The id the engines take ("tiny", "base", "small" for speech; "small",
    /// "large" for the study model).
    pub name: String,
    pub display_name: String,
    /// Folder under `models/`.
    pub dir_name: String,
    pub total_bytes: u64,
    pub files: Vec<FileInfo>,
}

/// Whisper tiny, base and small.
#[uniffi::export]
pub fn speech_models() -> Vec<ModelInfo> {
    SpeechModel::ALL
        .into_iter()
        .map(|model| {
            let info = lectio_speech::models::info(model);
            ModelInfo {
                name: model.name().to_owned(),
                display_name: format!("Whisper {} ({} MB)", model.name(), info.total_bytes() / 1_000_000),
                dir_name: format!("whisper-{}", model.name()),
                total_bytes: info.total_bytes(),
                files: info
                    .files
                    .iter()
                    .map(|f| FileInfo { name: f.name.into(), url: info.url(f), bytes: f.bytes, sha256: f.sha256.into() })
                    .collect(),
            }
        })
        .collect()
}

pub(crate) fn local_model(name: &str) -> Option<LocalModel> {
    match name {
        "small" => Some(LocalModel::Small),
        "large" => Some(LocalModel::Large),
        _ => None,
    }
}

/// The local study models (Qwen2.5 0.5B and 1.5B).
#[uniffi::export]
pub fn study_models() -> Vec<ModelInfo> {
    LocalModel::ALL
        .into_iter()
        .map(|model| {
            let info = lectio_llm::models::info(model);
            ModelInfo {
                name: if model == LocalModel::Small { "small".into() } else { "large".into() },
                display_name: info.name.into(),
                dir_name: info.dir.into(),
                total_bytes: info.total_bytes(),
                files: info
                    .files
                    .iter()
                    .map(|f| FileInfo { name: f.file.name.into(), url: f.url.into(), bytes: f.file.bytes, sha256: f.file.sha256.into() })
                    .collect(),
            }
        })
        .collect()
}

/// Is the file at `path` exactly `bytes` long with this SHA-256? Read in
/// pieces, so a 1 GB model doesn't need 1 GB of memory.
#[uniffi::export]
pub fn verify_download(path: String, bytes: u64, sha256: String) -> bool {
    let Ok(mut file) = std::fs::File::open(&path) else { return false };
    if file.metadata().map(|m| m.len()).ok() != Some(bytes) {
        return false;
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buffer[..n]),
            Err(_) => return false,
        }
    }
    let digest: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    digest.eq_ignore_ascii_case(sha256.trim())
}
