//! Screen sharing in the browser (`getDisplayMedia`, ADR-019): the audio of
//! a shared tab or screen for "Computer audio", and one video frame for
//! "Screenshot to feed". The browser always asks the user what to share.
//!
//! Called through `Reflect` so it works the same in every browser that has
//! the function, and gives a clear message in those that don't.

use eframe::egui::ColorImage;
use js_sys::{Function, Object, Promise, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlVideoElement, MediaStream, MediaStreamTrack};

use super::camera::offscreen_canvas;
use super::js_err;

/// `HTMLMediaElement.HAVE_CURRENT_DATA`: a frame can be drawn.
const HAVE_CURRENT_DATA: u16 = 2;
const FRAME_WAIT_MS: i32 = 50;
const FRAME_TRIES: u32 = 60;

pub const UNSUPPORTED: &str = "This browser can't share a screen or tab here. Use a desktop browser (Chrome or Edge \
                               also share a tab's sound) on an https:// page or localhost.";

fn get_display_media() -> Option<(JsValue, Function)> {
    let devices = web_sys::window()?.navigator().media_devices().ok()?;
    let function = Reflect::get(&devices, &"getDisplayMedia".into()).ok()?.dyn_into::<Function>().ok()?;
    Some((devices.into(), function))
}

pub fn supported() -> bool {
    get_display_media().is_some()
}

/// Asks the user to share a screen, window or tab. With `audio`, its sound
/// must be shared too (in Chrome: "Share tab audio" / "Share system audio").
pub async fn share(audio: bool) -> Result<MediaStream, String> {
    let (devices, function) = get_display_media().ok_or(UNSUPPORTED)?;
    let constraints = Object::new();
    let _ = Reflect::set(&constraints, &"video".into(), &JsValue::TRUE);
    let _ = Reflect::set(&constraints, &"audio".into(), &JsValue::from_bool(audio));
    let promise: Promise = function.call1(&devices, &constraints).map_err(js_err)?.dyn_into().map_err(js_err)?;
    let stream: MediaStream = JsFuture::from(promise)
        .await
        .map_err(|e| format!("Screen sharing was not started: {}", js_err(e)))?
        .dyn_into()
        .map_err(|_| "the browser did not return a stream".to_string())?;
    if audio && stream.get_audio_tracks().length() == 0 {
        stop(&stream);
        return Err("No sound was shared. Share a tab (or the whole screen) and tick \"Share audio\".".into());
    }
    Ok(stream)
}

pub fn stop(stream: &MediaStream) {
    for track in stream.get_tracks().iter() {
        if let Ok(track) = track.dyn_into::<MediaStreamTrack>() {
            track.stop();
        }
    }
}

fn sleep(ms: i32) -> JsFuture {
    let promise = Promise::new(&mut |resolve, _| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    JsFuture::from(promise)
}

/// One full-resolution frame of a screen the user picks.
pub async fn grab_screen() -> Result<ColorImage, String> {
    let stream = share(false).await?;
    let result = grab_frame(&stream).await;
    stop(&stream);
    result
}

async fn grab_frame(stream: &MediaStream) -> Result<ColorImage, String> {
    let document = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let video: HtmlVideoElement =
        document.create_element("video").map_err(js_err)?.dyn_into().map_err(|_| "no <video> element")?;
    video.set_muted(true);
    video.set_src_object(Some(stream));
    JsFuture::from(video.play().map_err(js_err)?).await.map_err(js_err)?;
    for _ in 0..FRAME_TRIES {
        if video.ready_state() >= HAVE_CURRENT_DATA && video.video_width() > 0 {
            break;
        }
        let _ = sleep(FRAME_WAIT_MS).await;
    }
    let (width, height) = (video.video_width(), video.video_height());
    if width == 0 {
        return Err("The shared screen sent no picture.".into());
    }
    let (canvas, ctx) = offscreen_canvas()?;
    canvas.set_width(width);
    canvas.set_height(height);
    ctx.draw_image_with_html_video_element_and_dw_and_dh(&video, 0.0, 0.0, width.into(), height.into())
        .map_err(js_err)?;
    let pixels = ctx.get_image_data(0.0, 0.0, width.into(), height.into()).map_err(js_err)?;
    Ok(ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &pixels.data()))
}
