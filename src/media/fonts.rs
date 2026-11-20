//! The bundled font files (see `assets/fonts/README.md`). The ones documents
//! also need live in `lectio_export::fonts` (embedded once, shared with the
//! exports and the mobile apps); the UI-only fonts are here.

pub use lectio_export::fonts::{INTER_REGULAR, INTER_SEMIBOLD};

pub const MONO: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
pub const ICONS: &[u8] = include_bytes!("../../assets/fonts/Phosphor.ttf");
