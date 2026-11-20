//! The bundled font files (see `assets/fonts/README.md`), embedded once and
//! shared by the UI theme, the formula renderer and the PDF export.

pub const INTER_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
pub const INTER_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-SemiBold.ttf");
pub const MONO: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
pub const ICONS: &[u8] = include_bytes!("../../assets/fonts/Phosphor.ttf");
/// Formulas (ADR-023).
pub const MATH: &[u8] = include_bytes!("../../assets/fonts/NotoSansMath-Regular.ttf");
