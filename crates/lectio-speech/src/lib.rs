//! Lectio's speech-to-text (Phase 3, ADR-016): OpenAI's Whisper model run
//! locally with candle, a machine-learning library written in Rust. Because
//! nothing here is platform code, the same crate runs on the desktop's
//! speech thread and inside the browser's Web Worker.
//!
//! - `mel`: the audio features the model reads
//! - `tokenizer`: model tokens → text, special-token ids, languages
//! - `models`: which files to download for tiny/base/small, and checksums
//! - `engine`: one 30-second window → timestamped phrases
//! - `job`: a whole recording, window by window; live captions
//! - `protocol`: messages between the page and the browser worker

pub mod engine;
pub mod job;
pub mod mel;
pub mod models;
pub mod protocol;
pub mod tokenizer;

pub use engine::{Engine, WindowResult};
pub use tokenizer::{LANGUAGES, language_name};
