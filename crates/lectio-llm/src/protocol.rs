//! Messages between the page and a browser worker that runs study work
//! (ADR-021). The same worker program also understands the speech messages
//! (`lectio_speech::protocol`); study messages use their own tag field
//! (`study` / `study_event`), so the two never mix.
//!
//! The page starts a *separate* worker for the study model: each worker has
//! its own WebAssembly memory (at most 4 GB), so Whisper and Qwen never have
//! to share one.
//!
//! Every message is an array: the JSON text first, then any buffers, which
//! are *transferred* (moved without a copy).

use lectio_core::prefs::LocalModel;
use serde::{Deserialize, Serialize};

/// Page → worker. Buffers: `Load`: model.gguf, tokenizer.json (bytes);
/// `BuildIndex`: the dictionary's JSON Lines (bytes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "study", rename_all = "snake_case")]
pub enum ToStudy {
    Load { model: LocalModel },
    Complete { id: u64, system: String, user: String, max_tokens: usize },
    BuildIndex,
}

/// Worker → page. Buffers: `Index`: the index text (UTF-8 bytes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "study_event", rename_all = "snake_case")]
pub enum FromStudy {
    /// The worker's program has loaded and is listening. Messages sent to a
    /// worker before this can be lost, so the page waits for it.
    Started,
    Loaded { model: LocalModel },
    /// The answer, with how long it took (for the speed shown in the console).
    Reply { id: u64, text: String, prompt_tokens: usize, generated_tokens: usize, seconds: f64 },
    Index,
    Failed { id: Option<u64>, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn study_commands_round_trip_and_use_their_own_tag() {
        let command = ToStudy::Complete { id: 7, system: "s".into(), user: "u".into(), max_tokens: 300 };
        let json = serde_json::to_string(&command).unwrap();
        assert!(json.contains("\"study\":\"complete\"") && !json.contains("\"cmd\""));
        assert_eq!(serde_json::from_str::<ToStudy>(&json).unwrap(), command);
    }

    #[test]
    fn speech_commands_are_not_study_commands() {
        assert!(serde_json::from_str::<ToStudy>(r#"{"cmd":"cancel"}"#).is_err());
    }

    #[test]
    fn study_events_round_trip() {
        let event = FromStudy::Loaded { model: LocalModel::Small };
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(serde_json::from_str::<FromStudy>(&json).unwrap(), event);
    }
}
