//! The recording clock.
//!
//! The current recording offset is *derived from the number of audio samples
//! actually recorded*, not from a wall-clock timer. Pauses add no samples, so
//! they add no time, and a note's offset always matches the audio file
//! exactly (see ADR-005).

use std::fmt;

use crate::timeline::samples_to_ms;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecorderPhase {
    Idle,
    Recording,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecorderCommand {
    Start,
    Pause,
    Resume,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClockError {
    InvalidTransition { from: RecorderPhase, command: RecorderCommand },
}

impl fmt::Display for ClockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition { from, command } => {
                write!(f, "Can't {command:?} while the recorder is {from:?}.")
            }
        }
    }
}

impl std::error::Error for ClockError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingClock {
    phase: RecorderPhase,
    sample_rate: u32,
    samples: u64,
}

impl RecordingClock {
    pub fn new(sample_rate: u32) -> Self {
        Self { phase: RecorderPhase::Idle, sample_rate, samples: 0 }
    }

    pub fn phase(&self) -> RecorderPhase {
        self.phase
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The state machine: Idle → Recording ⇄ Paused → Stopped.
    pub fn apply(&self, command: RecorderCommand) -> Result<RecordingClock, ClockError> {
        use RecorderCommand as C;
        use RecorderPhase as P;
        let next = match (self.phase, command) {
            (P::Idle, C::Start) => P::Recording,
            (P::Recording, C::Pause) => P::Paused,
            (P::Paused, C::Resume) => P::Recording,
            (P::Recording | P::Paused, C::Stop) => P::Stopped,
            (from, command) => return Err(ClockError::InvalidTransition { from, command }),
        };
        Ok(RecordingClock { phase: next, ..*self })
    }

    /// Updates the count of samples written so far. The count only grows: a
    /// stale reading from the audio thread can't move the clock backwards.
    pub fn with_total_samples(&self, total: u64) -> RecordingClock {
        RecordingClock { samples: self.samples.max(total), ..*self }
    }

    pub fn offset_ms(&self) -> u64 {
        samples_to_ms(self.samples, self.sample_rate)
    }
}

/// The note the student is typing. Its anchor is the moment they *started*
/// typing, because that is when they heard the thing worth writing down.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteDraft {
    text: String,
    started_at_ms: Option<u64>,
}

impl NoteDraft {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn with_text(&self, text: &str, now_ms: u64) -> NoteDraft {
        let started_at_ms = match (text.trim().is_empty(), self.started_at_ms) {
            (true, _) => None,
            (false, Some(start)) => Some(start),
            (false, None) => Some(now_ms),
        };
        NoteDraft { text: text.to_owned(), started_at_ms }
    }

    /// The offset to save the note at; `now_ms` if typing hasn't started.
    pub fn anchor_ms(&self, now_ms: u64) -> u64 {
        self.started_at_ms.unwrap_or(now_ms)
    }
}
