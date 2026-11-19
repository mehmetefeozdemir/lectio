//! Browser recording: `getUserMedia` (microphone permission) → an
//! `AudioWorklet` that runs on the browser's audio thread and posts batches
//! of mono samples → this code, which converts them to 16 kHz with the same
//! `SpeechConverter` the native writer thread uses (ADR-009, ADR-010).
//!
//! The worklet is `web/lectio-capture-worklet.js`, copied next to the page
//! by trunk. When recording stops, the samples are encoded as WAV and stored
//! in IndexedDB under the session's file name.

use std::cell::{Cell, RefCell};
use std::io::Cursor;
use std::rc::Rc;

use js_sys::{Float32Array, Object, Reflect, Uint8Array};
use lectio_core::resample::{SpeechConverter, to_i16};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioContext, AudioWorkletNode, IdbDatabase, MediaStream, MediaStreamConstraints, MessageEvent};

use super::{ErrorSink, display, idb, js_err, report};
use crate::platform::wav::encode_mono_i16;
use crate::platform::{AudioCapture, AudioError, CaptureSource, RecorderStats, STORE_SAMPLE_RATE};

const WORKLET_URL: &str = "./lectio-capture-worklet.js";
const PROCESSOR_NAME: &str = "lectio-capture";
/// Live captions read at most this much recent audio (16 kHz samples).
const RECENT_MAX: usize = 30 * STORE_SAMPLE_RATE as usize;

/// The live audio graph: inputs → worklet. Kept so it can be torn down.
struct Graph {
    ctx: AudioContext,
    streams: Vec<MediaStream>,
    node: AudioWorkletNode,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
}

impl Graph {
    fn tear_down(self) {
        if let Ok(port) = self.node.port() {
            port.set_onmessage(None);
        }
        let _ = self.node.disconnect();
        // Stopping the tracks turns the browser's recording/sharing indicator off.
        self.streams.iter().for_each(display::stop);
        let _ = self.ctx.close();
    }
}

#[derive(Default)]
struct Shared {
    paused: Cell<bool>,
    stopped: Cell<bool>,
    converter: Cell<Option<SpeechConverter>>,
    input_rate: Cell<u32>,
    samples: RefCell<Vec<i16>>,
    scratch: RefCell<Vec<f32>>,
    frames_captured: Cell<u64>,
    batches: Cell<u64>,
    peak: Cell<f32>,
    /// Loudest sample of the latest batch (for the level meter).
    level: Cell<f32>,
    error: RefCell<Option<String>>,
    graph: RefCell<Option<Graph>>,
}

impl Shared {
    /// One batch of mono samples from the worklet.
    fn receive(&self, data: &JsValue) {
        let (Some(batch), Some(converter)) = (data.dyn_ref::<Float32Array>(), self.converter.get()) else {
            return;
        };
        if self.paused.get() || self.stopped.get() {
            return;
        }
        let input = batch.to_vec();
        let mut out = self.scratch.borrow_mut();
        out.clear();
        self.converter.set(Some(converter.process(&input, &mut out)));
        let batch_peak = out.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        self.level.set(batch_peak);
        self.peak.set(self.peak.get().max(batch_peak));
        self.samples.borrow_mut().extend(out.iter().map(|&s| to_i16(s)));
        self.frames_captured.set(self.frames_captured.get() + input.len() as u64);
        self.batches.set(self.batches.get() + 1);
    }
}

pub struct WebRecorder {
    shared: Rc<Shared>,
    db: IdbDatabase,
    file_name: String,
    errors: ErrorSink,
}

impl WebRecorder {
    /// Returns at once; the microphone connects when the user allows it.
    /// Until then the clock stays at 0:00.
    pub fn start(db: IdbDatabase, file_name: &str, source: CaptureSource, errors: ErrorSink) -> Self {
        let shared = Rc::new(Shared::default());
        let connecting = Rc::clone(&shared);
        wasm_bindgen_futures::spawn_local(async move {
            match connect(Rc::clone(&connecting), source).await {
                Ok(graph) if connecting.stopped.get() => graph.tear_down(),
                Ok(graph) => *connecting.graph.borrow_mut() = Some(graph),
                Err(e) => *connecting.error.borrow_mut() = Some(e),
            }
        });
        Self { shared, db, file_name: file_name.to_owned(), errors }
    }
}

async fn microphone() -> Result<MediaStream, String> {
    let window = web_sys::window().ok_or("no browser window")?;
    let media = window
        .navigator()
        .media_devices()
        .map_err(|_| "This page can't use the microphone (it needs https:// or localhost)".to_string())?;
    let constraints = MediaStreamConstraints::new();
    constraints.set_audio(&raw_audio_constraints());
    let request = media.get_user_media_with_constraints(&constraints).map_err(js_err)?;
    JsFuture::from(request)
        .await
        .map_err(|e| format!("Microphone access failed: {}", js_err(e)))?
        .dyn_into()
        .map_err(|_| "the browser did not return a media stream".to_string())
}

/// Microphone and/or a shared tab's sound; with both, Web Audio adds the
/// two inputs of the worklet together (ADR-019).
async fn connect(shared: Rc<Shared>, source: CaptureSource) -> Result<Graph, String> {
    let mut streams = Vec::new();
    if source != CaptureSource::Computer {
        streams.push(microphone().await?);
    }
    if source != CaptureSource::Microphone {
        match display::share(true).await {
            Ok(stream) => streams.push(stream),
            Err(e) => {
                streams.iter().for_each(display::stop);
                return Err(e);
            }
        }
    }

    let ctx = AudioContext::new().map_err(js_err)?;
    let worklet = ctx.audio_worklet().map_err(js_err)?;
    JsFuture::from(worklet.add_module(WORKLET_URL).map_err(js_err)?)
        .await
        .map_err(|e| format!("Could not load the recording worklet: {}", js_err(e)))?;
    let node = AudioWorkletNode::new(&ctx, PROCESSOR_NAME).map_err(js_err)?;
    for stream in &streams {
        let input = ctx.create_media_stream_source(stream).map_err(js_err)?;
        input.connect_with_audio_node(&node).map_err(js_err)?;
    }
    // The worklet outputs silence; connecting it keeps the browser running it.
    node.connect_with_audio_node(&ctx.destination()).map_err(js_err)?;

    let rate = ctx.sample_rate() as u32;
    shared.input_rate.set(rate);
    shared.converter.set(Some(SpeechConverter::new(1, rate, STORE_SAMPLE_RATE)));
    let receiver = Rc::clone(&shared);
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| receiver.receive(&event.data()));
    node.port().map_err(js_err)?.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    if let Ok(resume) = ctx.resume() {
        let _ = JsFuture::from(resume).await;
    }
    Ok(Graph { ctx, streams, node, _on_message: on_message })
}

/// Lecture audio as the microphone hears it: the browser's echo
/// cancellation, noise suppression and automatic gain are meant for calls
/// and can pump or cut a lecturer's voice. Clean-up is a Phase 3 feature.
fn raw_audio_constraints() -> JsValue {
    let audio = Object::new();
    for (key, value) in [
        ("echoCancellation", JsValue::FALSE),
        ("noiseSuppression", JsValue::FALSE),
        ("autoGainControl", JsValue::FALSE),
        ("channelCount", JsValue::from(1)),
    ] {
        let _ = Reflect::set(&audio, &JsValue::from_str(key), &value);
    }
    audio.into()
}

impl AudioCapture for WebRecorder {
    fn set_paused(&self, paused: bool) {
        self.shared.paused.set(paused);
    }

    fn samples_written(&self) -> u64 {
        self.shared.samples.borrow().len() as u64
    }

    fn level(&self) -> f32 {
        self.shared.level.get()
    }

    fn stats(&self) -> RecorderStats {
        let s = &self.shared;
        RecorderStats {
            device_name: "Browser microphone".into(),
            device_sample_rate: s.input_rate.get(),
            channels: 1,
            sample_format: "f32 (AudioWorklet)".into(),
            frames_captured: s.frames_captured.get(),
            callbacks: s.batches.get(),
            samples_written: self.samples_written(),
            peak: s.peak.get(),
            ..RecorderStats::default()
        }
    }

    fn recent_audio(&self, ms: u64) -> Vec<f32> {
        let wanted = ((ms * u64::from(STORE_SAMPLE_RATE) / 1_000) as usize).min(RECENT_MAX);
        let samples = self.shared.samples.borrow();
        samples[samples.len().saturating_sub(wanted)..].iter().map(|&s| f32::from(s) / f32::from(i16::MAX)).collect()
    }

    fn take_error(&self) -> Option<String> {
        self.shared.error.borrow_mut().take()
    }

    /// Stops the microphone and stores the WAV in IndexedDB in the background.
    fn finish(self: Box<Self>) -> Result<u64, AudioError> {
        self.shared.stopped.set(true);
        if let Some(graph) = self.shared.graph.borrow_mut().take() {
            graph.tear_down();
        }
        let samples = std::mem::take(&mut *self.shared.samples.borrow_mut());
        let mut wav = Cursor::new(Vec::new());
        encode_mono_i16(&samples, STORE_SAMPLE_RATE, &mut wav)?;
        let bytes = Uint8Array::from(wav.get_ref().as_slice());
        let (db, file_name, errors) = (self.db.clone(), self.file_name.clone(), self.errors.clone());
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = idb::put(&db, idb::RECORDINGS, &file_name, &bytes).await {
                report(&errors, format!("the recording could not be stored ({e})"));
            }
        });
        Ok(samples.len() as u64)
    }
}
