//! PDF slides with `hayro` (ADR-011): count pages at import and render a
//! page to pixels. Shared by the desktop app (which keeps a parsed deck on a
//! thread) and the mobile apps (which render pages for exports).

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};

use crate::raster::Rgba;

/// Slides are rendered this wide (pixels): sharp in a side panel or a
/// printed page, and well under GPU texture limits.
pub const TARGET_WIDTH_PX: f32 = 1400.0;
pub const MAX_SIDE_PX: f32 = 2048.0;

pub fn open(bytes: Vec<u8>) -> Result<Pdf, String> {
    Pdf::new(bytes).map_err(|_| {
        "This PDF can't be opened. It may be damaged or password-protected (Lectio can't open protected PDFs).".into()
    })
}

/// Number of pages, checked once at import (also proves the PDF is readable).
pub fn page_count(bytes: &[u8]) -> Result<u32, String> {
    let pdf = open(bytes.to_vec())?;
    let count = pdf.pages().len() as u32;
    if count == 0 { Err("This PDF has no pages.".into()) } else { Ok(count) }
}

/// Renders page `page` (1-based) of an opened deck. `RenderCache<'a>`
/// borrows from the parsed PDF, so the caller keeps both (P-014).
pub fn render_page<'a>(pdf: &'a Pdf, cache: &RenderCache<'a>, page: u32) -> Result<Rgba, String> {
    let pages = pdf.pages();
    let index = page.checked_sub(1).ok_or("pages start at 1")? as usize;
    let pdf_page = pages.get(index).ok_or_else(|| format!("This deck has no slide {page}."))?;
    let (width, height) = pdf_page.render_dimensions();
    let scale = (TARGET_WIDTH_PX / width.max(1.0)).min(MAX_SIDE_PX / height.max(1.0));
    let settings = PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE };
    let pixmap = render(pdf_page, cache, &InterpreterSettings::default(), &RenderSettings::default(), &settings);
    Ok(Rgba { width: u32::from(pixmap.width()), height: u32::from(pixmap.height()), pixels: pixmap.data_as_u8_slice().to_vec() })
}

/// The words on every page (index 0 = page 1), for search (ADR-018); a page
/// whose text can't be read is empty.
pub fn all_text(pdf: &Pdf) -> Vec<String> {
    (1..=pdf.pages().len() as u32).map(|page| crate::slide_text::page_text(pdf, page).unwrap_or_default()).collect()
}

/// Opens the deck and renders one page right away.
pub fn render_one(bytes: &[u8], page: u32) -> Result<Rgba, String> {
    let pdf = open(bytes.to_vec())?;
    let cache = RenderCache::new();
    render_page(&pdf, &cache, page)
}
