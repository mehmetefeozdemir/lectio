//! The speech service at the platform boundary (ADR-016). The app sends
//! requests and polls for events each frame, like the other platform parts:
//! on the desktop a background thread does the work, in the browser a Web
//! Worker does. Both run the same engine (`lectio_speech`).

use lectio_core::SessionId;
use lectio_core::prefs::SpeechModel;
use lectio_core::transcript::Segment;

#[derive(Debug, Clone)]
pub enum SpeechRequest {
    /// Downloads (if needed) and checks a model, without transcribing.
    Prepare { model: SpeechModel },
    /// Transcribes a session's recording from `start_ms` on.
    Transcribe { session: SessionId, file_name: String, model: SpeechModel, language: Option<String>, start_ms: u64 },
    /// Live captions: the last few seconds of the recording in progress.
    Caption { pcm: Vec<f32>, model: SpeechModel, language: Option<String> },
    /// Stops the running transcription after the current window.
    Cancel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SpeechEvent {
    Downloading { model: SpeechModel, done: u64, total: u64 },
    /// The model is on this device and its checksums are correct.
    Ready { model: SpeechModel },
    Progress { session: SessionId, done_ms: u64, total_ms: u64 },
    Segments { session: SessionId, segments: Vec<Segment>, language: String },
    Finished { session: SessionId, language: String, complete: bool },
    Caption(String),
    Failed { session: Option<SessionId>, message: String },
}

pub trait SpeechService {
    fn send(&self, request: SpeechRequest);
    /// Everything that happened since the last call.
    fn poll(&self) -> Vec<SpeechEvent>;
    /// Whether the model's files are already on this device.
    fn is_downloaded(&self, model: SpeechModel) -> bool;
    /// Why a model can't be used here (e.g. too big for the browser).
    fn unavailable(&self, model: SpeechModel) -> Option<&'static str>;
    /// Live captions while recording are offered.
    fn supports_captions(&self) -> bool;
}
