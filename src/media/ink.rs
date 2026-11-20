//! Slide ink (ADR-024): colours, widths and drawing order live in
//! `lectio_export::ink`, shared with the exports and the mobile apps, so a
//! drawing looks the same everywhere. egui draws them in `ui::ink`.

pub use lectio_export::ink::{HIGHLIGHT_ALPHA, alpha, in_drawing_order, stroke_width};
