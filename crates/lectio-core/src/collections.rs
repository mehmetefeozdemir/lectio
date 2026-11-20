//! Collections: user-defined groups of sessions across courses (Phase 4b,
//! ADR-026), and the Home list's filter and sort.
//!
//! A collection stores session ids only, so a session can be in several
//! collections and keeps living in its course. Trashed sessions stay listed
//! (they can be restored) but are hidden from every view; a session removed
//! for good leaves every collection (`trash::without_expired_trash`).

use std::cmp::Reverse;

use serde::{Deserialize, Serialize};

use crate::labels::validated_name;
use crate::{CollectionId, Course, CourseId, LabelKind, Library, ModelError, Session, SessionId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
    pub id: CollectionId,
    pub name: String,
    /// In the order they were added; each at most once.
    pub sessions: Vec<SessionId>,
}

/// What the Home list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum HomeFilter {
    #[default]
    All,
    Course(CourseId),
    Collection(CollectionId),
    /// Sessions with at least one note carrying this label.
    Label(LabelKind),
}

/// The order of the Home list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionSort {
    #[default]
    Newest,
    Oldest,
    /// A–Z, ignoring case.
    Title,
    /// Longest recording first.
    Longest,
}

impl SessionSort {
    pub const ALL: [SessionSort; 4] = [Self::Newest, Self::Oldest, Self::Title, Self::Longest];

    pub fn name(self) -> &'static str {
        match self {
            Self::Newest => "Newest first",
            Self::Oldest => "Oldest first",
            Self::Title => "Title A–Z",
            Self::Longest => "Longest first",
        }
    }
}

impl Library {
    pub fn collections(&self) -> &[Collection] {
        &self.collections
    }

    pub fn collection(&self, id: CollectionId) -> Option<&Collection> {
        self.collections.iter().find(|c| c.id == id)
    }

    /// The collections a session is in.
    pub fn collections_of(&self, session: SessionId) -> Vec<&Collection> {
        self.collections.iter().filter(|c| c.sessions.contains(&session)).collect()
    }

    /// The collection's sessions that aren't in the trash, in its order.
    pub fn collection_sessions(&self, id: CollectionId) -> Vec<SessionId> {
        let Some(collection) = self.collection(id) else { return Vec::new() };
        let live = |s: &SessionId| self.session(*s).is_some_and(|s| s.deleted_unix_ms.is_none());
        collection.sessions.iter().copied().filter(live).collect()
    }

    pub fn with_collection(&self, name: &str) -> Result<(Library, CollectionId), ModelError> {
        let collection = Collection { id: CollectionId::new_random(), name: validated_name(name)?, sessions: Vec::new() };
        let id = collection.id;
        let mut collections = self.collections.clone();
        collections.push(collection);
        Ok((self.with_collection_list(collections), id))
    }

    pub fn with_collection_renamed(&self, id: CollectionId, name: &str) -> Result<Library, ModelError> {
        let name = validated_name(name)?;
        self.map_collection(id, |c| Collection { name, ..c.clone() })
    }

    /// Deletes the collection; its sessions stay in their courses.
    pub fn without_collection(&self, id: CollectionId) -> Result<Library, ModelError> {
        self.collection(id).ok_or(ModelError::CollectionNotFound)?;
        Ok(self.with_collection_list(self.collections.iter().filter(|c| c.id != id).cloned().collect()))
    }

    /// Adds a session at the end (no change if it is already there).
    pub fn with_session_in_collection(&self, id: CollectionId, session: SessionId) -> Result<Library, ModelError> {
        self.collection(id).ok_or(ModelError::CollectionNotFound)?;
        self.session(session).ok_or(ModelError::SessionNotFound)?;
        self.map_collection(id, |c| {
            let mut sessions = c.sessions.clone();
            if !sessions.contains(&session) {
                sessions.push(session);
            }
            Collection { sessions, ..c.clone() }
        })
    }

    pub fn without_session_in_collection(&self, id: CollectionId, session: SessionId) -> Result<Library, ModelError> {
        self.map_collection(id, |c| Collection { sessions: c.sessions.iter().copied().filter(|s| *s != session).collect(), ..c.clone() })
    }

    /// Drops ids of sessions that no longer exist (after removing them for good).
    pub(crate) fn without_missing_sessions_in_collections(&self) -> Library {
        let exists = |s: &SessionId| self.session(*s).is_some();
        let collections = self
            .collections
            .iter()
            .map(|c| Collection { sessions: c.sessions.iter().copied().filter(exists).collect(), ..c.clone() })
            .collect();
        self.with_collection_list(collections)
    }

    /// The Home list: sessions not in the trash that match `filter`, in `sort` order.
    pub fn home_sessions(&self, filter: &HomeFilter, sort: SessionSort) -> Vec<(&Course, &Session)> {
        let collection = match filter {
            HomeFilter::Collection(id) => match self.collection(*id) {
                Some(c) => Some(c),
                None => return Vec::new(),
            },
            _ => None,
        };
        let keep = |course: &Course, session: &Session| match filter {
            HomeFilter::All => true,
            HomeFilter::Course(id) => course.id == *id,
            HomeFilter::Collection(_) => collection.is_some_and(|c| c.sessions.contains(&session.id)),
            HomeFilter::Label(kind) => session.notes.iter().any(|n| n.labels.contains(kind)),
        };
        // `live_sessions` is newest first; the stable sorts below keep that as the tie-break.
        let mut sessions: Vec<_> = self.live_sessions().into_iter().filter(|(c, s)| keep(c, s)).collect();
        match sort {
            SessionSort::Newest => {}
            SessionSort::Oldest => sessions.sort_by_key(|(_, s)| s.created_unix_ms),
            SessionSort::Title => sessions.sort_by_key(|(_, s)| s.title.to_lowercase()),
            SessionSort::Longest => sessions.sort_by_key(|(_, s)| Reverse(s.recording.as_ref().map_or(0, |r| r.duration_ms))),
        }
        sessions
    }

    fn map_collection(&self, id: CollectionId, change: impl FnOnce(&Collection) -> Collection) -> Result<Library, ModelError> {
        let index = self.collections.iter().position(|c| c.id == id).ok_or(ModelError::CollectionNotFound)?;
        let mut collections = self.collections.clone();
        collections[index] = change(&self.collections[index]);
        Ok(self.with_collection_list(collections))
    }
}
