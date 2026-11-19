//! Trash: deleting a session moves it here; it can be restored for
//! `TRASH_DAYS` days and is then removed for good (ADR-015).

use crate::{Course, Library, ModelError, Session, SessionId};

pub const TRASH_DAYS: u64 = 30;
const DAY_MS: u64 = 86_400_000;

impl Library {
    fn all_sessions(&self) -> impl Iterator<Item = (&Course, &Session)> {
        self.courses().iter().flat_map(|c| c.sessions.iter().map(move |s| (c, s)))
    }

    /// Sessions not in the trash, newest first, with their course.
    pub fn live_sessions(&self) -> Vec<(&Course, &Session)> {
        let mut all: Vec<_> = self.all_sessions().filter(|(_, s)| s.deleted_unix_ms.is_none()).collect();
        all.sort_by_key(|(_, s)| std::cmp::Reverse(s.created_unix_ms));
        all
    }

    /// Sessions in the trash, most recently deleted first.
    pub fn trashed_sessions(&self) -> Vec<(&Course, &Session)> {
        let mut all: Vec<_> = self.all_sessions().filter(|(_, s)| s.deleted_unix_ms.is_some()).collect();
        all.sort_by_key(|(_, s)| std::cmp::Reverse(s.deleted_unix_ms));
        all
    }

    pub fn with_session_trashed(&self, session: SessionId, now_ms: u64) -> Result<Library, ModelError> {
        self.map_session(session, |s| Ok(Session { deleted_unix_ms: Some(now_ms), ..s.clone() }))
    }

    pub fn with_session_restored(&self, session: SessionId) -> Result<Library, ModelError> {
        self.map_session(session, |s| Ok(Session { deleted_unix_ms: None, ..s.clone() }))
    }

    /// Removes sessions that have been in the trash for `TRASH_DAYS` or more.
    /// Returns the removed sessions so the app can delete their audio and
    /// files (`material::orphaned_blobs` finds the blobs).
    pub fn without_expired_trash(&self, now_ms: u64) -> (Library, Vec<Session>) {
        let expired =
            |s: &Session| s.deleted_unix_ms.is_some_and(|d| now_ms.saturating_sub(d) >= TRASH_DAYS * DAY_MS);
        let removed: Vec<Session> = self.all_sessions().map(|(_, s)| s).filter(|s| expired(s)).cloned().collect();
        if removed.is_empty() {
            return (self.clone(), removed);
        }
        let courses = self
            .courses()
            .iter()
            .map(|c| Course { sessions: c.sessions.iter().filter(|s| !expired(s)).cloned().collect(), ..c.clone() })
            .collect();
        (self.with_courses(courses), removed)
    }
}
