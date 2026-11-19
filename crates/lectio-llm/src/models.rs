//! The local study models' files: a pinned revision, size and SHA-256 for
//! each (checked like the speech models, ADR-016 / ADR-021).

use lectio_core::prefs::LocalModel;
use lectio_speech::models::ModelFile;

pub struct LlmFile {
    pub file: ModelFile,
    pub url: &'static str,
}

pub struct LlmInfo {
    pub model: LocalModel,
    /// Shown in the settings.
    pub name: &'static str,
    /// Folder (desktop) or key prefix (browser) for its files.
    pub dir: &'static str,
    pub files: [LlmFile; 2],
    pub in_browser: bool,
}

/// Both sizes share Qwen2.5's tokenizer file (same checksum).
const TOKENIZER: ModelFile = ModelFile {
    name: "tokenizer.json",
    bytes: 7_031_645,
    sha256: "c0382117ea329cdf097041132f6d735924b697924d6f6fc3945713e96ce87539",
};

const CATALOGUE: [LlmInfo; 2] = [
    LlmInfo {
        model: LocalModel::Small,
        name: "Qwen2.5-0.5B-Instruct (469 MB)",
        dir: "qwen2.5-0.5b-instruct",
        files: [
            LlmFile {
                file: ModelFile {
                    name: "model.gguf",
                    bytes: 491_400_032,
                    sha256: "74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db",
                },
                url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/9217f5db79a29953eb74d5343926648285ec7e67/qwen2.5-0.5b-instruct-q4_k_m.gguf",
            },
            LlmFile {
                file: TOKENIZER,
                url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct/resolve/7ae557604adf67be50417f59c2c2f167def9a775/tokenizer.json",
            },
        ],
        in_browser: true,
    },
    LlmInfo {
        model: LocalModel::Large,
        name: "Qwen2.5-1.5B-Instruct (1.1 GB)",
        dir: "qwen2.5-1.5b-instruct",
        files: [
            LlmFile {
                file: ModelFile {
                    name: "model.gguf",
                    bytes: 1_117_320_736,
                    sha256: "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e",
                },
                url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/91cad51170dc346986eccefdc2dd33a9da36ead9/qwen2.5-1.5b-instruct-q4_k_m.gguf",
            },
            LlmFile {
                file: TOKENIZER,
                url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct/resolve/989aa7980e4cf806f80c7fef2b1adb7bc71aa306/tokenizer.json",
            },
        ],
        in_browser: false,
    },
];

pub fn info(model: LocalModel) -> &'static LlmInfo {
    CATALOGUE.iter().find(|m| m.model == model).unwrap_or(&CATALOGUE[0])
}

impl LlmInfo {
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.file.bytes).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_pins_a_revision_and_has_checksums() {
        for model in LocalModel::ALL {
            let info = info(model);
            for f in &info.files {
                assert!(f.url.contains("/resolve/") && !f.url.contains("/resolve/main/"), "{}", f.url);
                assert_eq!(f.file.sha256.len(), 64);
            }
        }
        assert!(!info(LocalModel::Large).in_browser);
    }
}
