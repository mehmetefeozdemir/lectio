//! The data model: Library → Course → Session → Note.
//!
//! Changes never edit a library in place. Each `with_*` method returns a new
//! `Library` and leaves the old one untouched, so the UI can't end up showing
//! a half-applied change, and undo can later be "keep the previous value".

use serde::{Deserialize, Serialize};

use crate::labels::validated_name;
use crate::material::{Attachment, SlideDeck, SlideLink};
use crate::prefs::Preferences;
use crate::{CourseId, LabelKind, LabelSet, ModelError, NoteId, SessionId};

/// Version of the `library.json` format. Bump it when the format changes.
/// 1 = Phase 1. 2 = Phase 2 (slide decks, slide links, photo/sketch notes).
/// 3 = session workflow (Heading label, slide items, trash, preferences).
/// Older files still load because every new field has a default.
pub const SCHEMA_VERSION: u32 = 3;

/// What a feed item is. Older files have no `kind`: they are text notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    #[default]
    Text,
    /// A slide posted to the feed; `slide` says which page, `body` is its title.
    Slide,
}

fn is_text(kind: &NoteKind) -> bool {
    *kind == NoteKind::Text
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    #[serde(default, skip_serializing_if = "is_text")]
    pub kind: NoteKind,
    /// Position in the session's recording, in milliseconds from its start.
    pub offset_ms: u64,
    pub body: String,
    pub labels: Vec<LabelKind>,
    /// The slide page this note is about, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slide: Option<SlideLink>,
    /// A photo or sketch. Only its key is stored here; the image itself is
    /// a separate blob (ADR-012).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment: Option<Attachment>,
}

impl Note {
    /// Every note must carry something: text, a label, an image or a slide.
    pub fn has_content(&self) -> bool {
        !self.body.is_empty() || !self.labels.is_empty() || self.attachment.is_some() || self.kind == NoteKind::Slide
    }

    /// Shown as a section title in the feed and the reading view.
    pub fn is_heading(&self) -> bool {
        self.labels.contains(&LabelKind::Heading)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recording {
    /// File name inside the store's `recordings/` folder.
    pub file_name: String,
    pub duration_ms: u64,
    pub sample_rate: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub title: String,
    pub created_unix_ms: u64,
    pub recording: Option<Recording>,
    /// Always sorted by `offset_ms` (ties keep the order they were added in).
    pub notes: Vec<Note>,
    /// Imported PDF slide decks (Phase 2). Missing in version 1 files.
    #[serde(default)]
    pub decks: Vec<SlideDeck>,
    /// Set when the session is moved to the trash (restorable for 30 days).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_unix_ms: Option<u64>,
}

impl Session {
    pub fn note(&self, id: NoteId) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    pub fn note_offsets(&self) -> Vec<u64> {
        self.notes.iter().map(|n| n.offset_ms).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Course {
    pub id: CourseId,
    pub name: String,
    pub sessions: Vec<Session>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    schema_version: u32,
    courses: Vec<Course>,
    labels: LabelSet,
    /// The user's layout and theme choices (saved with the library).
    #[serde(default)]
    preferences: Preferences,
}

impl Default for Library {
    fn default() -> Self {
        Self { schema_version: SCHEMA_VERSION, courses: Vec::new(), labels: LabelSet::default(), preferences: Preferences::default() }
    }
}

/// Only the version field, read first so a newer file is refused before we
/// try (and fail) to understand the rest of it.
#[derive(Deserialize)]
struct VersionProbe {
    schema_version: u32,
}

impl Library {
    // ---------- reading ----------

    pub fn courses(&self) -> &[Course] {
        &self.courses
    }

    pub fn labels(&self) -> &LabelSet {
        &self.labels
    }

    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }

    pub fn with_preferences(&self, preferences: Preferences) -> Library {
        Library { preferences, ..self.clone() }
    }

    /// A copy with these courses (used by the trash to drop sessions).
    pub(crate) fn with_courses(&self, courses: Vec<Course>) -> Library {
        Library { courses, ..self.clone() }
    }

    pub fn course(&self, id: CourseId) -> Option<&Course> {
        self.courses.iter().find(|c| c.id == id)
    }

    pub fn session(&self, id: SessionId) -> Option<&Session> {
        self.courses.iter().flat_map(|c| c.sessions.iter()).find(|s| s.id == id)
    }

    pub fn course_of(&self, session: SessionId) -> Option<&Course> {
        self.courses.iter().find(|c| c.sessions.iter().any(|s| s.id == session))
    }

    // ---------- changes (each returns a new Library) ----------

    pub fn with_course(&self, name: &str) -> Result<(Library, CourseId), ModelError> {
        let course =
            Course { id: CourseId::new_random(), name: validated_name(name)?, sessions: Vec::new() };
        let id = course.id;
        let mut next = self.clone();
        next.courses.push(course);
        Ok((next, id))
    }

    pub fn with_session(
        &self,
        course: CourseId,
        title: &str,
        created_unix_ms: u64,
    ) -> Result<(Library, SessionId), ModelError> {
        let session = Session {
            id: SessionId::new_random(),
            title: validated_name(title)?,
            created_unix_ms,
            recording: None,
            notes: Vec::new(),
            decks: Vec::new(),
            deleted_unix_ms: None,
        };
        let id = session.id;
        let next = self.map_course(course, |c| {
            let mut sessions = c.sessions.clone();
            sessions.push(session);
            Ok(Course { sessions, ..c.clone() })
        })?;
        Ok((next, id))
    }

    /// Renames a session (inline title editing); everything else stays.
    pub fn with_session_renamed(&self, session: SessionId, title: &str) -> Result<Library, ModelError> {
        let title = validated_name(title)?;
        self.map_session(session, |s| Ok(Session { title, ..s.clone() }))
    }

    /// Adds a note at `offset_ms`. A note with no text but at least one label
    /// is a one-tap "moment" marker.
    pub fn with_note(
        &self,
        session: SessionId,
        offset_ms: u64,
        body: &str,
        labels: &[LabelKind],
    ) -> Result<(Library, NoteId), ModelError> {
        self.insert_note(session, new_note(offset_ms, body, labels, None))
    }

    /// The one way any note (text, label-only, photo, sketch) enters the
    /// timeline: content and offset are checked, then it is inserted in
    /// `offset_ms` order (ties keep the order they were added in).
    pub(crate) fn insert_note(&self, session: SessionId, note: Note) -> Result<(Library, NoteId), ModelError> {
        if !note.has_content() {
            return Err(ModelError::EmptyNote);
        }
        let (id, offset_ms) = (note.id, note.offset_ms);
        let next = self.map_session(session, |s| {
            if s.recording.as_ref().is_some_and(|rec| offset_ms > rec.duration_ms) {
                return Err(ModelError::OffsetOutOfRange);
            }
            let at = s.notes.partition_point(|n| n.offset_ms <= offset_ms);
            let mut notes = s.notes.clone();
            notes.insert(at, note);
            Ok(Session { notes, ..s.clone() })
        })?;
        Ok((next, id))
    }

    pub fn with_label_toggled(
        &self,
        session: SessionId,
        note: NoteId,
        kind: LabelKind,
    ) -> Result<Library, ModelError> {
        self.map_note(session, note, |n| {
            let labels: Vec<LabelKind> = if n.labels.contains(&kind) {
                n.labels.iter().copied().filter(|&k| k != kind).collect()
            } else {
                n.labels.iter().copied().chain([kind]).collect()
            };
            let changed = Note { labels, ..n.clone() };
            if !changed.has_content() {
                return Err(ModelError::EmptyNote);
            }
            Ok(changed)
        })
    }

    pub fn without_note(&self, session: SessionId, note: NoteId) -> Result<Library, ModelError> {
        self.map_session(session, |s| {
            if s.note(note).is_none() {
                return Err(ModelError::NoteNotFound);
            }
            let notes = s.notes.iter().filter(|n| n.id != note).cloned().collect();
            Ok(Session { notes, ..s.clone() })
        })
    }

    pub fn with_recording(
        &self,
        session: SessionId,
        recording: Recording,
    ) -> Result<Library, ModelError> {
        self.map_session(session, |s| {
            if s.recording.is_some() {
                return Err(ModelError::RecordingAlreadyExists);
            }
            Ok(Session { recording: Some(recording), ..s.clone() })
        })
    }

    pub fn with_label_renamed(&self, kind: LabelKind, name: &str) -> Result<Library, ModelError> {
        Ok(Library { labels: self.labels.renamed(kind, name)?, ..self.clone() })
    }

    // ---------- JSON ----------

    pub fn to_json(&self) -> Result<String, ModelError> {
        serde_json::to_string_pretty(self).map_err(|e| ModelError::Corrupt(e.to_string()))
    }

    pub fn from_json(json: &str) -> Result<Library, ModelError> {
        let probe: VersionProbe =
            serde_json::from_str(json).map_err(|e| ModelError::Corrupt(e.to_string()))?;
        if probe.schema_version > SCHEMA_VERSION {
            return Err(ModelError::UnsupportedSchema {
                found: probe.schema_version,
                supported: SCHEMA_VERSION,
            });
        }
        serde_json::from_str(json).map_err(|e| ModelError::Corrupt(e.to_string()))
    }

    // ---------- helpers: copy the path down to one item, change it, rebuild ----------

    fn map_course(
        &self,
        id: CourseId,
        change: impl FnOnce(&Course) -> Result<Course, ModelError>,
    ) -> Result<Library, ModelError> {
        let index = self.courses.iter().position(|c| c.id == id).ok_or(ModelError::CourseNotFound)?;
        let mut courses = self.courses.clone();
        courses[index] = change(&self.courses[index])?;
        Ok(Library { courses, ..self.clone() })
    }

    pub(crate) fn map_session(
        &self,
        id: SessionId,
        change: impl FnOnce(&Session) -> Result<Session, ModelError>,
    ) -> Result<Library, ModelError> {
        let course = self.course_of(id).ok_or(ModelError::SessionNotFound)?.id;
        self.map_course(course, |c| {
            let index = c.sessions.iter().position(|s| s.id == id).ok_or(ModelError::SessionNotFound)?;
            let mut sessions = c.sessions.clone();
            sessions[index] = change(&c.sessions[index])?;
            Ok(Course { sessions, ..c.clone() })
        })
    }

    pub(crate) fn map_note(
        &self,
        session: SessionId,
        id: NoteId,
        change: impl FnOnce(&Note) -> Result<Note, ModelError>,
    ) -> Result<Library, ModelError> {
        self.map_session(session, |s| {
            let index = s.notes.iter().position(|n| n.id == id).ok_or(ModelError::NoteNotFound)?;
            let mut notes = s.notes.clone();
            notes[index] = change(&s.notes[index])?;
            Ok(Session { notes, ..s.clone() })
        })
    }
}

/// A fresh note with trimmed text and each label at most once.
pub(crate) fn new_note(offset_ms: u64, body: &str, labels: &[LabelKind], attachment: Option<Attachment>) -> Note {
    Note {
        id: NoteId::new_random(),
        kind: NoteKind::Text,
        offset_ms,
        body: body.trim().to_owned(),
        labels: unique_labels(labels),
        slide: None,
        attachment,
    }
}

fn unique_labels(labels: &[LabelKind]) -> Vec<LabelKind> {
    labels.iter().enumerate().filter(|(i, k)| !labels[..*i].contains(k)).map(|(_, &k)| k).collect()
}
