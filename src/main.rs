//! Lectio: lecture notes timestamped to the audio (ADR-009: one codebase for
//! Windows, macOS, Linux and the web).
//!
//! Native usage:
//!   lectio                         open the app with your saved data
//!   lectio --screenshot out.png    open sample data, draw ~120 frames,
//!                                  save a PNG of the window and quit
//!   lectio --self-test-audio 10    record 10 s with no window and print
//!                                  captured / dropped frames and overruns
//!
//! Web: `trunk build --release` produces `dist/`; open `index.html#sample`
//! for sample data.

mod action;
mod app;
mod demo;
mod material;
mod media;
mod platform;
mod ui;

#[cfg(not(target_arch = "wasm32"))]
mod entry_native;
#[cfg(not(target_arch = "wasm32"))]
mod screenshot;
#[cfg(not(target_arch = "wasm32"))]
mod self_test;

#[cfg(target_arch = "wasm32")]
mod entry_web;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    entry_native::run()
}

#[cfg(target_arch = "wasm32")]
fn main() {
    entry_web::start();
}
