//! Browser playback with Web Audio. The WAV is loaded from IndexedDB into an
//! `AudioBuffer` once; playing starts an `AudioBufferSourceNode` at an exact
//! offset, and seeking replaces it with a new one at the new offset. (A
//! source node can only be started once, so this is how Web Audio is meant
//! to be used.) The position is computed from the audio clock
//! `AudioContext.currentTime`, not a timer.

use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;

use js_sys::Uint8Array;
use lectio_core::timeline::{clamp_offset, samples_to_ms};
use web_sys::{AudioBuffer, AudioBufferSourceNode, AudioContext, IdbDatabase};

use super::{idb, js_err};
use crate::platform::AudioPlayback;
use crate::platform::wav::decode_mono_i16;

const MS_PER_S: f64 = 1_000.0;

enum State {
    Loading,
    Ready(Loaded),
    /// The message is handed to the app once, then kept as `None`.
    Failed(Option<String>),
}

struct Loaded {
    ctx: AudioContext,
    buffer: AudioBuffer,
    duration_ms: u64,
    source: Option<AudioBufferSourceNode>,
    /// `ctx.currentTime` (seconds) when the current source started.
    started_at: f64,
    started_from_ms: u64,
    paused_at_ms: u64,
}

pub struct WebPlayer {
    state: Rc<RefCell<State>>,
}

impl WebPlayer {
    /// Returns at once; `is_ready` turns true when the audio has loaded.
    pub fn open(db: IdbDatabase, file_name: &str) -> Self {
        let state = Rc::new(RefCell::new(State::Loading));
        let loading = Rc::clone(&state);
        let file_name = file_name.to_owned();
        wasm_bindgen_futures::spawn_local(async move {
            let next = match load(&db, &file_name).await {
                Ok(loaded) => State::Ready(loaded),
                Err(e) => State::Failed(Some(e)),
            };
            *loading.borrow_mut() = next;
        });
        Self { state }
    }

    fn with_loaded<R>(&self, default: R, f: impl FnOnce(&mut Loaded) -> Result<R, String>) -> R {
        let mut state = self.state.borrow_mut();
        let State::Ready(loaded) = &mut *state else { return default };
        match f(loaded) {
            Ok(value) => value,
            Err(e) => {
                *state = State::Failed(Some(e));
                default
            }
        }
    }
}

async fn load(db: &IdbDatabase, file_name: &str) -> Result<Loaded, String> {
    let value = idb::get(db, idb::RECORDINGS, file_name)
        .await?
        .ok_or("The audio for this session is not in this browser's storage.")?;
    let bytes = Uint8Array::new(&value).to_vec();
    let (samples, rate) = decode_mono_i16(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let floats: Vec<f32> = samples.iter().map(|&s| f32::from(s) / f32::from(i16::MAX)).collect();

    let ctx = AudioContext::new().map_err(js_err)?;
    let length = floats.len().max(1) as u32;
    let buffer = ctx.create_buffer(1, length, rate as f32).map_err(js_err)?;
    buffer.copy_to_channel(&floats, 0).map_err(js_err)?;
    let duration_ms = samples_to_ms(samples.len() as u64, rate);
    Ok(Loaded { ctx, buffer, duration_ms, source: None, started_at: 0.0, started_from_ms: 0, paused_at_ms: 0 })
}

impl Loaded {
    fn position_ms(&self) -> u64 {
        if self.source.is_none() {
            return self.paused_at_ms;
        }
        let elapsed_ms = ((self.ctx.current_time() - self.started_at).max(0.0) * MS_PER_S) as u64;
        clamp_offset(self.started_from_ms + elapsed_ms, self.duration_ms)
    }

    fn is_playing(&self) -> bool {
        self.source.is_some() && self.position_ms() < self.duration_ms
    }

    fn start_at(&mut self, offset_ms: u64) -> Result<(), String> {
        self.stop_source();
        if let Ok(resume) = self.ctx.resume() {
            drop(resume); // the browser resumes on its own; no need to wait
        }
        let source = self.ctx.create_buffer_source().map_err(js_err)?;
        source.set_buffer(Some(&self.buffer));
        source.connect_with_audio_node(&self.ctx.destination()).map_err(js_err)?;
        source.start_with_when_and_grain_offset(0.0, offset_ms as f64 / MS_PER_S).map_err(js_err)?;
        self.started_at = self.ctx.current_time();
        self.started_from_ms = offset_ms;
        self.source = Some(source);
        Ok(())
    }

    fn stop_source(&mut self) {
        if let Some(source) = self.source.take() {
            // `AudioBufferSourceNode::stop` is deprecated in web-sys; the
            // parent class's `stop` is the current binding.
            let _ = web_sys::AudioScheduledSourceNode::stop(&source);
            let _ = source.disconnect();
        }
    }
}

impl AudioPlayback for WebPlayer {
    fn is_ready(&self) -> bool {
        matches!(*self.state.borrow(), State::Ready(_))
    }

    fn take_error(&self) -> Option<String> {
        match &mut *self.state.borrow_mut() {
            State::Failed(message) => message.take(),
            _ => None,
        }
    }

    fn position_ms(&self) -> u64 {
        self.with_loaded(0, |l| Ok(l.position_ms()))
    }

    fn is_playing(&self) -> bool {
        self.with_loaded(false, |l| Ok(l.is_playing()))
    }

    fn set_playing(&self, playing: bool) {
        self.with_loaded((), |l| {
            if !playing {
                l.paused_at_ms = l.position_ms();
                l.stop_source();
                return Ok(());
            }
            let at_end = l.position_ms() >= l.duration_ms;
            let from = if at_end { 0 } else { l.position_ms() };
            l.start_at(from)
        });
    }

    fn seek_ms(&self, offset_ms: u64) {
        self.with_loaded((), |l| {
            let target = clamp_offset(offset_ms, l.duration_ms);
            if l.is_playing() {
                return l.start_at(target);
            }
            l.stop_source();
            l.paused_at_ms = target;
            Ok(())
        });
    }
}
