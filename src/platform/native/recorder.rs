//! Records the default microphone straight to a WAV file (ADR-008).
//!
//! Three parties are involved:
//! - the **audio callback** runs on the OS real-time audio thread, which has a
//!   hard deadline (about 10 ms per buffer). It only copies raw interleaved
//!   samples into a pre-allocated lock-free ring buffer and bumps atomic
//!   counters: **no heap allocation, no locks, no blocking**;
//! - the **writer thread** drains the ring, downmixes and downsamples to
//!   16 kHz mono (`lectio_core::resample::SpeechConverter`), writes the WAV,
//!   then bumps `samples_written`;
//! - the **UI** reads `samples_written` to know the current recording offset,
//!   and `stats()` to warn if any audio was dropped.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, Sample, SampleFormat, SizedSample};
use lectio_core::realtime::{ring_capacity_samples, whole_frame_samples};
use lectio_core::resample::{SpeechConverter, to_i16};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::platform::{AudioCapture, AudioError, RecorderStats, STORE_SAMPLE_RATE, err};

/// Seconds of raw audio the ring can hold if the writer thread stalls
/// (e.g. a slow disk). Allocated once, before the stream starts.
const RING_SECONDS: u32 = 2;
/// How long the writer sleeps when the ring is empty.
const WRITER_POLL: Duration = Duration::from_millis(10);

/// Counters shared between the callback, the writer and the UI. Everything
/// the callback touches is an atomic, which never blocks.
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

pub struct Recorder {
    stream: Option<cpal::Stream>,
    shared: Arc<Shared>,
    writer: Option<JoinHandle<Result<(), AudioError>>>,
    device: DeviceInfo,
}

impl Recorder {
    /// Opens the default microphone and starts writing to `path`.
    pub fn start(path: &Path) -> Result<Recorder, AudioError> {
        let device = cpal::default_host()
            .default_input_device()
            .ok_or_else(|| AudioError("No microphone was found".into()))?;
        let supported = device.default_input_config().map_err(|e| err("Microphone setup failed", e))?;
        let config: cpal::StreamConfig = supported.config();
        let info = DeviceInfo {
            name: device.description().map(|d| d.name().to_owned()).unwrap_or_else(|_| "unknown".into()),
            sample_rate: config.sample_rate,
            channels: config.channels,
            sample_format: supported.sample_format().to_string(),
        };

        let wav = create_wav(path)?;
        let shared = Arc::new(Shared::default());
        let capacity = ring_capacity_samples(config.sample_rate, config.channels, RING_SECONDS);
        let (producer, consumer) = RingBuffer::<f32>::new(capacity);
        let converter = SpeechConverter::new(config.channels, config.sample_rate, STORE_SAMPLE_RATE);
        let writer = spawn_writer(wav, consumer, converter, Arc::clone(&shared));

        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, &config, producer, &shared),
            SampleFormat::I16 => build::<i16>(&device, &config, producer, &shared),
            SampleFormat::I32 => build::<i32>(&device, &config, producer, &shared),
            SampleFormat::U16 => build::<u16>(&device, &config, producer, &shared),
            other => Err(AudioError(format!("Unsupported microphone format: {other}"))),
        }?;
        stream.play().map_err(|e| err("Could not start the microphone", e))?;

        Ok(Recorder { stream: Some(stream), shared, writer: Some(writer), device: info })
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

    /// Stops the microphone, waits for the file to be finished, and returns
    /// the total number of 16 kHz samples written.
    pub fn finish(mut self) -> Result<u64, AudioError> {
        // Dropping the stream drops the callback and its ring producer; the
        // writer sees the ring "abandoned", drains what's left and finalizes.
        drop(self.stream.take());
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
    fn stats(&self) -> RecorderStats {
        Recorder::stats(self)
    }
    fn take_error(&self) -> Option<String> {
        Recorder::take_error(self)
    }
    fn finish(self: Box<Self>) -> Result<u64, AudioError> {
        Recorder::finish(*self)
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        drop(self.stream.take());
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
    mut ring: Consumer<f32>,
    mut converter: SpeechConverter,
    shared: Arc<Shared>,
) -> JoinHandle<Result<(), AudioError>> {
    std::thread::spawn(move || {
        let mut out = Vec::with_capacity(STORE_SAMPLE_RATE as usize);
        let mut peak = 0.0f32;
        loop {
            // Read "abandoned" first: once it's true no more samples can
            // arrive, so an empty ring after that really means "done".
            let abandoned = ring.is_abandoned();
            let available = ring.slots();
            if available == 0 {
                if abandoned {
                    break;
                }
                std::thread::sleep(WRITER_POLL);
                continue;
            }
            let chunk = ring.read_chunk(available).map_err(|e| err("Reading audio failed", e))?;
            let (first, second) = chunk.as_slices();
            out.clear();
            converter = converter.process(first, &mut out).process(second, &mut out);
            chunk.commit_all();

            for &sample in &out {
                peak = peak.max(sample.abs());
                wav.write_sample(to_i16(sample)).map_err(|e| err("Writing audio failed", e))?;
            }
            shared.peak_bits.store(peak.to_bits(), Ordering::Relaxed);
            shared.samples_written.fetch_add(out.len() as u64, Ordering::SeqCst);
        }
        wav.finalize().map_err(|e| err("Finishing the audio file failed", e))
    })
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
                    *slot = Some(format!("Microphone problem: {e}"));
                }
            },
            None,
        )
        .map_err(|e| err("Could not open the microphone", e))
}
