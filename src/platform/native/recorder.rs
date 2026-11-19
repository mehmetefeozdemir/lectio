//! Records the microphone, the computer's own audio, or both, straight to a
//! WAV file (ADR-009, ADR-019).
//!
//! Three parties are involved:
//! - each **audio callback** runs on an OS real-time audio thread, which has
//!   a hard deadline (about 10 ms per buffer). It only copies raw interleaved
//!   samples into its own pre-allocated lock-free ring buffer and bumps
//!   atomic counters: **no heap allocation, no locks, no blocking**;
//! - the **writer thread** drains the rings, converts each input to 16 kHz
//!   mono (`lectio_core::resample::SpeechConverter`), mixes them, writes the
//!   WAV, keeps the last 30 s for live captions, and bumps `samples_written`;
//! - the **UI** reads `samples_written` to know the current recording offset,
//!   and `stats()` to warn if any audio was dropped.
//!
//! Computer audio ("loopback") comes from cpal on every desktop: on macOS a
//! Core Audio process tap (macOS 14.2+), on Windows WASAPI loopback, on Linux
//! the default output's PulseAudio/PipeWire "monitor" source.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, Sample, SampleFormat, SizedSample, SupportedStreamConfig};
use lectio_core::realtime::{ring_capacity_samples, whole_frame_samples};
use lectio_core::resample::{SpeechConverter, to_i16};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::platform::{AudioCapture, AudioError, CaptureSource, RecorderStats, STORE_SAMPLE_RATE, err};

/// Seconds of raw audio each ring can hold if the writer thread stalls
/// (e.g. a slow disk). Allocated once, before the streams start.
const RING_SECONDS: u32 = 2;
/// How long the writer sleeps when the rings are empty.
const WRITER_POLL: Duration = Duration::from_millis(10);
/// Audio kept for live captions (16 kHz samples).
const RECENT_SAMPLES: usize = 30 * STORE_SAMPLE_RATE as usize;
/// A second input may lag the first by at most this much before its oldest
/// audio is dropped, so the two never drift apart audibly.
const MAX_LAG_SAMPLES: usize = STORE_SAMPLE_RATE as usize / 2;

/// Counters shared between the callbacks, the writer and the UI. Everything
/// a callback touches is an atomic, which never blocks.
#[derive(Default)]
struct Shared {
    paused: AtomicBool,
    frames_captured: AtomicU64,
    samples_dropped: AtomicU64,
    overruns: AtomicU64,
    callbacks: AtomicU64,
    max_callback_ns: AtomicU64,
    max_callback_frames: AtomicU64,
    samples_written: AtomicU64,
    peak_bits: AtomicU32,
    /// Loudest sample of the most recent chunk (for the level meter).
    level_bits: AtomicU32,
    /// Only for rare non-real-time errors (device unplugged); written with
    /// `try_lock`, so even that path can't block.
    last_error: Mutex<Option<String>>,
}

struct DeviceInfo {
    name: String,
    sample_rate: u32,
    channels: u16,
    sample_format: String,
}

/// One input on its way to the writer.
struct Input {
    ring: Consumer<f32>,
    converter: SpeechConverter,
    pending: VecDeque<f32>,
}

type Recent = Arc<Mutex<VecDeque<f32>>>;

pub struct Recorder {
    streams: Vec<cpal::Stream>,
    shared: Arc<Shared>,
    writer: Option<JoinHandle<Result<(), AudioError>>>,
    device: DeviceInfo,
    recent: Recent,
}

/// The device and settings for the computer's own output.
fn system_audio() -> Result<(cpal::Device, SupportedStreamConfig), AudioError> {
    let unavailable = |e: &dyn std::fmt::Display| {
        AudioError(format!("Computer audio can't be recorded: {e}. {}", system_audio_help()))
    };
    #[cfg(target_os = "linux")]
    {
        let host = cpal::host_from_id(cpal::HostId::PulseAudio).map_err(|e| unavailable(&e))?;
        let devices = host.input_devices().map_err(|e| unavailable(&e))?;
        let monitor = devices
            .into_iter()
            .find(|d| d.description().is_ok_and(|desc| desc.name().ends_with(".monitor")))
            .ok_or_else(|| unavailable(&"no monitor source was found"))?;
        let config = monitor.default_input_config().map_err(|e| unavailable(&e))?;
        Ok((monitor, config))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let device = cpal::default_host().default_output_device().ok_or_else(|| unavailable(&"no speaker found"))?;
        let config = device.default_output_config().map_err(|e| unavailable(&e))?;
        Ok((device, config))
    }
}

pub fn system_audio_help() -> &'static str {
    if cfg!(target_os = "macos") {
        "It needs macOS 14.2 or later, and permission under System Settings › Privacy & Security › \
         Screen & System Audio Recording for the app you started Lectio from."
    } else if cfg!(target_os = "windows") {
        "Windows records what the default speaker plays (WASAPI loopback)."
    } else {
        "Lectio records the default output's monitor source through PulseAudio or PipeWire."
    }
}

fn microphone() -> Result<(cpal::Device, SupportedStreamConfig), AudioError> {
    let device = cpal::default_host().default_input_device().ok_or_else(|| AudioError("No microphone was found".into()))?;
    let config = device.default_input_config().map_err(|e| err("Microphone setup failed", e))?;
    Ok((device, config))
}

impl Recorder {
    /// Opens the chosen inputs and starts writing to `path`.
    pub fn start(path: &Path, source: CaptureSource) -> Result<Recorder, AudioError> {
        let devices = match source {
            CaptureSource::Microphone => vec![microphone()?],
            CaptureSource::Computer => vec![system_audio()?],
            CaptureSource::Both => vec![microphone()?, system_audio()?],
        };
        let wav = create_wav(path)?;
        let shared = Arc::new(Shared::default());
        let recent: Recent = Arc::new(Mutex::new(VecDeque::with_capacity(RECENT_SAMPLES)));
        let mut inputs = Vec::new();
        let mut streams = Vec::new();
        let mut first_info = None;
        for (device, supported) in &devices {
            let config: cpal::StreamConfig = supported.config();
            let capacity = ring_capacity_samples(config.sample_rate, config.channels, RING_SECONDS);
            let (producer, consumer) = RingBuffer::<f32>::new(capacity);
            inputs.push(Input {
                ring: consumer,
                converter: SpeechConverter::new(config.channels, config.sample_rate, STORE_SAMPLE_RATE),
                pending: VecDeque::new(),
            });
            let stream = match supported.sample_format() {
                SampleFormat::F32 => build::<f32>(device, &config, producer, &shared),
                SampleFormat::I16 => build::<i16>(device, &config, producer, &shared),
                SampleFormat::I32 => build::<i32>(device, &config, producer, &shared),
                SampleFormat::U16 => build::<u16>(device, &config, producer, &shared),
                other => Err(AudioError(format!("Unsupported audio format: {other}"))),
            }?;
            streams.push(stream);
            first_info.get_or_insert_with(|| DeviceInfo {
                name: device.description().map(|d| d.name().to_owned()).unwrap_or_else(|_| "unknown".into()),
                sample_rate: config.sample_rate,
                channels: config.channels,
                sample_format: supported.sample_format().to_string(),
            });
        }
        let writer = spawn_writer(wav, inputs, Arc::clone(&shared), Arc::clone(&recent));
        for stream in &streams {
            stream.play().map_err(|e| err("Could not start recording", e))?;
        }
        let device = first_info.ok_or_else(|| AudioError("No audio input".into()))?;
        Ok(Recorder { streams, shared, writer: Some(writer), device, recent })
    }

    pub fn set_paused(&self, paused: bool) {
        self.shared.paused.store(paused, Ordering::SeqCst);
    }

    /// Samples at 16 kHz that are safely in the file so far.
    pub fn samples_written(&self) -> u64 {
        self.shared.samples_written.load(Ordering::SeqCst)
    }

    pub fn stats(&self) -> RecorderStats {
        let s = &self.shared;
        let channels = u64::from(self.device.channels.max(1));
        RecorderStats {
            device_name: self.device.name.clone(),
            device_sample_rate: self.device.sample_rate,
            channels: self.device.channels,
            sample_format: self.device.sample_format.clone(),
            frames_captured: s.frames_captured.load(Ordering::Relaxed),
            frames_dropped: s.samples_dropped.load(Ordering::Relaxed) / channels,
            overruns: s.overruns.load(Ordering::Relaxed),
            callbacks: s.callbacks.load(Ordering::Relaxed),
            max_callback_us: s.max_callback_ns.load(Ordering::Relaxed) / 1_000,
            max_callback_frames: s.max_callback_frames.load(Ordering::Relaxed),
            samples_written: s.samples_written.load(Ordering::Relaxed),
            peak: f32::from_bits(s.peak_bits.load(Ordering::Relaxed)),
        }
    }

    /// The most recent non-real-time problem (e.g. the device went away).
    pub fn take_error(&self) -> Option<String> {
        self.shared.last_error.lock().ok().and_then(|mut slot| slot.take())
    }

    /// Stops the inputs, waits for the file to be finished, and returns the
    /// total number of 16 kHz samples written.
    pub fn finish(mut self) -> Result<u64, AudioError> {
        // Dropping a stream drops its callback and ring producer; the writer
        // sees every ring "abandoned", drains what's left and finalizes.
        self.streams.clear();
        if let Some(handle) = self.writer.take() {
            handle.join().map_err(|_| AudioError("The audio writer crashed".into()))??;
        }
        Ok(self.samples_written())
    }
}

impl AudioCapture for Recorder {
    fn set_paused(&self, paused: bool) {
        Recorder::set_paused(self, paused);
    }
    fn samples_written(&self) -> u64 {
        Recorder::samples_written(self)
    }
    fn level(&self) -> f32 {
        f32::from_bits(self.shared.level_bits.load(Ordering::Relaxed))
    }
    fn stats(&self) -> RecorderStats {
        Recorder::stats(self)
    }
    fn take_error(&self) -> Option<String> {
        Recorder::take_error(self)
    }
    fn recent_audio(&self, ms: u64) -> Vec<f32> {
        let wanted = (ms * u64::from(STORE_SAMPLE_RATE) / 1_000) as usize;
        let Ok(recent) = self.recent.lock() else { return Vec::new() };
        recent.iter().skip(recent.len().saturating_sub(wanted)).copied().collect()
    }
    fn finish(self: Box<Self>) -> Result<u64, AudioError> {
        Recorder::finish(*self)
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.streams.clear();
        if let Some(handle) = self.writer.take() {
            let _ = handle.join();
        }
    }
}

type WavWriter = hound::WavWriter<std::io::BufWriter<std::fs::File>>;

fn create_wav(path: &Path) -> Result<WavWriter, AudioError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| err("Could not create the recordings folder", e))?;
    }
    let spec = crate::platform::wav::mono_spec(STORE_SAMPLE_RATE);
    hound::WavWriter::create(path, spec).map_err(|e| err("Could not create the audio file", e))
}

/// The writer thread: not real-time, so it may allocate, convert and do I/O.
fn spawn_writer(
    mut wav: WavWriter,
    mut inputs: Vec<Input>,
    shared: Arc<Shared>,
    recent: Recent,
) -> JoinHandle<Result<(), AudioError>> {
    std::thread::spawn(move || {
        let mut converted = Vec::with_capacity(STORE_SAMPLE_RATE as usize);
        let mut out = Vec::with_capacity(STORE_SAMPLE_RATE as usize);
        let mut peak = 0.0f32;
        loop {
            let mut finished = true;
            for input in &mut inputs {
                // Read "abandoned" first: once it's true no more samples can
                // arrive, so an empty ring after that really means "done".
                let abandoned = input.ring.is_abandoned();
                let available = input.ring.slots();
                finished &= abandoned && available == 0;
                if available == 0 {
                    continue;
                }
                let chunk = input.ring.read_chunk(available).map_err(|e| err("Reading audio failed", e))?;
                let (first, second) = chunk.as_slices();
                converted.clear();
                input.converter = input.converter.process(first, &mut converted).process(second, &mut converted);
                chunk.commit_all();
                input.pending.extend(converted.iter().copied());
            }
            mix(&mut inputs, &mut out);
            if out.is_empty() {
                if finished {
                    break;
                }
                std::thread::sleep(WRITER_POLL);
                continue;
            }
            let mut chunk_peak = 0.0f32;
            for &sample in &out {
                chunk_peak = chunk_peak.max(sample.abs());
                wav.write_sample(to_i16(sample)).map_err(|e| err("Writing audio failed", e))?;
            }
            if let Ok(mut recent) = recent.lock() {
                recent.extend(out.iter().copied());
                let excess = recent.len().saturating_sub(RECENT_SAMPLES);
                recent.drain(..excess);
            }
            peak = peak.max(chunk_peak);
            shared.peak_bits.store(peak.to_bits(), Ordering::Relaxed);
            shared.level_bits.store(chunk_peak.to_bits(), Ordering::Relaxed);
            shared.samples_written.fetch_add(out.len() as u64, Ordering::SeqCst);
        }
        wav.finalize().map_err(|e| err("Finishing the audio file failed", e))
    })
}

/// The first input sets the pace; the others are added sample by sample
/// (silence if they are behind) and trimmed if they run too far ahead.
fn mix(inputs: &mut [Input], out: &mut Vec<f32>) {
    out.clear();
    let Some((lead, rest)) = inputs.split_first_mut() else { return };
    out.extend(lead.pending.drain(..));
    for other in rest {
        for sample in out.iter_mut() {
            *sample = (*sample + other.pending.pop_front().unwrap_or(0.0)).clamp(-1.0, 1.0);
        }
        let excess = other.pending.len().saturating_sub(MAX_LAG_SAMPLES);
        other.pending.drain(..excess);
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut ring: Producer<f32>,
    shared: &Arc<Shared>,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let channels = config.channels;
    let frame_len = u64::from(channels.max(1));
    let rt = Arc::clone(shared);
    let on_error = Arc::clone(shared);

    device
        .build_input_stream::<T, _, _>(
            *config,
            // REAL-TIME THREAD: no allocation, no locks, no blocking.
            move |data: &[T], _| {
                let started = Instant::now();
                rt.callbacks.fetch_add(1, Ordering::Relaxed);
                if rt.paused.load(Ordering::Relaxed) {
                    return;
                }
                let n = whole_frame_samples(ring.slots(), data.len(), channels);
                if let Ok(chunk) = ring.write_chunk_uninit(n) {
                    chunk.fill_from_iter(data[..n].iter().map(|&s| f32::from_sample(s)));
                }
                let dropped = (data.len() - n) as u64;
                if dropped > 0 {
                    rt.samples_dropped.fetch_add(dropped, Ordering::Relaxed);
                }
                rt.frames_captured.fetch_add(n as u64 / frame_len, Ordering::Relaxed);
                rt.max_callback_frames.fetch_max(data.len() as u64 / frame_len, Ordering::Relaxed);
                rt.max_callback_ns.fetch_max(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
            },
            move |e: cpal::Error| {
                // macOS reports overloads from the real-time thread too, so
                // this path is an atomic increment only.
                if e.kind() == ErrorKind::Xrun {
                    on_error.overruns.fetch_add(1, Ordering::Relaxed);
                } else if let Ok(mut slot) = on_error.last_error.try_lock() {
                    *slot = Some(format!("Recording problem: {e}"));
                }
            },
            None,
        )
        .map_err(|e| err("Could not open the audio input", e))
}
