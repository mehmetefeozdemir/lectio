//! Everything the UI can ask for. Views never change state themselves: they
//! push an `Action`, and `LectioApp::apply` performs it in one place
//! (ADR-007). This keeps drawing code free of side effects.

use lectio_core::clock::RecorderCommand;
use lectio_core::dsp::FilterSettings;
use lectio_core::prefs::SpeechModel;

use crate::platform::CaptureSource;
use lectio_core::material::SlideLink;
use lectio_core::{CourseId, DeckId, LabelKind, NoteId, SessionId};

/// Top-level screens (ADR-015).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Home,
    Collections,
    Tasks,
    Search,
    Trash,
    /// One session: slides, feed, audio stream, input bar.
    Workspace,
}

/// The hideable workspace panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Panel {
    Slides,
    #[default]
    Feed,
    Audio,
}

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

    // ---- UI redesign (ADR-014): layout and safety, no new features ----
    RenameSession(SessionId, String),
    /// Restores the library from before the last delete.
    Undo,
    DismissUndo,
    SetSidebar(bool),
    SetTheme(eframe::egui::ThemePreference),

    // ---- Session workflow (ADR-015) ----
    GoTo(Screen),
    TrashSession(SessionId),
    RestoreSession(SessionId),
    /// Adds the next slide (`true`) or the shown slide (`false`) to the feed.
    PostSlide { next: bool },
    SetPanel(Panel, bool),
    /// On narrow screens one panel is shown at a time, as tabs.
    ShowTab(Panel),
    EditNoteText(NoteId, String),
    CopyAll,
    ExportMarkdown,
    SlideZoom(f32),
    SetFullscreen(bool),

    // ---- Phase 3: text and audio (ADR-016 to ADR-019) ----
    /// Right panel: transcript (`true`) or audio stream.
    ShowTranscript(bool),
    SetSpeechModel(SpeechModel),
    /// `None` = detect the language.
    SetLanguage(Option<String>),
    DownloadModel,
    Transcribe,
    CancelTranscription,
    ClearTranscript,
    /// Copies transcript segment `index` into the feed as a note.
    SegmentToFeed(usize),
    SetCaptions(bool),
    SearchFor(String),
    /// Opens a search result: the session, the moment and the slide.
    OpenHit { session: SessionId, offset_ms: Option<u64>, slide: Option<SlideLink> },
    SetSpeed(u16),
    SetFilters(FilterSettings),
    SaveCleanedCopy,
    SetPlayCleaned(bool),
    SetSource(CaptureSource),
    TakeScreenshot,
    SaveScreenshot { x: f32, y: f32, w: f32, h: f32 },
    CloseScreenshot,
}
