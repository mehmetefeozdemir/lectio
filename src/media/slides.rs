//! PDF slides with `hayro` (ADR-011): count pages at import, render pages to
//! images for display.
//!
//! Rendering can take a while for big decks, so `DeckRenderer` keeps it off
//! the UI's critical path: on native it runs on its own thread; in the
//! browser (no threads in our build) it renders at most one page per frame.

use eframe::egui::ColorImage;
use hayro::RenderCache;
use hayro::hayro_syntax::Pdf;
// Opening and rendering are shared with the exports and the mobile apps (ADR-029).
pub use lectio_export::slides::page_count;
use lectio_export::slides::open;

/// A rendered page (1-based) or why it failed.
pub type RenderedPage = (u32, Result<ColorImage, String>);

/// Renders a page of an opened deck as an egui image.
fn render_page<'a>(pdf: &'a Pdf, cache: &RenderCache<'a>, page: u32) -> Result<ColorImage, String> {
    let picture = lectio_export::slides::render_page(pdf, cache, page)?;
    Ok(ColorImage::from_rgba_premultiplied([picture.width as usize, picture.height as usize], &picture.pixels))
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::DeckRenderer;
#[cfg(target_arch = "wasm32")]
pub use web::DeckRenderer;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::HashSet;
    use std::sync::mpsc::{self, Receiver, Sender};

    use super::{RenderCache, RenderedPage, all_text, open, render_page};

    enum Job {
        Page(u32),
        Text,
    }

    /// Renders pages (and reads the deck's text) on a background thread. The
    /// thread owns the parsed PDF and stops by itself when this value is
    /// dropped (its channel closes).
    pub struct DeckRenderer {
        requests: Sender<Job>,
        results: Receiver<RenderedPage>,
        texts: Receiver<Vec<String>>,
        asked: HashSet<u32>,
        text_asked: bool,
    }

    impl DeckRenderer {
        pub fn start(bytes: Vec<u8>) -> Self {
            let (requests, inbox) = mpsc::channel::<Job>();
            let (outbox, results) = mpsc::channel::<RenderedPage>();
            let (text_out, texts) = mpsc::channel::<Vec<String>>();
            std::thread::spawn(move || {
                let pdf = open(bytes);
                let cache = RenderCache::new();
                for job in inbox {
                    let sent = match job {
                        Job::Page(page) => {
                            let rendered =
                                pdf.as_ref().map_err(Clone::clone).and_then(|pdf| render_page(pdf, &cache, page));
                            outbox.send((page, rendered)).is_ok()
                        }
                        Job::Text => text_out.send(pdf.as_ref().map(all_text).unwrap_or_default()).is_ok(),
                    };
                    if !sent {
                        break;
                    }
                }
            });
            Self { requests, results, texts, asked: HashSet::new(), text_asked: false }
        }

        /// Asks for a page; repeated asks for the same page are ignored.
        pub fn request(&mut self, page: u32) {
            if self.asked.insert(page) {
                let _ = self.requests.send(Job::Page(page));
            }
        }

        /// Asks (once) for the text of every page.
        pub fn request_text(&mut self) {
            if !self.text_asked {
                self.text_asked = true;
                let _ = self.requests.send(Job::Text);
            }
        }

        /// The text of every page, once it is ready.
        pub fn take_text(&mut self) -> Option<Vec<String>> {
            self.texts.try_recv().ok()
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
    use crate::media::slide_text::page_text;

    /// Renders on the main thread, one queued page per `poll` (per frame),
    /// so the page stays responsive between pages. The render cache borrows
    /// the PDF, so it can't live in this struct; each page gets a fresh one
    /// (pages are cached as textures by the app anyway; P-014). Reading the
    /// deck's text also goes one page per frame, after any pages waiting.
    pub struct DeckRenderer {
        pdf: Result<Pdf, String>,
        queue: VecDeque<u32>,
        /// Text read so far, while reading.
        text: Option<Vec<String>>,
        text_done: Option<Vec<String>>,
        text_asked: bool,
    }

    impl DeckRenderer {
        pub fn start(bytes: Vec<u8>) -> Self {
            Self { pdf: open(bytes), queue: VecDeque::new(), text: None, text_done: None, text_asked: false }
        }

        pub fn request(&mut self, page: u32) {
            if !self.queue.contains(&page) {
                self.queue.push_back(page);
            }
        }

        pub fn request_text(&mut self) {
            if !self.text_asked {
                self.text_asked = true;
                self.text = Some(Vec::new());
            }
        }

        pub fn take_text(&mut self) -> Option<Vec<String>> {
            self.text_done.take()
        }

        pub fn poll(&mut self) -> Vec<RenderedPage> {
            if self.queue.is_empty() {
                self.read_one_page_of_text();
            }
            let Some(page) = self.queue.pop_front() else { return Vec::new() };
            let rendered =
                self.pdf.as_ref().map_err(Clone::clone).and_then(|pdf| render_page(pdf, &RenderCache::new(), page));
            vec![(page, rendered)]
        }

        fn read_one_page_of_text(&mut self) {
            let Some(text) = &mut self.text else { return };
            let Ok(pdf) = &self.pdf else {
                self.text = None;
                return;
            };
            let count = pdf.pages().len();
            if text.len() < count {
                text.push(page_text(pdf, text.len() as u32 + 1).unwrap_or_default());
            }
            if text.len() >= count {
                self.text_done = self.text.take();
            }
        }
    }
}

/// The text of every page (empty for a page whose text can't be read).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn all_text(pdf: &Pdf) -> Vec<String> {
    lectio_export::slides::all_text(pdf)
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
        assert_eq!(image.size[0], lectio_export::slides::TARGET_WIDTH_PX as usize);
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
