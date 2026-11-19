//! Messages between the page and the browser's speech worker (ADR-016).
//! Each message is a JSON text; large data (model files, audio) travels
//! next to it as *transferred* buffers, which move to the worker without a
//! copy.

use lectio_core::SessionId;
use lectio_core::prefs::SpeechModel;
use lectio_core::transcript::Segment;
use serde::{Deserialize, Serialize};

/// Page → worker. Buffers that go with each command:
/// `Load`: config.json, tokenizer.json, model.safetensors (bytes);
/// `Transcribe` and `Caption`: the audio (16 kHz `f32` samples).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum ToWorker {
    Load { model: SpeechModel },
    Transcribe { session: SessionId, language: Option<String>, start_ms: u64 },
    Caption { language: Option<String> },
    Cancel,
}

/// Worker → page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FromWorker {
    Loaded { model: SpeechModel },
    Segments { session: SessionId, segments: Vec<Segment>, language: String },
    Progress { session: SessionId, done_ms: u64, total_ms: u64 },
    Finished { session: SessionId, language: String, complete: bool },
    Caption { text: String },
    Error { session: Option<SessionId>, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_round_trip_as_json() {
        let command = ToWorker::Transcribe { session: SessionId::new_random(), language: Some("tr".into()), start_ms: 30_000 };
        let json = serde_json::to_string(&command).unwrap();
        assert!(json.contains("\"cmd\":\"transcribe\""));
        assert_eq!(serde_json::from_str::<ToWorker>(&json).unwrap(), command);
    }

    #[test]
    fn events_round_trip_as_json() {
        let event = FromWorker::Segments {
            session: SessionId::new_random(),
            segments: vec![Segment { start_ms: 0, end_ms: 900, text: "Hi".into() }],
            language: "en".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(serde_json::from_str::<FromWorker>(&json).unwrap(), event);
    }
}
