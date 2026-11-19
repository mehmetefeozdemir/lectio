//! Plays a session's WAV file through the default speaker, with seeking.
//!
//! The whole file is loaded into memory as 16-bit samples (16 kHz mono is
//! about 115 MB for 1 hour). The output callback reads from a shared
//! position, so seeking is just "set the position" (ADR-005).

use std::path::Path;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use lectio_core::timeline::{clamp_offset, ms_to_samples, samples_to_ms};

use crate::platform::wav::decode_mono_i16;
use crate::platform::{AudioError, AudioPlayback, err};

struct PlayState {
    /// Position in source samples (fractional, because the speaker's rate
    /// usually differs from the file's).
    position: f64,
    playing: bool,
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
        let samples: Arc<[i16]> = samples.into();
        let len_samples = samples.len() as u64;
        let state = Arc::new(Mutex::new(PlayState { position: 0.0, playing: false }));

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

    pub fn duration_ms(&self) -> u64 {
        samples_to_ms(self.len_samples, self.sample_rate)
    }

    pub fn position_ms(&self) -> u64 {
        self.with_state(|s| samples_to_ms(s.position as u64, self.sample_rate))
    }

    pub fn is_playing(&self) -> bool {
        self.with_state(|s| s.playing)
    }

    pub fn set_playing(&self, playing: bool) {
        let at_end = self.position_ms() >= self.duration_ms();
        if playing && at_end {
            self.seek_ms(0);
        }
        self.with_state(|s| s.playing = playing);
    }

    /// Jumps to `offset_ms`, clamped to the recording (tested in
    /// `lectio_core::timeline`).
    pub fn seek_ms(&self, offset_ms: u64) {
        let target = ms_to_samples(clamp_offset(offset_ms, self.duration_ms()), self.sample_rate);
        self.with_state(|s| s.position = target as f64);
    }

    fn with_state<R>(&self, f: impl FnOnce(&mut PlayState) -> R) -> R {
        // A poisoned lock only means the audio thread panicked mid-update;
        // the position is still a usable number.
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
        Player::position_ms(self)
    }
    fn is_playing(&self) -> bool {
        Player::is_playing(self)
    }
    fn set_playing(&self, playing: bool) {
        Player::set_playing(self, playing);
    }
    fn seek_ms(&self, offset_ms: u64) {
        Player::seek_ms(self, offset_ms);
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
    let step = source.sample_rate as f64 / config.sample_rate.max(1) as f64;

    device
        .build_output_stream::<T, _, _>(
            *config,
            move |out: &mut [T], _| {
                let Ok(mut state) = source.state.try_lock() else {
                    out.fill(T::from_sample(0.0));
                    return;
                };
                for frame in out.chunks_mut(channels) {
                    let value = next_sample(&source.samples, &mut state, step);
                    frame.fill(T::from_sample(value));
                }
            },
            |e| eprintln!("lectio: playback stream error: {e}"),
            None,
        )
        .map_err(|e| err("Could not open the speaker", e))
}

/// Linear interpolation between neighbouring samples; stops at the end.
fn next_sample(samples: &[i16], state: &mut PlayState, step: f64) -> f32 {
    if !state.playing {
        return 0.0;
    }
    let index = state.position as usize;
    if index + 1 >= samples.len() {
        state.playing = false;
        state.position = samples.len() as f64;
        return 0.0;
    }
    let t = (state.position - index as f64) as f32;
    let a = f32::from(samples[index]) / f32::from(i16::MAX);
    let b = f32::from(samples[index + 1]) / f32::from(i16::MAX);
    state.position += step;
    a + (b - a) * t
}
