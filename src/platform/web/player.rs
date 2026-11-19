//! Browser playback (ADR-010, ADR-017) with an `<audio>` element playing the
//! WAV from IndexedDB through a temporary `blob:` URL.
//!
//! Why an `<audio>` element and not Web Audio buffers: browsers already
//! change a media element's speed without changing the pitch
//! (`playbackRate` with `preservesPitch`), on their own audio thread. Doing
//! the same in Rust on the page's main thread would stutter whenever the UI
//! is busy. The filters (noise reduction, voice boost, normalise) are applied
//! here in Rust with the same `FilterChain` as the desktop, a slice per
//! frame so the page stays responsive, and the filtered copy is played.

use std::cell::{Cell, RefCell};
use std::io::Cursor;
use std::rc::Rc;

use js_sys::{Array, Reflect, Uint8Array};
use lectio_core::dsp::{Analysis, FilterChain, FilterSettings, analyse};
use lectio_core::resample::to_i16;
use lectio_core::timeline::{clamp_offset, samples_to_ms};
use wasm_bindgen::JsValue;
use web_sys::{Blob, BlobPropertyBag, HtmlAudioElement, IdbDatabase, Url};

use super::{idb, js_err};
use crate::platform::AudioPlayback;
use crate::platform::wav::{decode_mono_i16, encode_mono_i16};

const MS_PER_S: f64 = 1_000.0;
/// Samples filtered per frame (about 25 s of 16 kHz audio).
const FILTER_SLICE: usize = 400_000;
/// `HTMLMediaElement.HAVE_METADATA`: duration known, seeking works.
const HAVE_METADATA: u16 = 1;

enum State {
    Loading,
    Ready(Box<Loaded>),
    /// The message is handed to the app once, then kept as `None`.
    Failed(Option<String>),
}

/// A filtered copy being made, one slice per frame.
struct Filtering {
    chain: FilterChain,
    out: Vec<f32>,
}

struct Loaded {
    audio: HtmlAudioElement,
    url: String,
    original_url: String,
    samples: Vec<f32>,
    rate: u32,
    analysis: Analysis,
    duration_ms: u64,
    filtering: Option<Filtering>,
}

pub struct WebPlayer {
    state: Rc<RefCell<State>>,
    speed: Rc<Cell<u16>>,
    filters: Rc<Cell<FilterSettings>>,
}

impl WebPlayer {
    /// Returns at once; `is_ready` turns true when the audio has loaded.
    pub fn open(db: IdbDatabase, file_name: &str) -> Self {
        let state = Rc::new(RefCell::new(State::Loading));
        let speed = Rc::new(Cell::new(100));
        let filters = Rc::new(Cell::new(FilterSettings::default()));
        let (loading, wanted_speed, wanted_filters) = (Rc::clone(&state), Rc::clone(&speed), Rc::clone(&filters));
        let file_name = file_name.to_owned();
        wasm_bindgen_futures::spawn_local(async move {
            let next = match load(&db, &file_name).await {
                Ok(mut loaded) => {
                    loaded.set_speed(wanted_speed.get());
                    loaded.set_filters(wanted_filters.get());
                    State::Ready(Box::new(loaded))
                }
                Err(e) => State::Failed(Some(e)),
            };
            *loading.borrow_mut() = next;
        });
        Self { state, speed, filters }
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

fn blob_url(wav: &[u8]) -> Result<String, String> {
    let parts = Array::of1(&Uint8Array::from(wav).into());
    let options = BlobPropertyBag::new();
    options.set_type("audio/wav");
    let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &options).map_err(js_err)?;
    Url::create_object_url_with_blob(&blob).map_err(js_err)
}

async fn load(db: &IdbDatabase, file_name: &str) -> Result<Loaded, String> {
    let value = idb::get(db, idb::RECORDINGS, file_name)
        .await?
        .ok_or("The audio for this session is not in this browser's storage.")?;
    let bytes = Uint8Array::new(&value).to_vec();
    let (pcm, rate) = decode_mono_i16(Cursor::new(&bytes)).map_err(|e| e.to_string())?;
    let samples: Vec<f32> = pcm.iter().map(|&s| f32::from(s) / f32::from(i16::MAX)).collect();
    let analysis = analyse(&samples, rate);
    let url = blob_url(&bytes)?;
    let audio = HtmlAudioElement::new_with_src(&url).map_err(js_err)?;
    audio.set_preload("auto");
    // Keep the voice's pitch when the speed changes (older Safari uses the prefixed name).
    for key in ["preservesPitch", "webkitPreservesPitch"] {
        let _ = Reflect::set(&audio, &JsValue::from_str(key), &JsValue::TRUE);
    }
    let duration_ms = samples_to_ms(pcm.len() as u64, rate);
    Ok(Loaded { audio, original_url: url.clone(), url, samples, rate, analysis, duration_ms, filtering: None })
}

impl Loaded {
    fn set_speed(&self, percent: u16) {
        self.audio.set_playback_rate(f64::from(percent) / 100.0);
    }

    fn set_filters(&mut self, settings: FilterSettings) {
        if !settings.any() {
            self.filtering = None;
            self.switch_to(self.original_url.clone());
            return;
        }
        let chain = FilterChain::new(settings, &self.analysis, self.rate);
        self.filtering = Some(Filtering { chain, out: Vec::with_capacity(self.samples.len()) });
    }

    /// Filters the next slice; when all is done, plays the filtered copy.
    fn step_filtering(&mut self) -> Result<(), String> {
        let Some(job) = &mut self.filtering else { return Ok(()) };
        let start = job.out.len();
        let end = (start + FILTER_SLICE).min(self.samples.len());
        job.out.extend_from_slice(&self.samples[start..end]);
        job.chain.process(&mut job.out[start..end]);
        if end < self.samples.len() {
            return Ok(());
        }
        let pcm: Vec<i16> = job.out.iter().map(|&s| to_i16(s)).collect();
        self.filtering = None;
        let mut wav = Cursor::new(Vec::new());
        encode_mono_i16(&pcm, self.rate, &mut wav).map_err(|e| e.to_string())?;
        let url = blob_url(wav.get_ref())?;
        self.switch_to(url);
        Ok(())
    }

    /// Plays another copy from the same moment, keeping play/pause and speed.
    fn switch_to(&mut self, url: String) {
        if url == self.url {
            return;
        }
        let (position, paused, rate) = (self.audio.current_time(), self.audio.paused(), self.audio.playback_rate());
        if self.url != self.original_url {
            let _ = Url::revoke_object_url(&self.url);
        }
        self.audio.set_src(&url);
        self.audio.set_current_time(position);
        self.audio.set_playback_rate(rate);
        if !paused {
            let _ = self.audio.play();
        }
        self.url = url;
    }
}

impl Drop for Loaded {
    fn drop(&mut self) {
        let _ = self.audio.pause();
        let _ = Url::revoke_object_url(&self.url);
        let _ = Url::revoke_object_url(&self.original_url);
    }
}

impl AudioPlayback for WebPlayer {
    /// Also does a slice of filtering work, since the app asks every frame.
    fn is_ready(&self) -> bool {
        self.with_loaded(false, |l| {
            l.step_filtering()?;
            Ok(l.filtering.is_none() && l.audio.ready_state() >= HAVE_METADATA)
        })
    }

    fn take_error(&self) -> Option<String> {
        match &mut *self.state.borrow_mut() {
            State::Failed(message) => message.take(),
            State::Ready(loaded) => loaded.audio.error().map(|e| format!("The audio could not be played (code {}).", e.code())),
            State::Loading => None,
        }
    }

    fn position_ms(&self) -> u64 {
        self.with_loaded(0, |l| Ok(clamp_offset((l.audio.current_time() * MS_PER_S) as u64, l.duration_ms)))
    }

    fn is_playing(&self) -> bool {
        self.with_loaded(false, |l| Ok(!l.audio.paused() && !l.audio.ended()))
    }

    fn set_playing(&self, playing: bool) {
        self.with_loaded((), |l| {
            if !playing {
                return l.audio.pause().map_err(js_err);
            }
            if l.audio.ended() {
                l.audio.set_current_time(0.0);
            }
            // `play()` returns a promise; a refusal shows up as "not playing".
            let _ = l.audio.play().map_err(js_err)?;
            Ok(())
        });
    }

    fn seek_ms(&self, offset_ms: u64) {
        self.with_loaded((), |l| {
            l.audio.set_current_time(clamp_offset(offset_ms, l.duration_ms) as f64 / MS_PER_S);
            Ok(())
        });
    }

    fn set_speed(&self, percent: u16) {
        self.speed.set(percent);
        self.with_loaded((), |l| {
            l.set_speed(percent);
            Ok(())
        });
    }

    fn set_filters(&self, settings: FilterSettings) {
        self.filters.set(settings);
        self.with_loaded((), |l| {
            l.set_filters(settings);
            Ok(())
        });
    }
}
