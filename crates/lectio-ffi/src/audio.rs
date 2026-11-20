//! Recording and playback maths for the mobile apps (ADR-031). The native
//! side owns the audio session and moves samples; Rust turns microphone
//! buffers into the desktop's 16 kHz, 16-bit mono WAV, and turns a WAV back
//! into blocks to play at any speed (the desktop's WSOLA time-stretch).

use std::fs::File;
use std::io::BufWriter;
use std::sync::{Mutex, MutexGuard};

use lectio_core::resample::{SpeechConverter, to_i16};
use lectio_core::stretch::TimeStretcher;

use crate::material::SAMPLE_RATE;
use crate::{LectioError, Result};

fn busy() -> LectioError {
    LectioError::Failed("The audio stopped after an earlier error.".into())
}

fn spec() -> hound::WavSpec {
    hound::WavSpec { channels: 1, sample_rate: SAMPLE_RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int }
}

fn ms(samples: u64) -> u64 {
    samples * 1_000 / u64::from(SAMPLE_RATE)
}

struct Recording {
    writer: Option<hound::WavWriter<BufWriter<File>>>,
    /// The converter for the current input format (it changes when the route
    /// changes, e.g. AirPods connect).
    converter: Option<((u32, u32), SpeechConverter)>,
    written: u64,
}

/// Writes a recording as it happens. Swift calls `push` with each buffer the
/// microphone delivers, from any thread, then `finish`.
#[derive(uniffi::Object)]
pub struct AudioRecorder {
    state: Mutex<Recording>,
}

impl AudioRecorder {
    fn lock(&self) -> Result<MutexGuard<'_, Recording>> {
        self.state.lock().map_err(|_| busy())
    }
}

#[uniffi::export]
impl AudioRecorder {
    /// Starts a new WAV at `path` (its folder is made if needed).
    #[uniffi::constructor]
    pub fn start(path: String) -> Result<std::sync::Arc<Self>> {
        let path = std::path::PathBuf::from(path);
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder).map_err(|e| LectioError::Failed(format!("Can't save the recording: {e}")))?;
        }
        let writer = hound::WavWriter::create(&path, spec()).map_err(|e| LectioError::Failed(format!("Can't save the recording: {e}")))?;
        Ok(std::sync::Arc::new(Self { state: Mutex::new(Recording { writer: Some(writer), converter: None, written: 0 }) }))
    }

    /// Adds interleaved samples (-1..1) as the microphone gave them.
    pub fn push(&self, samples: Vec<f32>, sample_rate: u32, channels: u32) -> Result<()> {
        if sample_rate == 0 || channels == 0 || channels > 32 {
            return Err(LectioError::Failed(format!("Unexpected audio format: {sample_rate} Hz, {channels} channels.")));
        }
        let mut state = self.lock()?;
        let format = (sample_rate, channels);
        let converter = match state.converter.take() {
            Some((f, c)) if f == format => c,
            _ => SpeechConverter::new(channels as u16, sample_rate, SAMPLE_RATE),
        };
        let mut out = Vec::with_capacity(samples.len() / channels as usize);
        let next = converter.process(&samples, &mut out);
        state.converter = Some((format, next));
        let writer = state.writer.as_mut().ok_or_else(|| LectioError::Failed("The recording has already ended.".into()))?;
        for sample in &out {
            writer.write_sample(to_i16(*sample)).map_err(|e| LectioError::Failed(format!("Can't save the recording: {e}")))?;
        }
        state.written += out.len() as u64;
        Ok(())
    }

    /// How much has been recorded so far.
    pub fn duration_ms(&self) -> u64 {
        self.lock().map_or(0, |s| ms(s.written))
    }

    /// Closes the file; returns the recording's length.
    pub fn finish(&self) -> Result<u64> {
        let mut state = self.lock()?;
        if let Some(writer) = state.writer.take() {
            writer.finalize().map_err(|e| LectioError::Failed(format!("Can't save the recording: {e}")))?;
        }
        Ok(ms(state.written))
    }
}

struct Playback {
    samples: Vec<i16>,
    stretcher: TimeStretcher,
    finished: bool,
}

/// Plays a recording: Swift asks for blocks and schedules them.
#[derive(uniffi::Object)]
pub struct AudioPlayer {
    state: Mutex<Playback>,
}

impl AudioPlayer {
    fn lock(&self) -> Option<MutexGuard<'_, Playback>> {
        self.state.lock().ok()
    }
}

#[uniffi::export]
impl AudioPlayer {
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<std::sync::Arc<Self>> {
        let reader = hound::WavReader::open(&path).map_err(|e| LectioError::Failed(format!("Can't open the recording: {e}")))?;
        let spec = reader.spec();
        if spec.sample_rate != SAMPLE_RATE || spec.bits_per_sample != 16 {
            return Err(LectioError::Failed("This recording isn't in Lectio's format (16 kHz, 16-bit).".into()));
        }
        let channels = usize::from(spec.channels.max(1));
        let all: Vec<i16> = reader.into_samples::<i16>().collect::<std::result::Result<_, _>>().map_err(|e| LectioError::Failed(format!("Can't read the recording: {e}")))?;
        let samples = if channels == 1 {
            all
        } else {
            all.chunks(channels).map(|frame| (frame.iter().map(|&s| i32::from(s)).sum::<i32>() / channels as i32) as i16).collect()
        };
        Ok(std::sync::Arc::new(Self { state: Mutex::new(Playback { samples, stretcher: TimeStretcher::new(SAMPLE_RATE), finished: false }) }))
    }

    pub fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    pub fn duration_ms(&self) -> u64 {
        self.lock().map_or(0, |s| ms(s.samples.len() as u64))
    }

    /// 50–200 (percent); values outside are clamped, as on the desktop.
    pub fn set_speed_percent(&self, percent: u16) {
        if let Some(mut s) = self.lock() {
            s.stretcher.set_speed(percent);
        }
    }

    pub fn seek_ms(&self, position_ms: u64) {
        if let Some(mut s) = self.lock() {
            let position = (position_ms * u64::from(SAMPLE_RATE) / 1_000).min(s.samples.len() as u64);
            s.stretcher.seek(position as f64);
            s.finished = false;
        }
    }

    /// Where in the recording playback is.
    pub fn position_ms(&self) -> u64 {
        self.lock().map_or(0, |s| (s.stretcher.position().max(0.0) * 1_000.0 / f64::from(SAMPLE_RATE)).round() as u64)
    }

    /// The next `frames` samples to play (mono, at `sample_rate`); fewer at
    /// the end, and empty once the recording is over.
    pub fn next_block(&self, frames: u32) -> Vec<f32> {
        let Some(mut s) = self.lock() else { return Vec::new() };
        let mut out = vec![0.0; frames as usize];
        let Playback { samples, stretcher, finished } = &mut *s;
        let n = stretcher.render(samples.as_slice(), &mut out);
        out.truncate(n);
        *finished = n == 0;
        out
    }

    pub fn is_finished(&self) -> bool {
        self.lock().is_none_or(|s| s.finished)
    }
}
