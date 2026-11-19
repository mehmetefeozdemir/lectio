//! The dictionary at the platform boundary (ADR-022): the offline Open
//! English WordNet (downloaded on first use, then kept as a compact index)
//! and, when the user allows it, an online provider behind
//! `lectio_core::dictionary::DefinitionProvider`.

use lectio_core::dictionary::Sense;
use lectio_speech::models::ModelFile;

/// Open English WordNet 2023 as JSON Lines, pinned (CC BY 4.0).
pub const WORDNET_URL: &str = "https://huggingface.co/datasets/jon-tow/open-english-wordnet-synset-2023/resolve/a8e719bd0bb9c28f5718f98a536b9e412f2d07ae/open_english_wordnet_2023.jsonl";
pub const WORDNET_FILE: ModelFile = ModelFile {
    name: "open_english_wordnet_2023.jsonl",
    bytes: 49_044_105,
    sha256: "72797460e94f4eb4ce6d533a4a76e74a006d187c2c92e3a669a6121a7420abd0",
};
/// The compact index made from it (kept on the device).
pub const INDEX_NAME: &str = "wordnet-index.tsv";

#[derive(Debug, Clone)]
pub enum DictRequest {
    /// Download and index the offline dictionary (if needed).
    Prepare,
    /// Look a word up; `online` also asks the online provider when the
    /// offline dictionary has nothing (or isn't there).
    Lookup { word: String, online: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub enum DictEvent {
    Downloading { done: u64, total: u64 },
    /// Building the index after the download.
    Indexing,
    Ready,
    Found { word: String, senses: Vec<Sense>, source: String },
    Failed { message: String },
}

pub trait DictionaryService {
    fn send(&self, request: DictRequest);
    fn poll(&self) -> Vec<DictEvent>;
    /// The offline dictionary is on this device.
    fn is_ready(&self) -> bool;
}
