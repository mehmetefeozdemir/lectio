//! Platform-free media code for Phase 2: decoding photos, drawing sketches
//! into PNGs, and rendering PDF slides. The same code runs on every target;
//! only *where* slide rendering runs differs (a thread on native, small steps
//! on the browser's main thread). See `slides.rs` and ADR-011.

pub mod fonts;
#[cfg(not(target_arch = "wasm32"))]
pub mod icon;
pub mod images;
pub mod ink;
pub mod math;
pub mod sketch;
pub mod slide_text;
pub mod slides;
