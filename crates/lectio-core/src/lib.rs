//! Lectio core: the domain rules of the app, with no UI and no audio I/O.
//!
//! Everything here is plain data plus pure functions, so it can be tested with
//! `cargo test` and reused unchanged by a future web (WASM) or mobile shell.
//! See `docs/DECISIONS.md` ADR-003 and ADR-006.

pub mod cards;
pub mod clock;
pub mod collections;
pub mod dictionary;
pub mod dsp;
mod error;
mod ids;
mod labels;
pub mod export;
pub mod focus;
pub mod ink;
pub mod math;
pub mod material;
pub mod outline;
pub mod paper;
mod model;
pub mod prefs;
pub mod prompts;
pub mod quiz;
pub mod realtime;
pub mod resample;
pub mod search;
pub mod sources;
pub mod storage;
pub mod stretch;
pub mod stream;
pub mod tasks;
pub mod timeline;
pub mod trash;
pub mod transcript;

pub use error::ModelError;
pub use ids::{CardId, CollectionId, CourseId, DeckId, NoteId, SessionId};
pub use labels::{Label, LabelKind, LabelSet};
pub use model::{Course, Library, Note, NoteKind, Recording, SCHEMA_VERSION, Session};

/// Longest course, session or label name we accept (in characters).
pub const MAX_NAME_CHARS: usize = 80;
