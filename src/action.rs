//! Everything the UI can ask for. Views never change state themselves: they
//! push an `Action`, and `LectioApp::apply` performs it in one place
//! (ADR-006). This keeps drawing code free of side effects.

use lectio_core::clock::RecorderCommand;
use lectio_core::material::SlideLink;
use lectio_core::{CourseId, DeckId, LabelKind, NoteId, SessionId};

use crate::media::sketch::StrokePoints;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    AddCourse(String),
    AddSession(CourseId),
    SelectSession(SessionId),
    Recorder(RecorderCommand),
    DraftChanged(String),
    SubmitNote,
    LabelMoment(LabelKind),
    ToggleLabel(NoteId, LabelKind),
    DeleteNote(NoteId),
    TogglePlay,
    /// Jump to an absolute offset (ms) and keep the current play state.
    Seek(u64),
    /// Jump to a note's offset and start playing from there.
    PlayFrom(u64),
    SeekBy(i64),
    OpenLabelEditor,
    SaveLabels(Vec<(LabelKind, String)>),
    CloseLabelEditor,
    DismissBanner,

    // ---- Phase 2: material ----
    ImportSlides,
    ShowDeck(DeckId),
    /// Move the slide viewer by this many pages (−1 = previous).
    TurnSlide(i32),
    /// Show the slide a note links to.
    GoToSlide(SlideLink),
    SetLinkNewNotes(bool),
    LinkNote(NoteId, Option<SlideLink>),
    RemoveDeck(DeckId),
    AddPhotoFromFile,
    OpenCamera,
    TakePhoto,
    CloseCamera,
    OpenSketch,
    SaveSketch { strokes: Vec<StrokePoints>, caption: String, offset_ms: u64 },
    CloseSketch,
    SetReading(bool),
}
