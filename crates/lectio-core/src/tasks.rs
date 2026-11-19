//! Tasks across every course (Phase 4a): notes labelled Task, with done and
//! an optional due date.

use std::cmp::Reverse;

use serde::{Deserialize, Serialize};

use crate::model::{Course, Note, Session};
use crate::{CourseId, LabelKind, Library, ModelError, NoteId, SessionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TaskInfo {
    pub done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TaskFilter {
    /// Only this course (`None` = every course).
    pub course: Option<CourseId>,
    pub include_done: bool,
}

/// One task and where it lives.
pub struct TaskRow<'a> {
    pub course: &'a Course,
    pub session: &'a Session,
    pub note: &'a Note,
    pub done: bool,
    pub due_unix_ms: Option<u64>,
}

/// Every task in live sessions: open ones first, then by due date (no date
/// last), then the newest session, then the moment.
pub fn tasks<'a>(library: &'a Library, filter: &TaskFilter) -> Vec<TaskRow<'a>> {
    let mut rows: Vec<TaskRow<'a>> = library
        .live_sessions()
        .into_iter()
        .filter(|(course, _)| filter.course.is_none_or(|c| c == course.id))
        .flat_map(|(course, session)| {
            session.notes.iter().filter(|n| n.labels.contains(&LabelKind::Task)).map(move |note| {
                let info = note.task.unwrap_or_default();
                TaskRow { course, session, note, done: info.done, due_unix_ms: info.due_unix_ms }
            })
        })
        .filter(|row| filter.include_done || !row.done)
        .collect();
    rows.sort_by_key(|r| (r.done, r.due_unix_ms.is_none(), r.due_unix_ms, Reverse(r.session.created_unix_ms), r.note.offset_ms));
    rows
}

impl Library {
    fn change_task(
        &self,
        session: SessionId,
        note: NoteId,
        change: impl FnOnce(TaskInfo) -> TaskInfo,
    ) -> Result<Library, ModelError> {
        self.map_note(session, note, |n| {
            if !n.labels.contains(&LabelKind::Task) {
                return Err(ModelError::NotATask);
            }
            Ok(Note { task: Some(change(n.task.unwrap_or_default())), ..n.clone() })
        })
    }

    pub fn with_task_done(&self, session: SessionId, note: NoteId, done: bool) -> Result<Library, ModelError> {
        self.change_task(session, note, |t| TaskInfo { done, ..t })
    }

    pub fn with_task_due(&self, session: SessionId, note: NoteId, due_unix_ms: Option<u64>) -> Result<Library, ModelError> {
        self.change_task(session, note, |t| TaskInfo { due_unix_ms, ..t })
    }
}
