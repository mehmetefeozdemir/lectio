//! The user's notes on a phone (ADR-029): one data folder laid out exactly
//! like the desktop's (`library.json`, `recordings/`, `blobs/`, `models/`),
//! read and written by the same store code, so a later sync (5c) can copy
//! files between devices unchanged. Every change is saved at once.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use lectio_core::collections::{HomeFilter as CoreFilter, SessionSort as CoreSort};
use lectio_core::storage::{FileBlobStore, JsonFileStore, LibraryStore};
use lectio_core::{CollectionId, CourseId, Library, ModelError, NoteId, NoteKind as CoreKind, Session, SessionId};

use crate::{
    CollectionInfo, CourseInfo, DeckInfo, HomeFilter, LectioError, NoteInfo, NoteKind, NoteLabel, OutlineItemInfo, PhotoInfo,
    QuizScore, Result, SearchHitInfo, SegmentInfo, SessionDetail, SessionInfo, SessionSort, SlideRef, id, kinds, now_ms,
};

/// Search returns at most this many hits.
const SEARCH_LIMIT: usize = 50;

pub(crate) struct Inner {
    pub library: Library,
    pub store: JsonFileStore,
    pub blobs: FileBlobStore,
    pub dir: PathBuf,
}

#[derive(uniffi::Object)]
pub struct LectioLibrary {
    inner: Mutex<Inner>,
}

impl LectioLibrary {
    pub(crate) fn lock(&self) -> Result<MutexGuard<'_, Inner>> {
        self.inner.lock().map_err(|_| LectioError::Failed("Lectio's data is busy after an earlier error; restart the app.".into()))
    }

    /// Applies a change and saves it; the library is unchanged if either fails.
    pub(crate) fn change<T>(&self, f: impl FnOnce(&Library) -> std::result::Result<(Library, T), ModelError>) -> Result<T> {
        let mut inner = self.lock()?;
        let (next, out) = f(&inner.library)?;
        inner.store.save(&next)?;
        inner.library = next;
        Ok(out)
    }

    pub(crate) fn update(&self, f: impl FnOnce(&Library) -> std::result::Result<Library, ModelError>) -> Result<()> {
        self.change(|lib| f(lib).map(|next| (next, ())))
    }

    pub(crate) fn read<T>(&self, f: impl FnOnce(&Inner) -> Result<T>) -> Result<T> {
        f(&*self.lock()?)
    }
}

pub(crate) fn session_info(library: &Library, session: &Session) -> SessionInfo {
    let course = library.course_of(session.id);
    SessionInfo {
        id: session.id.to_string(),
        course_id: course.map(|c| c.id.to_string()).unwrap_or_default(),
        course_name: course.map(|c| c.name.clone()).unwrap_or_default(),
        title: session.title.clone(),
        created_unix_ms: session.created_unix_ms,
        duration_ms: session.recording.as_ref().map(|r| r.duration_ms),
        note_count: session.notes.len() as u32,
    }
}

fn note_info(inner: &Inner, note: &lectio_core::Note) -> NoteInfo {
    let kind = match note.kind {
        CoreKind::Text => NoteKind::Text,
        CoreKind::Slide => NoteKind::Slide,
        CoreKind::Math => NoteKind::Formula,
    };
    NoteInfo {
        id: note.id.to_string(),
        kind,
        offset_ms: note.offset_ms,
        body: note.body.clone(),
        labels: note.labels.iter().copied().map(NoteLabel::from).collect(),
        slide: note.slide.map(|l| SlideRef { deck_id: l.deck.to_string(), page: l.page }),
        photo: note.attachment.as_ref().map(|a| PhotoInfo {
            path: inner.blobs.path(&a.blob).to_string_lossy().into_owned(),
            width: a.width,
            height: a.height,
            is_sketch: a.kind == lectio_core::material::AttachmentKind::Sketch,
        }),
    }
}

fn filter(filter: HomeFilter) -> Result<CoreFilter> {
    Ok(match filter {
        HomeFilter::All => CoreFilter::All,
        HomeFilter::Course { id: course } => CoreFilter::Course(id::<CourseId>(&course)?),
        HomeFilter::Collection { id: collection } => CoreFilter::Collection(id::<CollectionId>(&collection)?),
        HomeFilter::Label { label } => CoreFilter::Label(label.into()),
    })
}

fn sort(sort: SessionSort) -> CoreSort {
    match sort {
        SessionSort::Newest => CoreSort::Newest,
        SessionSort::Oldest => CoreSort::Oldest,
        SessionSort::Title => CoreSort::Title,
        SessionSort::Longest => CoreSort::Longest,
    }
}

#[uniffi::export]
impl LectioLibrary {
    /// Opens (or starts) the data folder at `data_dir`.
    #[uniffi::constructor]
    pub fn open(data_dir: String) -> Result<Arc<Self>> {
        let dir = PathBuf::from(&data_dir);
        std::fs::create_dir_all(dir.join("recordings")).map_err(|e| LectioError::Failed(format!("Lectio can't use its data folder: {e}")))?;
        let store = JsonFileStore::new(&dir);
        let library = store.load()?.library;
        let blobs = FileBlobStore::new(&dir);
        Ok(Arc::new(Self { inner: Mutex::new(Inner { library, store, blobs, dir }) }))
    }

    pub fn data_dir(&self) -> Result<String> {
        self.read(|inner| Ok(inner.dir.to_string_lossy().into_owned()))
    }

    // ---------- courses and sessions ----------

    pub fn courses(&self) -> Result<Vec<CourseInfo>> {
        self.read(|inner| {
            let live = inner.library.live_sessions();
            Ok(inner
                .library
                .courses()
                .iter()
                .map(|c| CourseInfo {
                    id: c.id.to_string(),
                    name: c.name.clone(),
                    session_count: live.iter().filter(|(course, _)| course.id == c.id).count() as u32,
                })
                .collect())
        })
    }

    pub fn add_course(&self, name: String) -> Result<String> {
        self.change(|lib| lib.with_course(&name)).map(|id| id.to_string())
    }

    /// A new session in the course (created now).
    pub fn add_session(&self, course_id: String, title: String) -> Result<String> {
        let course = id::<CourseId>(&course_id)?;
        self.change(|lib| lib.with_session(course, &title, now_ms())).map(|id| id.to_string())
    }

    pub fn rename_session(&self, session_id: String, title: String) -> Result<()> {
        let session = id::<SessionId>(&session_id)?;
        self.update(|lib| lib.with_session_renamed(session, &title))
    }

    /// Moves the session to the trash (restorable for 30 days, as on the desktop).
    pub fn trash_session(&self, session_id: String) -> Result<()> {
        let session = id::<SessionId>(&session_id)?;
        self.update(|lib| lib.with_session_trashed(session, now_ms()))
    }

    /// The Home list: sessions not in the trash, filtered and sorted.
    pub fn sessions(&self, filter: HomeFilter, sort: SessionSort) -> Result<Vec<SessionInfo>> {
        let (filter, sort) = (self::filter(filter)?, self::sort(sort));
        self.read(|inner| Ok(inner.library.home_sessions(&filter, sort).into_iter().map(|(_, s)| session_info(&inner.library, s)).collect()))
    }

    pub fn session(&self, session_id: String) -> Result<SessionDetail> {
        let session = id::<SessionId>(&session_id)?;
        self.read(|inner| {
            let s = inner.library.session(session).ok_or(ModelError::SessionNotFound)?;
            Ok(SessionDetail {
                info: session_info(&inner.library, s),
                notes: s.notes.iter().map(|n| note_info(inner, n)).collect(),
                decks: s
                    .decks
                    .iter()
                    .map(|d| DeckInfo {
                        id: d.id.to_string(),
                        title: d.title.clone(),
                        page_count: d.page_count,
                        pdf_path: inner.blobs.path(&d.blob).to_string_lossy().into_owned(),
                    })
                    .collect(),
                transcript: s
                    .transcript
                    .iter()
                    .flat_map(|t| &t.segments)
                    .map(|seg| SegmentInfo { start_ms: seg.start_ms, end_ms: seg.end_ms, text: seg.text.clone() })
                    .collect(),
                outline: s
                    .outline
                    .iter()
                    .flat_map(|o| &o.items)
                    .map(|i| OutlineItemInfo { title: i.title.clone(), level: i.level, offset_ms: i.start_ms() })
                    .collect(),
                quiz_scores: s
                    .quiz_attempts
                    .iter()
                    .map(|a| QuizScore { correct: a.correct, asked: a.questions, unix_ms: a.unix_ms })
                    .collect(),
                recording_path: s.recording.as_ref().map(|r| inner.store.audio_path(&r.file_name).to_string_lossy().into_owned()),
            })
        })
    }

    // ---------- notes ----------

    pub fn add_note(&self, session_id: String, offset_ms: u64, text: String, labels: Vec<NoteLabel>) -> Result<String> {
        let session = id::<SessionId>(&session_id)?;
        self.change(|lib| lib.with_note(session, offset_ms, &text, &kinds(&labels))).map(|id| id.to_string())
    }

    /// A LaTeX formula note (checked first; the error says what is wrong).
    pub fn add_formula_note(&self, session_id: String, offset_ms: u64, latex: String, labels: Vec<NoteLabel>) -> Result<String> {
        let session = id::<SessionId>(&session_id)?;
        self.change(|lib| lib.with_math_note(session, offset_ms, &latex, &kinds(&labels))).map(|id| id.to_string())
    }

    pub fn toggle_label(&self, session_id: String, note_id: String, label: NoteLabel) -> Result<()> {
        let (session, note) = (id::<SessionId>(&session_id)?, id::<NoteId>(&note_id)?);
        self.update(|lib| lib.with_label_toggled(session, note, label.into()))
    }

    pub fn edit_note_text(&self, session_id: String, note_id: String, text: String) -> Result<()> {
        let (session, note) = (id::<SessionId>(&session_id)?, id::<NoteId>(&note_id)?);
        self.update(|lib| lib.with_note_text(session, note, &text))
    }

    pub fn delete_note(&self, session_id: String, note_id: String) -> Result<()> {
        let (session, note) = (id::<SessionId>(&session_id)?, id::<NoteId>(&note_id)?);
        self.update(|lib| lib.without_note(session, note))
    }

    // ---------- collections ----------

    pub fn collections(&self) -> Result<Vec<CollectionInfo>> {
        self.read(|inner| {
            Ok(inner
                .library
                .collections()
                .iter()
                .map(|c| CollectionInfo {
                    id: c.id.to_string(),
                    name: c.name.clone(),
                    session_ids: inner.library.collection_sessions(c.id).iter().map(ToString::to_string).collect(),
                })
                .collect())
        })
    }

    pub fn add_collection(&self, name: String) -> Result<String> {
        self.change(|lib| lib.with_collection(&name)).map(|id| id.to_string())
    }

    pub fn rename_collection(&self, collection_id: String, name: String) -> Result<()> {
        let collection = id::<CollectionId>(&collection_id)?;
        self.update(|lib| lib.with_collection_renamed(collection, &name))
    }

    pub fn delete_collection(&self, collection_id: String) -> Result<()> {
        let collection = id::<CollectionId>(&collection_id)?;
        self.update(|lib| lib.without_collection(collection))
    }

    pub fn add_to_collection(&self, collection_id: String, session_id: String) -> Result<()> {
        let (collection, session) = (id::<CollectionId>(&collection_id)?, id::<SessionId>(&session_id)?);
        self.update(|lib| lib.with_session_in_collection(collection, session))
    }

    pub fn remove_from_collection(&self, collection_id: String, session_id: String) -> Result<()> {
        let (collection, session) = (id::<CollectionId>(&collection_id)?, id::<SessionId>(&session_id)?);
        self.update(|lib| lib.without_session_in_collection(collection, session))
    }

    // ---------- search ----------

    /// Notes, transcripts and slide text across every session.
    pub fn search(&self, query: String) -> Result<Vec<SearchHitInfo>> {
        self.read(|inner| {
            Ok(lectio_core::search::search(&inner.library, &query, SEARCH_LIMIT)
                .into_iter()
                .map(|hit| SearchHitInfo {
                    session_id: hit.session.to_string(),
                    session_title: inner.library.session(hit.session).map(|s| s.title.clone()).unwrap_or_default(),
                    snippet: hit.snippet,
                    offset_ms: hit.offset_ms,
                })
                .collect())
        })
    }
}
