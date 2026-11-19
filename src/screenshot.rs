//! `--screenshot <file.png>`: draw a fixed number of frames, capture the
//! window, save it as PNG, then close. Used by the build checks.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;

/// Frames to draw before capturing, so layout and fonts have settled.
const FRAMES_BEFORE_CAPTURE: u32 = 120;
/// Give up if the capture never arrives (e.g. the window can't draw).
const MAX_FRAMES: u32 = 1_200;

type Outcome = Arc<Mutex<Option<Result<(), String>>>>;

pub struct ScreenshotDriver {
    path: PathBuf,
    frames: u32,
    requested: bool,
    outcome: Outcome,
}

impl ScreenshotDriver {
    pub fn new(path: PathBuf) -> Self {
        Self { path, frames: 0, requested: false, outcome: Arc::new(Mutex::new(None)) }
    }

    /// Call once per frame. Keeps the window redrawing even when unfocused.
    pub fn on_frame(&mut self, ctx: &egui::Context) {
        ctx.request_repaint();
        self.frames += 1;

        if self.frames >= FRAMES_BEFORE_CAPTURE && !self.requested {
            self.requested = true;
            let path = self.path.clone();
            let outcome = Arc::clone(&self.outcome);
            let repaint = ctx.clone();
            ctx.request_screenshot(move |image| {
                let result = save_png(&path, &image);
                if let Ok(mut slot) = outcome.lock() {
                    *slot = Some(result);
                }
                repaint.request_repaint();
            });
        }

        let finished = self.outcome.lock().ok().and_then(|mut slot| slot.take());
        match finished {
            Some(Ok(())) => {
                println!("Saved screenshot to {}", self.path.display());
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(Err(message)) => fail(&message),
            None if self.frames > MAX_FRAMES => fail("the window never delivered a screenshot"),
            None => {}
        }
    }
}

fn fail(message: &str) -> ! {
    eprintln!("Screenshot failed: {message}");
    std::process::exit(1);
}

fn save_png(path: &std::path::Path, image: &egui::ColorImage) -> Result<(), String> {
    let [width, height] = image.size;
    let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
    let buffer = image::RgbaImage::from_raw(width as u32, height as u32, bytes)
        .ok_or_else(|| "image size did not match its pixels".to_string())?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    buffer.save(path).map_err(|e| format!("{}: {e}", path.display()))
}
