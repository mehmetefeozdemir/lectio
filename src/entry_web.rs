//! Web entry point: opens browser storage, then starts the egui app on the
//! page's canvas. `index.html#sample` starts with sample data instead.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;
use eframe::wasm_bindgen::{JsCast, JsValue};

use crate::app::{LectioApp, Startup};
use crate::platform::Platform;
use crate::platform::web::WebPlatform;

const CANVAS_ID: &str = "lectio_canvas";
const STATUS_ID: &str = "lectio_status";
/// Set by `#test` in the URL: publish named controls for the browser test.
static TEST_HOOKS: AtomicBool = AtomicBool::new(false);
const WIDGETS_GLOBAL: &str = "__lectioWidgets";

pub fn start() {
    wasm_bindgen_futures::spawn_local(async {
        match run().await {
            Ok(()) => set_status(None),
            Err(message) => set_status(Some(&format!("Lectio could not start: {message}"))),
        }
    });
}

async fn run() -> Result<(), String> {
    let window = web_sys::window().ok_or("no browser window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id(CANVAS_ID)
        .ok_or("the page has no canvas")?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| "the canvas element is not a canvas")?;

    let hash = window.location().hash().unwrap_or_default();
    let wants_sample = hash.contains("sample");
    TEST_HOOKS.store(hash.contains("test"), Ordering::Relaxed);
    let startup = if wants_sample {
        Startup::Sample
    } else {
        Startup::Saved(WebPlatform::open().await.map(|p| Box::new(p) as Box<dyn Platform>))
    };

    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(move |cc| {
                // Screen readers (ADR-027): mirror egui's accessibility tree into the page.
                cc.egui_ctx.add_plugin(crate::platform::web::a11y::ScreenReaderMirror);
                Ok(Box::new(LectioApp::new(cc, startup)))
            }),
        )
        .await
        .map_err(|e| format!("{e:?}"))
}

/// Shows a message in the page's status line, or hides it.
fn set_status(message: Option<&str>) {
    let Some(element) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id(STATUS_ID))
    else {
        return;
    };
    match message {
        Some(text) => element.set_text_content(Some(text)),
        None => element.remove(),
    }
}

/// With `#test` in the URL, publishes `[{name, x, y, w, h}]` (CSS pixels) of
/// every named control as `window.__lectioWidgets`, so the browser test can
/// click controls by their accessible name. egui draws everything on one
/// canvas, so there is no DOM element to find.
pub fn publish_named_widgets(ctx: &egui::Context) {
    if !TEST_HOOKS.load(Ordering::Relaxed) {
        return;
    }
    let entries: Vec<String> = crate::ui::widgets::named_widgets(ctx)
        .into_iter()
        .map(|(name, r)| {
            let name = name.replace('\\', "\\\\").replace('"', "\\\"");
            format!(r#"{{"name":"{name}","x":{},"y":{},"w":{},"h":{}}}"#, r.left(), r.top(), r.width(), r.height())
        })
        .collect();
    if let Some(window) = web_sys::window() {
        let json = JsValue::from_str(&format!("[{}]", entries.join(",")));
        let _ = js_sys::Reflect::set(&window, &JsValue::from_str(WIDGETS_GLOBAL), &json);
    }
}
