//! A whole recording, window by window (Phase 3, ADR-016). The same stepper
//! runs on the desktop's speech thread and in the browser's worker; the
//! caller decides what to do with each window's segments (show them, save
//! them) and can stop between windows. The worker also pauses between
//! windows so a "stop" message can reach it.

use lectio_core::transcript::{Segment, WINDOW_MS, after_last_window, align_window, next_window_start};

use crate::engine::Engine;
use crate::mel::SAMPLE_RATE;

const SAMPLES_PER_MS: u64 = SAMPLE_RATE as u64 / 1_000;

/// What the caller hears after each window.
pub struct WindowDone {
    /// New segments on the recording's timeline.
    pub segments: Vec<Segment>,
    /// How far the transcription has got (ms of audio).
    pub done_ms: u64,
    pub total_ms: u64,
    pub language: String,
}

/// Where a transcription has got to.
pub struct Transcription {
    start_ms: u64,
    total_ms: u64,
    language: Option<String>,
}

impl Transcription {
    /// From `start_ms` on, over `pcm_len` samples. `language` = `None` detects
    /// it from the first window and keeps it for the rest.
    pub fn new(pcm_len: usize, start_ms: u64, language: Option<&str>) -> Self {
        Self { start_ms, total_ms: pcm_len as u64 / SAMPLES_PER_MS, language: language.map(str::to_owned) }
    }

    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// Transcribes the next window; `None` when the recording is done.
    pub fn next(&mut self, engine: &mut Engine, pcm: &[f32]) -> Result<Option<WindowDone>, String> {
        if self.start_ms >= self.total_ms {
            return Ok(None);
        }
        let from = (self.start_ms * SAMPLES_PER_MS) as usize;
        let to = (from + (WINDOW_MS * SAMPLES_PER_MS) as usize).min(pcm.len());
        let last = to == pcm.len();
        let window_ms = (to - from) as u64 / SAMPLES_PER_MS;
        let result = engine.transcribe_window(&pcm[from..to], self.language.as_deref(), last)?;
        let language = self.language.get_or_insert(result.language).clone();
        let segments = align_window(self.start_ms, &result.segments, self.total_ms);
        let start = self.start_ms;
        self.start_ms = if last {
            after_last_window(start, &result.segments, window_ms, self.total_ms)
        } else {
            next_window_start(start, &result.segments, window_ms)
        };
        Ok(Some(WindowDone { segments, done_ms: self.start_ms.min(self.total_ms), total_ms: self.total_ms, language }))
    }
}

/// Transcribes `pcm` (16 kHz mono) from `start_ms` on. After each window
/// `on_window` is called; returning `false` stops the job. Returns the
/// language used.
pub fn transcribe(
    engine: &mut Engine,
    pcm: &[f32],
    start_ms: u64,
    language: Option<&str>,
    mut on_window: impl FnMut(WindowDone) -> bool,
) -> Result<String, String> {
    let mut job = Transcription::new(pcm.len(), start_ms, language);
    while let Some(window) = job.next(engine, pcm)? {
        if !on_window(window) {
            break;
        }
    }
    Ok(job.language().unwrap_or("en").to_owned())
}

/// Live captions: the text of the last few seconds of a recording.
pub fn caption(engine: &mut Engine, recent_pcm: &[f32], language: Option<&str>) -> Result<String, String> {
    let result = engine.transcribe_window(recent_pcm, language, true)?;
    let text: Vec<String> =
        result.segments.into_iter().map(|s| s.text.trim().to_owned()).filter(|t| !t.is_empty()).collect();
    Ok(text.join(" "))
}
