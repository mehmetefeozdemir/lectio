//! Plays a session's WAV file through the default speaker, with seeking,
//! pitch-preserving speed and the audio filters (ADR-006, ADR-017).
//!
//! The whole file is loaded into memory as 16-bit samples (16 kHz mono is
//! about 115 MB for 1 hour). The output callback pulls audio at the file's
//! rate from `TimeStretcher` (speed) and `FilterChain` (filters) into a small
//! buffer, then converts to the speaker's rate by linear interpolation.
//! Everything the callback uses is allocated before playback starts.

use std::path::Path;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use lectio_core::dsp::{Analysis, FilterChain, FilterSettings, analyse};
use lectio_core::stretch::TimeStretcher;
use lectio_core::timeline::{clamp_offset, ms_to_samples, samples_to_ms};

use crate::platform::wav::decode_mono_i16;
use crate::platform::{AudioError, AudioPlayback, err};

/// Samples (at the file's rate) made per refill of the callback's buffer.
const BLOCK: usize = 512;

struct PlayState {
    playing: bool,
    stretcher: TimeStretcher,
    filters: FilterChain,
    analysis: Analysis,
    sample_rate: u32,
    /// `block[0]` is the last sample of the previous block, so
    /// interpolation works across block boundaries.
    block: Vec<f32>,
    block_len: usize,
    /// Read position inside `block` (fractional).
    cursor: f64,
}

impl PlayState {
    fn reset_buffer(&mut self) {
        self.block.fill(0.0);
        self.block_len = 1;
        self.cursor = 0.0;
        self.filters.reset();
    }

    /// Next output sample; `step` = file rate ÷ speaker rate.
    fn next_sample(&mut self, samples: &[i16], step: f64) -> f32 {
        if !self.playing {
            return 0.0;
        }
        while self.cursor + 1.0 >= self.block_len as f64 {
            if !self.refill(samples) {
                self.playing = false;
                return 0.0;
            }
        }
        let index = self.cursor as usize;
        let t = (self.cursor - index as f64) as f32;
        let (a, b) = (self.block[index], self.block[index + 1]);
        self.cursor += step;
        a + (b - a) * t
    }

    fn refill(&mut self, samples: &[i16]) -> bool {
        let last = self.block[self.block_len - 1];
        self.block[0] = last;
        self.cursor -= (self.block_len - 1) as f64;
        let written = self.stretcher.render(samples, &mut self.block[1..]);
        self.filters.process(&mut self.block[1..=written]);
        self.block_len = written + 1;
        written > 0
    }
}

pub struct Player {
    _stream: cpal::Stream,
    state: Arc<Mutex<PlayState>>,
    sample_rate: u32,
    len_samples: u64,
}

impl Player {
    pub fn open(path: &Path) -> Result<Player, AudioError> {
        let file = std::fs::File::open(path).map_err(|e| err("Could not open the recording", e))?;
        let (samples, sample_rate) = decode_mono_i16(std::io::BufReader::new(file))?;
        let floats: Vec<f32> = samples.iter().map(|&s| f32::from(s) / f32::from(i16::MAX)).collect();
        let analysis = analyse(&floats, sample_rate);
        drop(floats);
        let samples: Arc<[i16]> = samples.into();
        let len_samples = samples.len() as u64;
        let state = Arc::new(Mutex::new(PlayState {
            playing: false,
            stretcher: TimeStretcher::new(sample_rate),
            filters: FilterChain::new(FilterSettings::default(), &analysis, sample_rate),
            analysis,
            sample_rate,
            block: vec![0.0; BLOCK + 1],
            block_len: 1,
            cursor: 0.0,
        }));

        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| AudioError("No speaker or headphones were found.".into()))?;
        let supported = device.default_output_config().map_err(|e| err("Speaker setup failed", e))?;
        let config: cpal::StreamConfig = supported.config();
        let source = Source { samples, state: Arc::clone(&state), sample_rate };

        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, &config, source),
            SampleFormat::I16 => build::<i16>(&device, &config, source),
            SampleFormat::I32 => build::<i32>(&device, &config, source),
            other => Err(AudioError(format!("Unsupported speaker format: {other}"))),
        }?;
        stream.play().map_err(|e| err("Could not start playback", e))?;
        Ok(Player { _stream: stream, state, sample_rate, len_samples })
    }

    fn duration_ms(&self) -> u64 {
        samples_to_ms(self.len_samples, self.sample_rate)
    }

    fn with_state<R>(&self, f: impl FnOnce(&mut PlayState) -> R) -> R {
        // A poisoned lock only means the audio thread panicked mid-update;
        // the state is still usable.
        let mut guard = self.state.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut guard)
    }
}

impl AudioPlayback for Player {
    fn is_ready(&self) -> bool {
        true
    }
    fn take_error(&self) -> Option<String> {
        None
    }
    fn position_ms(&self) -> u64 {
        self.with_state(|s| samples_to_ms(s.stretcher.position() as u64, self.sample_rate))
    }
    fn is_playing(&self) -> bool {
        self.with_state(|s| s.playing)
    }
    fn set_playing(&self, playing: bool) {
        if playing && self.position_ms() >= self.duration_ms() {
            self.seek_ms(0);
        }
        self.with_state(|s| s.playing = playing);
    }
    /// Jumps to `offset_ms`, clamped to the recording (tested in `lectio_core::timeline`).
    fn seek_ms(&self, offset_ms: u64) {
        let target = ms_to_samples(clamp_offset(offset_ms, self.duration_ms()), self.sample_rate);
        self.with_state(|s| {
            s.stretcher.seek(target as f64);
            s.reset_buffer();
        });
    }
    fn set_speed(&self, percent: u16) {
        self.with_state(|s| {
            s.stretcher.set_speed(percent);
            s.reset_buffer();
        });
    }
    fn set_filters(&self, settings: FilterSettings) {
        self.with_state(|s| {
            s.filters = FilterChain::new(settings, &s.analysis, s.sample_rate);
            s.reset_buffer();
        });
    }
}

struct Source {
    samples: Arc<[i16]>,
    state: Arc<Mutex<PlayState>>,
    sample_rate: u32,
}

fn build<T>(device: &cpal::Device, config: &cpal::StreamConfig, source: Source) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = usize::from(config.channels.max(1));
    let step = f64::from(source.sample_rate) / f64::from(config.sample_rate.max(1));

    device
        .build_output_stream::<T, _, _>(
            *config,
            // REAL-TIME THREAD: never waits for the lock (plays silence instead).
            move |out: &mut [T], _| {
                let Ok(mut state) = source.state.try_lock() else {
                    out.fill(T::from_sample(0.0));
                    return;
                };
                for frame in out.chunks_mut(channels) {
                    let value = state.next_sample(&source.samples, step);
                    frame.fill(T::from_sample(value));
                }
            },
            |e| eprintln!("lectio: playback stream error: {e}"),
            None,
        )
        .map_err(|e| err("Could not open the speaker", e))
}
