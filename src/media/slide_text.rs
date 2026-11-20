//! The text on each PDF page, for search (ADR-018). It is read by
//! `lectio_export::slide_text`, shared with the mobile apps (ADR-029). Only
//! the browser build reads text here; the desktop reads it through
//! `lectio_export::slides::all_text` on its render thread.

#[cfg(target_arch = "wasm32")]
pub use lectio_export::slide_text::page_text;
