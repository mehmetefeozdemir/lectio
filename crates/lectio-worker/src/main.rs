//! Lectio's speech worker for the browser (ADR-016).
//!
//! A *Web Worker* is a separate JavaScript thread with its own WebAssembly
//! copy of this program. Running Whisper here keeps the page's main thread
//! (the UI) free while a lecture is transcribed. The page sends commands
//! (`lectio_speech::protocol::ToWorker`) with the model files or audio as
//! transferred buffers; the worker answers with `FromWorker` messages.
//! Commands run one at a time, in order; between 30-second windows the
//! worker yields so that a "cancel" message can arrive.

#[cfg(target_arch = "wasm32")]
mod worker;

fn main() {
    #[cfg(target_arch = "wasm32")]
    worker::start();
}
