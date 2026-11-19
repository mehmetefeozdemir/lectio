//! Web entry point: opens browser storage, then starts the egui app on the
//! page's canvas. `index.html#sample` starts with sample data instead.

use eframe::wasm_bindgen::JsCast;

use crate::app::{LectioApp, Startup};
use crate::platform::Platform;
use crate::platform::web::WebPlatform;

const CANVAS_ID: &str = "lectio_canvas";
const STATUS_ID: &str = "lectio_status";

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

    let wants_sample = window.location().hash().is_ok_and(|hash| hash == "#sample");
    let startup = if wants_sample {
        Startup::Sample
    } else {
        Startup::Saved(WebPlatform::open().await.map(|p| Box::new(p) as Box<dyn Platform>))
    };

    eframe::WebRunner::new()
        .start(canvas, eframe::WebOptions::default(), Box::new(move |cc| Ok(Box::new(LectioApp::new(cc, startup)))))
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
