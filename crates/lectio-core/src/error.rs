use std::fmt;

/// Every way a change to the library can be refused. The UI shows these
/// messages to the student, so they are written in plain language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    EmptyName,
    NameTooLong { max: usize },
    CourseNotFound,
    SessionNotFound,
    NoteNotFound,
    EmptyNote,
    OffsetOutOfRange,
    RecordingAlreadyExists,
    UnsupportedSchema { found: u32, supported: u32 },
    Corrupt(String),
    DeckNotFound,
    PageOutOfRange { page: u32, page_count: u32 },
    InvalidBlobKey,
    UnsupportedFile,
    FileTooLarge { max_mb: u64 },
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => write!(f, "Please type a name."),
            Self::NameTooLong { max } => write!(f, "Names can be at most {max} characters."),
            Self::CourseNotFound => write!(f, "That course no longer exists."),
            Self::SessionNotFound => write!(f, "That session no longer exists."),
            Self::NoteNotFound => write!(f, "That note no longer exists."),
            Self::EmptyNote => write!(f, "A note needs some text or at least one label."),
            Self::OffsetOutOfRange => write!(f, "That moment is past the end of the recording."),
            Self::RecordingAlreadyExists => write!(f, "This session already has a recording."),
            Self::UnsupportedSchema { found, supported } => write!(
                f,
                "Your data was saved by a newer Lectio (format {found}; this version reads up to {supported}). \
                 Please update Lectio. Your file was not changed."
            ),
            Self::Corrupt(detail) => write!(f, "The saved data could not be read: {detail}"),
            Self::DeckNotFound => write!(f, "Those slides are not in this session."),
            Self::PageOutOfRange { page, page_count } => {
                write!(f, "There is no slide {page}; this deck has {page_count} slides.")
            }
            Self::InvalidBlobKey => write!(f, "A stored file has an invalid name."),
            Self::UnsupportedFile => {
                write!(f, "That file type isn't supported here. Use a PDF for slides and a PNG or JPEG for photos.")
            }
            Self::FileTooLarge { max_mb } => write!(f, "That file is too large (the limit is {max_mb} MB)."),
        }
    }
}

impl std::error::Error for ModelError {}
