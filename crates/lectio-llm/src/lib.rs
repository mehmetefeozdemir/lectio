//! Lectio's local study model (ADR-021): Qwen2.5-Instruct in GGUF form (a
//! file format for *quantized* models, whose weights are stored in 4–8 bits
//! instead of 32 to make them smaller and faster), run with candle. Like the
//! speech engine, nothing here is platform code: the desktop runs it on a
//! thread and the browser in its Web Worker.
//!
//! - `models`: the files for each size, pinned revisions, SHA-256
//! - `engine`: load, then generate a reply to a system + user message
//! - `protocol`: the browser worker's study messages

pub mod engine;
pub mod models;
pub mod protocol;

pub use engine::LlmEngine;
