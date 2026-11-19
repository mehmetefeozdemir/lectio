//! The browser camera for photo notes (ADR-013).
//!
//! `getUserMedia({video})` asks for camera permission and returns a live
//! stream. A hidden `<video>` element plays it; to show a preview inside egui
//! (which draws on its own canvas) we copy a frame into an off-screen 2D
//! canvas and read its pixels about 15 times per second. A snapshot copies
//! one frame at full resolution and encodes it as JPEG in Rust.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use eframe::egui::ColorImage;
use js_sys::{Object, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlVideoElement, MediaStream, MediaStreamConstraints};

use super::js_err;
use crate::media::images::encode_jpeg;
use crate::platform::{CameraSession, CapturedPhoto};

const PREVIEW_INTERVAL_MS: f64 = 66.0;
const PREVIEW_MAX_WIDTH: u32 = 960;
/// `HTMLMediaElement.HAVE_CURRENT_DATA`: a frame can be drawn.
const HAVE_CURRENT_DATA: u16 = 2;

#[derive(Default)]
struct Shared {
    video: RefCell<Option<HtmlVideoElement>>,
    stream: RefCell<Option<MediaStream>>,
    error: RefCell<Option<String>>,
    closed: Cell<bool>,
}

impl Shared {
    fn stop(&self) {
        if let Some(stream) = self.stream.borrow_mut().take() {
            for track in stream.get_tracks().iter() {
                if let Ok(track) = track.dyn_into::<web_sys::MediaStreamTrack>() {
                    track.stop(); // turns the browser's camera light off
                }
            }
        }
        if let Some(video) = self.video.borrow_mut().take() {
            video.set_src_object(None);
        }
    }
}

pub struct WebCamera {
    shared: Rc<Shared>,
    canvas: Option<(HtmlCanvasElement, CanvasRenderingContext2d)>,
    last_preview_ms: f64,
}

impl WebCamera {
    /// Returns at once; the preview starts when the user allows the camera.
    pub fn open() -> Self {
        let shared = Rc::new(Shared::default());
        let connecting = Rc::clone(&shared);
        wasm_bindgen_futures::spawn_local(async move {
            match connect().await {
                Ok((stream, video)) => {
                    *connecting.stream.borrow_mut() = Some(stream);
                    *connecting.video.borrow_mut() = Some(video);
                    if connecting.closed.get() {
                        connecting.stop();
                    }
                }
                Err(e) => *connecting.error.borrow_mut() = Some(e),
            }
        });
        let canvas = offscreen_canvas().map_err(|e| *shared.error.borrow_mut() = Some(e)).ok();
        Self { shared, canvas, last_preview_ms: 0.0 }
    }

    /// Copies the current video frame, scaled to `width`×`height`, as RGBA.
    fn grab(&self, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let video = self.shared.video.borrow();
        let video = video.as_ref().ok_or("The camera is not ready yet.")?;
        let (canvas, ctx) = self.canvas.as_ref().ok_or("This browser can't copy camera frames.")?;
        canvas.set_width(width);
        canvas.set_height(height);
        ctx.draw_image_with_html_video_element_and_dw_and_dh(video, 0.0, 0.0, width.into(), height.into())
            .map_err(js_err)?;
        let pixels = ctx.get_image_data(0.0, 0.0, width.into(), height.into()).map_err(js_err)?;
        Ok(pixels.data().0)
    }

    /// The camera's own resolution, once frames are arriving.
    fn frame_size(&self) -> Option<(u32, u32)> {
        let video = self.shared.video.borrow();
        let video = video.as_ref()?;
        let ready = video.ready_state() >= HAVE_CURRENT_DATA && video.video_width() > 0;
        ready.then(|| (video.video_width(), video.video_height()))
    }
}

impl CameraSession for WebCamera {
    fn preview(&mut self) -> Option<ColorImage> {
        let now = js_sys::Date::now();
        if now - self.last_preview_ms < PREVIEW_INTERVAL_MS {
            return None;
        }
        let (w, h) = self.frame_size()?;
        let scale = (PREVIEW_MAX_WIDTH as f32 / w as f32).min(1.0);
        let (pw, ph) = (((w as f32) * scale) as u32, ((h as f32) * scale) as u32);
        let rgba = self.grab(pw, ph).ok()?;
        self.last_preview_ms = now;
        Some(ColorImage::from_rgba_unmultiplied([pw as usize, ph as usize], &rgba))
    }

    fn take_error(&self) -> Option<String> {
        self.shared.error.borrow_mut().take()
    }

    fn snapshot(&mut self) -> Result<CapturedPhoto, String> {
        let (width, height) = self.frame_size().ok_or("The camera is not ready yet.")?;
        let rgba = self.grab(width, height)?;
        Ok(CapturedPhoto { jpeg: encode_jpeg(&rgba, width, height)?, width, height })
    }
}

impl Drop for WebCamera {
    fn drop(&mut self) {
        self.shared.closed.set(true);
        self.shared.stop();
    }
}

async fn connect() -> Result<(MediaStream, HtmlVideoElement), String> {
    let window = web_sys::window().ok_or("no browser window")?;
    let media = window
        .navigator()
        .media_devices()
        .map_err(|_| "This page can't use the camera (it needs https:// or localhost).".to_string())?;
    let constraints = MediaStreamConstraints::new();
    constraints.set_video(&video_constraints());
    constraints.set_audio_bool(false);
    let request = media.get_user_media_with_constraints(&constraints).map_err(js_err)?;
    let stream: MediaStream = JsFuture::from(request)
        .await
        .map_err(|e| format!("Camera access failed: {}", js_err(e)))?
        .dyn_into()
        .map_err(|_| "the browser did not return a camera stream".to_string())?;

    let document = window.document().ok_or("no document")?;
    let video: HtmlVideoElement =
        document.create_element("video").map_err(js_err)?.dyn_into().map_err(|_| "no <video> element")?;
    video.set_muted(true);
    let _ = video.set_attribute("playsinline", ""); // iOS: don't jump to full screen
    video.set_src_object(Some(&stream));
    JsFuture::from(video.play().map_err(js_err)?).await.map_err(js_err)?;
    Ok((stream, video))
}

/// Prefer the rear camera (pointing at the board) where there is one, at up
/// to Full HD.
fn video_constraints() -> JsValue {
    let ideal = |value: JsValue| {
        let o = Object::new();
        let _ = Reflect::set(&o, &"ideal".into(), &value);
        JsValue::from(o)
    };
    let video = Object::new();
    let _ = Reflect::set(&video, &"facingMode".into(), &ideal("environment".into()));
    let _ = Reflect::set(&video, &"width".into(), &ideal(1920.into()));
    let _ = Reflect::set(&video, &"height".into(), &ideal(1080.into()));
    video.into()
}

/// A canvas that is never added to the page, used only to read pixels.
/// `willReadFrequently` tells the browser to keep it in CPU memory, which
/// makes repeated `getImageData` calls fast.
pub(super) fn offscreen_canvas() -> Result<(HtmlCanvasElement, CanvasRenderingContext2d), String> {
    let document = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let canvas: HtmlCanvasElement =
        document.create_element("canvas").map_err(js_err)?.dyn_into().map_err(|_| "no <canvas> element")?;
    let options = Object::new();
    let _ = Reflect::set(&options, &"willReadFrequently".into(), &JsValue::TRUE);
    let ctx = canvas
        .get_context_with_context_options("2d", &options)
        .map_err(js_err)?
        .ok_or("no 2D canvas")?
        .dyn_into::<CanvasRenderingContext2d>()
        .map_err(|_| "no 2D canvas".to_string())?;
    Ok((canvas, ctx))
}
