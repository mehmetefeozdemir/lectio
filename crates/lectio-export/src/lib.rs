//! Lectio's document writers (ADR-025): the paper view's blocks
//! (`lectio_core::paper`) as an HTML page (the browser prints it), a Word
//! document (.docx) and a PDF. Markdown is written by `lectio_core::paper`.
//!
//! The pictures in a document (slide pages with their ink, photos, sketches,
//! formulas) are made by `pictures` from the stored files and passed to the
//! writers as PNG bytes. The crate has no UI or platform code, so the desktop
//! app, the web app and the mobile apps (ADR-029) all share it, and it is
//! tested on its own.

mod docx;
pub mod fonts;
pub mod formula;
mod html;
pub mod images;
pub mod ink;
mod pdf;
pub mod pictures;
pub mod raster;
pub mod slide_text;
pub mod slides;

use std::collections::HashMap;

use lectio_core::paper::{AssetKey, Block};

pub use docx::docx;
pub use html::html;
pub use pdf::{Fonts, pdf};

/// A picture as PNG, with the size it should be shown at in CSS pixels
/// (the PNG itself may have more pixels, for sharp printing).
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub type Pictures = HashMap<AssetKey, Picture>;

/// The picture for a block, and its alternative text (read by screen
/// readers, and shown where the picture is missing).
pub(crate) fn picture_of<'a>(block: &Block, pictures: &'a Pictures) -> Option<(&'a Picture, String)> {
    let (key, alt) = match block {
        Block::Slide { deck, page, deck_title, caption, .. } => {
            (AssetKey::Slide(*deck, *page), format!("{caption} ({deck_title}, page {page})"))
        }
        Block::Formula { latex, .. } => (AssetKey::Formula(latex.clone()), latex.clone()),
        Block::Image { blob, kind, caption, .. } => {
            let word = lectio_core::paper::photo_word(*kind).trim_matches(['[', ']']);
            let alt = if caption.is_empty() { word.to_owned() } else { caption.clone() };
            (AssetKey::Image(blob.clone()), alt)
        }
        _ => return None,
    };
    pictures.get(&key).map(|p| (p, alt))
}

/// Escapes text for HTML and XML (both attribute values and content).
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            // XML 1.0 forbids most control characters; drop them.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding (for `data:` URLs in the HTML page).
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let byte = |i: usize| u32::from(chunk.get(i).copied().unwrap_or(0));
        let n = (byte(0) << 16) | (byte(1) << 8) | byte(2);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}
