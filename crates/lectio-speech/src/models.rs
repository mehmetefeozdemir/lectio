//! Which Whisper files to download for each model size, from where, and the
//! SHA-256 checksum each must have (ADR-016).
//!
//! The URLs pin an exact revision of each model repository, so the files can
//! never change under the app; the checksum is checked after every download
//! (and on desktop before every load), so a damaged or swapped file is never
//! used. A SHA-256 checksum is a 64-hex-digit fingerprint of the file's
//! bytes: any change to the file gives a different fingerprint.

use lectio_core::prefs::SpeechModel;
use sha2::{Digest, Sha256};

pub struct ModelFile {
    pub name: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

pub struct ModelInfo {
    pub model: SpeechModel,
    pub revision: &'static str,
    pub files: [ModelFile; 3],
    /// Whether the browser build offers it (memory limits, ADR-016).
    pub in_browser: bool,
    /// What the UI says about speed and accuracy.
    pub summary: &'static str,
}

const TOKENIZER: ModelFile = ModelFile {
    name: "tokenizer.json",
    bytes: 2_480_466,
    sha256: "27fc476bfe7f17299480be2273fc0608e4d5a99aba2ab5dec5374b4482d1a566",
};

const CATALOGUE: [ModelInfo; 3] = [
    ModelInfo {
        model: SpeechModel::Tiny,
        revision: "169d4a4341b33bc18d8881c4b69c2e104e1cc0af",
        files: [
            ModelFile {
                name: "config.json",
                bytes: 1_983,
                sha256: "ffdccec4f3211f4c63310f2b7098f309fe70f3952cedc5e4d11e43f5b2379b98",
            },
            TOKENIZER,
            ModelFile {
                name: "model.safetensors",
                bytes: 151_061_672,
                sha256: "7ebd0e69e78190ffe1438491fa05cc1f5c1aa3a4c4db3bc1723adbb551ea2395",
            },
        ],
        in_browser: true,
        summary: "fastest, good for clear speech",
    },
    ModelInfo {
        model: SpeechModel::Base,
        revision: "e37978b90ca9030d5170a5c07aadb050351a65bb",
        files: [
            ModelFile {
                name: "config.json",
                bytes: 1_983,
                sha256: "a153c53883a6799b6f056b4a8d1a515c9926d03994682ba88a7616618d7da0c1",
            },
            TOKENIZER,
            ModelFile {
                name: "model.safetensors",
                bytes: 290_403_936,
                sha256: "07cadb9f25677c8d50df603e66a98fbd842cce45047139baeb16e6219a1e807b",
            },
        ],
        in_browser: true,
        summary: "about 2× slower, fewer mistakes",
    },
    ModelInfo {
        model: SpeechModel::Small,
        revision: "973afd24965f72e36ca33b3055d56a652f456b4d",
        files: [
            ModelFile {
                name: "config.json",
                bytes: 1_967,
                sha256: "e6a2b489da1b5aed65a8eb8d1e7466fa867ad5643a8bc138ba708bd56b2875c4",
            },
            TOKENIZER,
            ModelFile {
                name: "model.safetensors",
                bytes: 966_995_080,
                sha256: "1d7734884874f1a1513ed9aa760a4f8e97aaa02fd6d93a3a85d27b2ae9ca596b",
            },
        ],
        in_browser: false,
        summary: "about 6× slower than tiny, most accurate",
    },
];

pub fn info(model: SpeechModel) -> &'static ModelInfo {
    CATALOGUE.iter().find(|m| m.model == model).unwrap_or(&CATALOGUE[0])
}

impl ModelInfo {
    pub fn url(&self, file: &ModelFile) -> String {
        format!("https://huggingface.co/openai/whisper-{}/resolve/{}/{}", self.model.name(), self.revision, file.name)
    }

    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }
}

/// Checks a whole file in memory.
pub fn verify(file: &ModelFile, bytes: &[u8]) -> Result<(), String> {
    let mut hasher = Checksum::default();
    hasher.update(bytes);
    hasher.check(file)
}

/// A checksum computed piece by piece while a download streams in.
#[derive(Default)]
pub struct Checksum {
    hasher: Sha256,
    bytes: u64,
}

impl Checksum {
    pub fn update(&mut self, chunk: &[u8]) {
        self.hasher.update(chunk);
        self.bytes += chunk.len() as u64;
    }

    pub fn check(self, file: &ModelFile) -> Result<(), String> {
        let digest = self.hasher.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        if self.bytes != file.bytes || hex != file.sha256 {
            return Err(format!(
                "{} did not download correctly (checksum mismatch). It was deleted; try again.",
                file.name
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_pin_a_revision() {
        let tiny = info(SpeechModel::Tiny);
        assert_eq!(
            tiny.url(&tiny.files[2]),
            "https://huggingface.co/openai/whisper-tiny/resolve/169d4a4341b33bc18d8881c4b69c2e104e1cc0af/model.safetensors"
        );
        assert_eq!(tiny.total_bytes(), 1_983 + 2_480_466 + 151_061_672);
    }

    #[test]
    fn the_checksum_accepts_the_right_bytes_and_refuses_others() {
        let file = ModelFile { name: "x", bytes: 3, sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" };
        assert!(verify(&file, b"abc").is_ok());
        assert!(verify(&file, b"abd").is_err());
        assert!(verify(&file, b"abcd").is_err());
    }

    #[test]
    fn every_model_has_three_files_and_small_stays_off_the_web() {
        for model in SpeechModel::ALL {
            assert_eq!(info(model).files.len(), 3);
            assert!(info(model).files.iter().all(|f| f.sha256.len() == 64));
        }
        assert!(!info(SpeechModel::Small).in_browser);
    }
}
