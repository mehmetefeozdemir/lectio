//! The text on each PDF page, for search (ADR-018), read with the same PDF
//! library that draws the slides (`hayro`).
//!
//! hayro "interprets" a page by calling a *device* for everything the page
//! draws. The renderer's device paints pixels; this one ignores shapes and
//! images and keeps only the Unicode text of each run of glyphs (the PDF's
//! `ToUnicode` tables say which character each glyph stands for). Runs are
//! joined with spaces, which is enough for word search, though not for
//! reproducing the page's layout.

use hayro::hayro_interpret::font::GlyphRun;
use hayro::hayro_interpret::hayro_cmap::BfString;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps, InterpreterCache,
    InterpreterSettings, SoftMask, interpret_page,
};
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::{Affine, BezPath, Rect};

#[derive(Default)]
struct TextDevice {
    text: String,
}

impl<'a> Device<'a> for TextDevice {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, _: DrawProps<'a>, _: &DrawMode) {
        if !self.text.is_empty() && !self.text.ends_with(' ') {
            self.text.push(' ');
        }
        for glyph in run.glyphs() {
            match glyph.as_unicode() {
                Some(BfString::Char(c)) => self.text.push(c),
                Some(BfString::String(s)) => self.text.push_str(&s),
                None => {}
            }
        }
    }
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

/// The words on page `page` (1-based), with runs of spaces collapsed.
pub fn page_text(pdf: &Pdf, page: u32) -> Result<String, String> {
    let pages = pdf.pages();
    let index = page.checked_sub(1).ok_or("pages start at 1")? as usize;
    let pdf_page = pages.get(index).ok_or_else(|| format!("This deck has no slide {page}."))?;
    let cache = InterpreterCache::new();
    let (width, height) = pdf_page.render_dimensions();
    let bounds = Rect::new(0.0, 0.0, f64::from(width), f64::from(height));
    let mut context = Context::new(Affine::IDENTITY, bounds, &cache, pdf_page.xref(), InterpreterSettings::default());
    let mut device = TextDevice::default();
    interpret_page(pdf_page, &mut context, &mut device);
    Ok(device.text.split_whitespace().collect::<Vec<_>>().join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../../../assets/sample-slides.pdf");

    #[test]
    fn the_words_on_each_slide_are_read() {
        let pdf = Pdf::new(FIXTURE.to_vec()).unwrap();
        let first = page_text(&pdf, 1).unwrap();
        let second = page_text(&pdf, 2).unwrap();
        assert!(first.contains("Graphs"), "page 1 text: {first:?}");
        assert!(second.contains("BFS") && second.contains("Queue"), "page 2 text: {second:?}");
        assert!(page_text(&pdf, 3).is_err());
    }
}
