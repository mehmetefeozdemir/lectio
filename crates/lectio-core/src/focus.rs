//! The focus timer (Phase 4a): study for a set number of minutes, with
//! pause, logged per course when finished. The timer is a value: each
//! change returns a new timer, and time is passed in, so it is tested
//! without waiting.

use serde::{Deserialize, Serialize};

use crate::{CourseId, Library, ModelError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusTimer {
    planned_ms: u64,
    /// Time counted before the current run (pauses split runs).
    elapsed_before_ms: u64,
    running_since: Option<u64>,
}

impl FocusTimer {
    pub fn new(planned_ms: u64) -> Self {
        Self { planned_ms, elapsed_before_ms: 0, running_since: None }
    }

    pub fn start(self, now_ms: u64) -> Self {
        Self { elapsed_before_ms: 0, running_since: Some(now_ms), ..self }
    }

    pub fn pause(self, now_ms: u64) -> Self {
        Self { elapsed_before_ms: self.elapsed_ms(now_ms), running_since: None, ..self }
    }

    pub fn resume(self, now_ms: u64) -> Self {
        if self.running_since.is_some() { self } else { Self { running_since: Some(now_ms), ..self } }
    }

    pub fn is_running(&self) -> bool {
        self.running_since.is_some()
    }

    pub fn planned_ms(&self) -> u64 {
        self.planned_ms
    }

    pub fn elapsed_ms(&self, now_ms: u64) -> u64 {
        let current = self.running_since.map_or(0, |since| now_ms.saturating_sub(since));
        (self.elapsed_before_ms + current).min(self.planned_ms)
    }

    pub fn remaining_ms(&self, now_ms: u64) -> u64 {
        self.planned_ms - self.elapsed_ms(now_ms)
    }

    pub fn is_done(&self, now_ms: u64) -> bool {
        self.elapsed_ms(now_ms) >= self.planned_ms
    }
}

/// A finished focus session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusEntry {
    pub course: CourseId,
    pub started_unix_ms: u64,
    pub minutes: u32,
}

impl Library {
    pub fn focus_log(&self) -> &[FocusEntry] {
        &self.focus_log
    }

    pub fn with_focus_entry(&self, entry: FocusEntry) -> Result<Library, ModelError> {
        self.course(entry.course).ok_or(ModelError::CourseNotFound)?;
        if entry.minutes == 0 {
            return Err(ModelError::InvalidStudyItem("A focus session needs at least one minute.".into()));
        }
        let mut log = self.focus_log.clone();
        log.push(entry);
        Ok(self.with_focus_log(log))
    }

    /// Minutes of focus for a course since `since_unix_ms`.
    pub fn focus_minutes(&self, course: CourseId, since_unix_ms: u64) -> u32 {
        self.focus_log.iter().filter(|e| e.course == course && e.started_unix_ms >= since_unix_ms).map(|e| e.minutes).sum()
    }
}
