//! PDF slides with `hayro` (ADR-010): count pages at import, render pages to
//! images for display.
//!
//! Rendering can take a while for big decks, so `DeckRenderer` keeps it off
//! the UI's critical path: on native it runs on its own thread; in the
//! browser (no threads in our build) it renders at most one page per frame.

use eframe::egui::ColorImage;
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};

/// Slides are rendered this wide (pixels): sharp in a side panel, and well
/// under GPU texture limits.
const TARGET_WIDTH_PX: f32 = 1400.0;
const MAX_SIDE_PX: f32 = 2048.0;

/// A rendered page (1-based) or why it failed.
pub type RenderedPage = (u32, Result<ColorImage, String>);

fn open(bytes: Vec<u8>) -> Result<Pdf, String> {
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

/// `RenderCache<'a>` borrows from the parsed PDF (`'a`), so the cache can't
/// outlive it or be stored next to it in one struct (P-013).
fn render_page<'a>(pdf: &'a Pdf, cache: &RenderCache<'a>, page: u32) -> Result<ColorImage, String> {
    let pages = pdf.pages();
    let index = page.checked_sub(1).ok_or("pages start at 1")? as usize;
    let pdf_page = pages.get(index).ok_or_else(|| format!("This deck has no slide {page}."))?;
    let (width, height) = pdf_page.render_dimensions();
    let scale = (TARGET_WIDTH_PX / width.max(1.0)).min(MAX_SIDE_PX / height.max(1.0));
    let settings = PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE };
    let pixmap = render(pdf_page, cache, &InterpreterSettings::default(), &RenderSettings::default(), &settings);
    let size = [pixmap.width() as usize, pixmap.height() as usize];
    Ok(ColorImage::from_rgba_premultiplied(size, pixmap.data_as_u8_slice()))
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::DeckRenderer;
#[cfg(target_arch = "wasm32")]
pub use web::DeckRenderer;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::HashSet;
    use std::sync::mpsc::{self, Receiver, Sender};

    use super::{RenderCache, RenderedPage, open, render_page};

    /// Renders pages on a background thread. The thread owns the parsed PDF
    /// and stops by itself when this value is dropped (its channel closes).
    pub struct DeckRenderer {
        requests: Sender<u32>,
        results: Receiver<RenderedPage>,
        asked: HashSet<u32>,
    }

    impl DeckRenderer {
        pub fn start(bytes: Vec<u8>) -> Self {
            let (requests, inbox) = mpsc::channel::<u32>();
            let (outbox, results) = mpsc::channel::<RenderedPage>();
            std::thread::spawn(move || {
                let pdf = open(bytes);
                let cache = RenderCache::new();
                for page in inbox {
                    let rendered = pdf.as_ref().map_err(Clone::clone).and_then(|pdf| render_page(pdf, &cache, page));
                    if outbox.send((page, rendered)).is_err() {
                        break;
                    }
                }
            });
            Self { requests, results, asked: HashSet::new() }
        }

        /// Asks for a page; repeated asks for the same page are ignored.
        pub fn request(&mut self, page: u32) {
            if self.asked.insert(page) {
                let _ = self.requests.send(page);
            }
        }

        pub fn poll(&mut self) -> Vec<RenderedPage> {
            let done: Vec<RenderedPage> = self.results.try_iter().collect();
            for (page, _) in &done {
                self.asked.remove(page);
            }
            done
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::collections::VecDeque;

    use super::{Pdf, RenderCache, RenderedPage, open, render_page};

    /// Renders on the main thread, one queued page per `poll` (per frame),
    /// so the page stays responsive between pages. The render cache borrows
    /// the PDF, so it can't live in this struct; each page gets a fresh one
    /// (pages are cached as textures by the app anyway; P-013).
    pub struct DeckRenderer {
        pdf: Result<Pdf, String>,
        queue: VecDeque<u32>,
    }

    impl DeckRenderer {
        pub fn start(bytes: Vec<u8>) -> Self {
            Self { pdf: open(bytes), queue: VecDeque::new() }
        }

        pub fn request(&mut self, page: u32) {
            if !self.queue.contains(&page) {
                self.queue.push_back(page);
            }
        }

        pub fn poll(&mut self) -> Vec<RenderedPage> {
            let Some(page) = self.queue.pop_front() else { return Vec::new() };
            let rendered =
                self.pdf.as_ref().map_err(Clone::clone).and_then(|pdf| render_page(pdf, &RenderCache::new(), page));
            vec![(page, rendered)]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../../assets/sample-slides.pdf");

    #[test]
    fn the_fixture_deck_has_two_pages() {
        assert_eq!(page_count(FIXTURE).unwrap(), 2);
    }

    #[test]
    fn a_page_renders_to_a_landscape_image_of_the_target_width() {
        let pdf = open(FIXTURE.to_vec()).unwrap();
        let image = render_page(&pdf, &RenderCache::new(), 1).unwrap();
        assert_eq!(image.size[0], TARGET_WIDTH_PX as usize);
        assert!(image.size[0] > image.size[1], "A4 landscape");
    }

    #[test]
    fn asking_for_a_missing_page_is_an_error() {
        let pdf = open(FIXTURE.to_vec()).unwrap();
        assert!(render_page(&pdf, &RenderCache::new(), 3).is_err());
        assert!(render_page(&pdf, &RenderCache::new(), 0).is_err());
    }

    #[test]
    fn a_file_that_is_not_a_pdf_is_refused() {
        assert!(page_count(b"%PDF-1.4 but nothing else").is_err());
    }

    #[test]
    fn the_background_renderer_delivers_requested_pages() {
        let mut renderer = DeckRenderer::start(FIXTURE.to_vec());
        renderer.request(2);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut got = Vec::new();
        while got.is_empty() && std::time::Instant::now() < deadline {
            got = renderer.poll();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, 2);
        assert!(got[0].1.is_ok());
    }
}
