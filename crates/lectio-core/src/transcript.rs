//! Transcripts (Phase 3, ADR-016): the rules for turning what the speech
//! model says about one 30-second window into segments on the recording's
//! timeline, and for stepping through a long recording window by window.
//!
//! Whisper reads audio in 30-second windows and marks where each phrase
//! starts and ends with *timestamp tokens* (one per 20 ms, counted from the
//! window's start). These functions are pure, so the alignment is unit-tested
//! without running a model.

use serde::{Deserialize, Serialize};

use crate::model::Session;
use crate::{Library, ModelError, SessionId};

/// Whisper's window length.
pub const WINDOW_MS: u64 = 30_000;
/// Distance between two Whisper timestamp tokens.
pub const TIMESTAMP_STEP_MS: u64 = 20;
/// The next window starts at least this far after the previous one, so a
/// window that only yields a tiny segment can't stall the transcription.
const MIN_STEP_MS: u64 = 1_000;

/// A phrase as the model reports it, relative to the start of its window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// A phrase on the recording's timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transcript {
    /// Whisper language code, e.g. "en".
    pub language: String,
    /// Which model made it ("tiny", "base", "small").
    pub model: String,
    pub segments: Vec<Segment>,
    /// False while a transcription is still running (or was stopped part-way).
    #[serde(default)]
    pub complete: bool,
}

/// Milliseconds for the `index`-th timestamp token.
pub fn timestamp_ms(index: u32) -> u64 {
    u64::from(index) * TIMESTAMP_STEP_MS
}

/// Moves one window's segments onto the recording timeline: adds the
/// window's start, clamps to the recording, drops empty and non-speech
/// segments ("[Music]", "(applause)"), and keeps the segments in order
/// without overlaps.
pub fn align_window(window_start_ms: u64, raw: &[RawSegment], duration_ms: u64) -> Vec<Segment> {
    let mut segments: Vec<Segment> = Vec::new();
    for r in raw {
        let text = clean_text(&r.text);
        let start = window_start_ms + r.start_ms;
        if text.is_empty() || start >= duration_ms {
            continue;
        }
        let previous_end = segments.last().map_or(0, |s| s.end_ms);
        let start = start.max(previous_end);
        let end = (window_start_ms + r.end_ms).clamp(start, duration_ms.max(start));
        segments.push(Segment { start_ms: start, end_ms: end, text });
    }
    segments
}

/// Trims, collapses spaces, and returns "" for bracketed non-speech tags.
fn clean_text(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let bracketed = |open: char, close: char| text.starts_with(open) && text.ends_with(close);
    if bracketed('[', ']') || bracketed('(', ')') || bracketed('*', '*') || !text.chars().any(char::is_alphanumeric) {
        return String::new();
    }
    text
}

/// Where the next window starts: right after the last phrase the model
/// finished, or after the whole window if it heard nothing.
/// `window_len_ms` is how much audio the window really had (the last one
/// is usually shorter than 30 s).
pub fn next_window_start(window_start_ms: u64, raw: &[RawSegment], window_len_ms: u64) -> u64 {
    match raw.last() {
        None => window_start_ms + window_len_ms,
        Some(last) => window_start_ms + last.end_ms.clamp(MIN_STEP_MS, window_len_ms.max(MIN_STEP_MS)),
    }
}

/// Appends `new` to `existing`, dropping segments already covered and
/// trimming the start of one that overlaps the last kept segment.
pub fn merged(existing: &[Segment], new: &[Segment]) -> Vec<Segment> {
    let mut out = existing.to_vec();
    for segment in new {
        let last_end = out.last().map_or(0, |s| s.end_ms);
        if segment.end_ms <= last_end {
            continue;
        }
        out.push(Segment { start_ms: segment.start_ms.max(last_end), ..segment.clone() });
    }
    out
}

/// The segment playing at `offset_ms` (the last one started; a gap keeps
/// the previous segment highlighted).
pub fn segment_at(segments: &[Segment], offset_ms: u64) -> Option<usize> {
    segments.partition_point(|s| s.start_ms <= offset_ms).checked_sub(1)
}

impl Library {
    /// Stores (or replaces) a session's transcript. The session must have a
    /// recording, and every segment must lie inside it, in order.
    pub fn with_transcript(&self, session: SessionId, transcript: Transcript) -> Result<Library, ModelError> {
        self.map_session(session, |s| {
            let duration = s.recording.as_ref().ok_or(ModelError::NoRecording)?.duration_ms;
            let in_order = transcript.segments.windows(2).all(|w| w[0].start_ms <= w[1].start_ms);
            let inside = transcript.segments.iter().all(|seg| seg.start_ms <= seg.end_ms && seg.end_ms <= duration);
            if !in_order || !inside {
                return Err(ModelError::OffsetOutOfRange);
            }
            Ok(Session { transcript: Some(transcript), ..s.clone() })
        })
    }

    pub fn without_transcript(&self, session: SessionId) -> Result<Library, ModelError> {
        self.map_session(session, |s| Ok(Session { transcript: None, ..s.clone() }))
    }
}
