//! Lectio's mobile surface (ADR-029). UniFFI turns these objects, records
//! and functions into Swift (and, in 5b, Kotlin). Everything here is a thin
//! layer over the shared crates: the rules stay in `lectio-core`, exports in
//! `lectio-export`, speech in `lectio-speech`, the study model in
//! `lectio-llm`. Ids cross the boundary as text, times as milliseconds.
//!
//! - `library`: the user's notes (open a data folder, courses, sessions,
//!   notes, collections, search), saved in the desktop's exact format
//! - `material`: recordings, slides, ink and photos
//! - `study`: quizzes, flashcards, outlines; `export`: documents
//! - `audio`: the recorder and player (Swift moves the samples); `draft`:
//!   the note being typed, stamped when typing started
//! - `engines`: Whisper and the study model; `models`: their files

#![cfg(not(target_arch = "wasm32"))]

mod audio;
mod draft;
mod engines;
mod export;
mod library;
mod material;
mod models;
mod study;

use lectio_core::LabelKind;
use lectio_core::ink::{InkColor as CoreInkColor, InkTool as CoreInkTool};

pub use audio::{AudioPlayer, AudioRecorder};
pub use draft::NoteDraft;
pub use engines::{ProgressListener, SpeechEngine, StudyEngine};
pub use library::LectioLibrary;
pub use models::{FileInfo, ModelInfo, speech_models, study_models, verify_download};
pub use study::{CardInfo, Grade, QuizQuestionInfo, grade_answer};

uniffi::setup_scaffolding!();

/// Every failure, with a message written for the user.
#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum LectioError {
    Failed(String),
}

impl std::fmt::Display for LectioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for LectioError {}

impl From<lectio_core::ModelError> for LectioError {
    fn from(e: lectio_core::ModelError) -> Self {
        Self::Failed(e.to_string())
    }
}

impl From<lectio_core::storage::StoreError> for LectioError {
    fn from(e: lectio_core::storage::StoreError) -> Self {
        Self::Failed(e.to_string())
    }
}

impl From<String> for LectioError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

pub type Result<T> = std::result::Result<T, LectioError>;

/// Reads an id passed as text.
pub(crate) fn id<T: std::str::FromStr<Err = lectio_core::ModelError>>(text: &str) -> Result<T> {
    Ok(text.parse::<T>()?)
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The five labels (named `NoteLabel` so it can't hide SwiftUI's `Label`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NoteLabel {
    Important,
    Review,
    Task,
    Definition,
    Heading,
}

impl From<LabelKind> for NoteLabel {
    fn from(kind: LabelKind) -> Self {
        match kind {
            LabelKind::Important => Self::Important,
            LabelKind::Review => Self::Review,
            LabelKind::Task => Self::Task,
            LabelKind::Definition => Self::Definition,
            LabelKind::Heading => Self::Heading,
        }
    }
}

impl From<NoteLabel> for LabelKind {
    fn from(label: NoteLabel) -> Self {
        match label {
            NoteLabel::Important => Self::Important,
            NoteLabel::Review => Self::Review,
            NoteLabel::Task => Self::Task,
            NoteLabel::Definition => Self::Definition,
            NoteLabel::Heading => Self::Heading,
        }
    }
}

pub(crate) fn kinds(labels: &[NoteLabel]) -> Vec<LabelKind> {
    labels.iter().copied().map(LabelKind::from).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NoteKind {
    Text,
    /// A slide posted to the feed.
    Slide,
    /// A LaTeX formula (the body is its source).
    Formula,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum InkTool {
    Pen,
    Highlighter,
}

impl From<InkTool> for CoreInkTool {
    fn from(tool: InkTool) -> Self {
        match tool {
            InkTool::Pen => Self::Pen,
            InkTool::Highlighter => Self::Highlighter,
        }
    }
}

impl From<CoreInkTool> for InkTool {
    fn from(tool: CoreInkTool) -> Self {
        match tool {
            CoreInkTool::Pen => Self::Pen,
            CoreInkTool::Highlighter => Self::Highlighter,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum InkColor {
    Black,
    Red,
    Blue,
    Green,
    Yellow,
}

impl From<InkColor> for CoreInkColor {
    fn from(color: InkColor) -> Self {
        match color {
            InkColor::Black => Self::Black,
            InkColor::Red => Self::Red,
            InkColor::Blue => Self::Blue,
            InkColor::Green => Self::Green,
            InkColor::Yellow => Self::Yellow,
        }
    }
}

impl From<CoreInkColor> for InkColor {
    fn from(color: CoreInkColor) -> Self {
        match color {
            CoreInkColor::Black => Self::Black,
            CoreInkColor::Red => Self::Red,
            CoreInkColor::Blue => Self::Blue,
            CoreInkColor::Green => Self::Green,
            CoreInkColor::Yellow => Self::Yellow,
        }
    }
}

/// sRGB of an ink colour (the same as on the desktop and in exports).
#[uniffi::export]
pub fn ink_rgb(color: InkColor) -> Vec<u8> {
    CoreInkColor::from(color).rgb().to_vec()
}

/// A point on a slide page: 0..1 across and down.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct InkStrokeInfo {
    pub tool: InkTool,
    pub color: InkColor,
    /// Line width as a fraction of the page width, as drawn (the
    /// highlighter is already widened).
    pub drawn_width: f32,
    /// 0..1: pens are opaque, highlighters see-through.
    pub opacity: f32,
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum HomeFilter {
    All,
    Course { id: String },
    Collection { id: String },
    Label { label: NoteLabel },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SessionSort {
    Newest,
    Oldest,
    Title,
    Longest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ExportFormat {
    Markdown,
    Pdf,
    Docx,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct CourseInfo {
    pub id: String,
    pub name: String,
    pub session_count: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct CollectionInfo {
    pub id: String,
    pub name: String,
    pub session_ids: Vec<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SessionInfo {
    pub id: String,
    pub course_id: String,
    pub course_name: String,
    pub title: String,
    pub created_unix_ms: u64,
    /// `None` until something is recorded.
    pub duration_ms: Option<u64>,
    pub note_count: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SlideRef {
    pub deck_id: String,
    pub page: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct PhotoInfo {
    /// The stored image file.
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub is_sketch: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct NoteInfo {
    pub id: String,
    pub kind: NoteKind,
    pub offset_ms: u64,
    pub body: String,
    pub labels: Vec<NoteLabel>,
    pub slide: Option<SlideRef>,
    pub photo: Option<PhotoInfo>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DeckInfo {
    pub id: String,
    pub title: String,
    pub page_count: u32,
    /// The stored PDF, for PDFKit.
    pub pdf_path: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SegmentInfo {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct OutlineItemInfo {
    pub title: String,
    pub level: u8,
    /// Where the section starts, if known.
    pub offset_ms: Option<u64>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct QuizScore {
    pub correct: u32,
    pub asked: u32,
    pub unix_ms: u64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SessionDetail {
    pub info: SessionInfo,
    pub notes: Vec<NoteInfo>,
    pub decks: Vec<DeckInfo>,
    pub transcript: Vec<SegmentInfo>,
    pub outline: Vec<OutlineItemInfo>,
    pub quiz_scores: Vec<QuizScore>,
    /// The recording, once there is one.
    pub recording_path: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SearchHitInfo {
    pub session_id: String,
    pub session_title: String,
    pub snippet: String,
    pub offset_ms: Option<u64>,
}

/// `None` if the formula is fine, else what is wrong (for the live preview).
#[uniffi::export]
pub fn check_formula(latex: String) -> Option<String> {
    lectio_core::math::parse(&latex).err().map(|e| e.message)
}

/// A formula as PNG for the preview and the feed: `size_px` is pixels per em
/// (multiply by the screen scale); light ink for dark mode.
#[uniffi::export]
pub fn render_formula_png(latex: String, size_px: f32, dark: bool) -> Result<Vec<u8>> {
    let color = if dark { [0xF2, 0xF2, 0xF7] } else { [0x1C, 0x1C, 0x22] };
    let image = lectio_export::formula::render(&latex, size_px, color)?;
    let picture = lectio_export::raster::Rgba { width: image.width, height: image.height, pixels: image.rgba };
    Ok(picture.to_png()?)
}
