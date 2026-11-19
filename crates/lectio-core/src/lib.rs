//! Lectio core: the domain rules of the app, with no UI and no audio I/O.
//!
//! Everything here is plain data plus pure functions, so it can be tested with
//! `cargo test` and reused unchanged by a future web (WASM) or mobile shell.
//! See `docs/DECISIONS.md` ADR-002 and ADR-005.

pub mod clock;
mod error;
mod ids;
mod labels;
pub mod material;
mod model;
pub mod realtime;
pub mod resample;
pub mod storage;
pub mod timeline;

pub use error::ModelError;
pub use ids::{CourseId, DeckId, NoteId, SessionId};
pub use labels::{Label, LabelKind, LabelSet};
pub use model::{Course, Library, Note, Recording, SCHEMA_VERSION, Session};

/// Longest course, session or label name we accept (in characters).
pub const MAX_NAME_CHARS: usize = 80;
