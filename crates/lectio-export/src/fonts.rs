//! The bundled fonts that documents need (see `assets/fonts/README.md`):
//! Inter for text in PDFs and formulas, Noto Sans Math for formulas. The
//! desktop app's theme uses the same bytes, so they are embedded once.

pub const INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
pub const INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");
/// Formulas (ADR-023).
pub const MATH: &[u8] = include_bytes!("../../../assets/fonts/NotoSansMath-Regular.ttf");
