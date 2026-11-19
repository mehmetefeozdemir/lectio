//! Playback speed without changing pitch, and the skip/seek maths
//! (Phase 3, ADR-017).
//!
//! Playing samples faster would raise the voice like a cartoon. Instead
//! `TimeStretcher` uses **WSOLA** (waveform-similarity overlap-add): it
//! copies short overlapping pieces of the recording (40 ms each) into the
//! output, stepping through the source faster or slower than through the
//! output, and picks each piece within ±10 ms of where it should be so that
//! its waveform lines up with the previous piece. Pitch stays the same; only
//! the tempo changes. Buffers are allocated once in `new`, so `render` is
//! safe to call on the audio thread.

/// Skip back / forward step.
pub const SKIP_MS: u64 = 10_000;
/// The speed steps offered, in percent.
pub const SPEEDS: [u16; 7] = [50, 75, 100, 125, 150, 175, 200];
const NORMAL: u16 = 100;
/// One WSOLA piece (40 ms at 16 kHz); pieces overlap by half.
const FRAME_MS: u32 = 40;
/// How far (± ms) a piece may move to line up with the previous one.
const SEARCH_MS: u32 = 10;
/// Compare every second sample when lining up (half the work, same result
/// for speech).
const SEARCH_STRIDE: usize = 2;

pub fn clamp_speed(percent: u16) -> u16 {
    percent.clamp(SPEEDS[0], SPEEDS[SPEEDS.len() - 1])
}

/// The next step up (stays at the top).
pub fn faster(percent: u16) -> u16 {
    SPEEDS.iter().copied().find(|&s| s > percent).unwrap_or(SPEEDS[SPEEDS.len() - 1])
}

/// The next step down (stays at the bottom).
pub fn slower(percent: u16) -> u16 {
    SPEEDS.iter().rev().copied().find(|&s| s < percent).unwrap_or(SPEEDS[0])
}

/// Position after skipping `delta_ms`, kept inside the recording.
pub fn skip(position_ms: u64, delta_ms: i64, duration_ms: u64) -> u64 {
    position_ms.saturating_add_signed(delta_ms).min(duration_ms)
}

/// Listening time left at `percent` speed.
pub fn remaining_at_speed_ms(position_ms: u64, duration_ms: u64, percent: u16) -> u64 {
    duration_ms.saturating_sub(position_ms) * u64::from(NORMAL) / u64::from(clamp_speed(percent))
}

/// Recordings are stored as 16-bit samples; tests use `f32`.
pub trait SampleSource {
    fn count(&self) -> usize;
    fn at(&self, index: usize) -> f32;
}

impl SampleSource for [f32] {
    fn count(&self) -> usize {
        self.len()
    }
    fn at(&self, index: usize) -> f32 {
        self[index]
    }
}

impl SampleSource for [i16] {
    fn count(&self) -> usize {
        self.len()
    }
    fn at(&self, index: usize) -> f32 {
        f32::from(self[index]) / f32::from(i16::MAX)
    }
}

pub struct TimeStretcher {
    speed: u16,
    /// Where in the source (samples) the next piece should come from.
    nominal: f64,
    /// Where the previous piece actually came from.
    previous: Option<usize>,
    frame: usize,
    hop: usize,
    search: usize,
    window: Vec<f32>,
    /// Second half of the previous piece, waiting to be overlapped.
    tail: Vec<f32>,
    /// Output made but not yet handed out.
    pending: Vec<f32>,
    pending_read: usize,
}

impl TimeStretcher {
    pub fn new(sample_rate: u32) -> Self {
        let frame = ((sample_rate * FRAME_MS / 1_000) as usize).max(4) & !1;
        let hop = frame / 2;
        // A periodic Hann window: two halves overlapping by `hop` add up to 1.
        let window = (0..frame)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / frame as f32).cos())
            .collect();
        Self {
            speed: NORMAL,
            nominal: 0.0,
            previous: None,
            frame,
            hop,
            search: (sample_rate * SEARCH_MS / 1_000) as usize,
            window,
            tail: vec![0.0; hop],
            pending: vec![0.0; hop],
            pending_read: hop,
        }
    }

    pub fn speed(&self) -> u16 {
        self.speed
    }

    pub fn set_speed(&mut self, percent: u16) {
        let percent = clamp_speed(percent);
        if percent != self.speed {
            let position = self.position();
            self.speed = percent;
            self.seek(position);
        }
    }

    /// The source position (samples) of what is being heard now.
    pub fn position(&self) -> f64 {
        let queued = (self.hop - self.pending_read) as f64 * f64::from(self.speed) / f64::from(NORMAL);
        (self.nominal - queued).max(0.0)
    }

    pub fn seek(&mut self, position: f64) {
        self.nominal = position.max(0.0);
        self.previous = None;
        self.tail.fill(0.0);
        self.pending_read = self.hop;
    }

    /// Fills `out` with audio from `source`; returns how many samples were
    /// written (fewer than `out.len()` at the end of the recording).
    pub fn render<S: SampleSource + ?Sized>(&mut self, source: &S, out: &mut [f32]) -> usize {
        if self.speed == NORMAL {
            return self.copy(source, out);
        }
        let mut written = 0;
        while written < out.len() {
            if self.pending_read == self.hop && !self.next_piece(source) {
                self.nominal = source.count() as f64;
                break;
            }
            let n = (self.hop - self.pending_read).min(out.len() - written);
            out[written..written + n].copy_from_slice(&self.pending[self.pending_read..self.pending_read + n]);
            self.pending_read += n;
            written += n;
        }
        written
    }

    fn copy<S: SampleSource + ?Sized>(&mut self, source: &S, out: &mut [f32]) -> usize {
        let start = self.nominal as usize;
        let n = out.len().min(source.count().saturating_sub(start));
        for (i, sample) in out[..n].iter_mut().enumerate() {
            *sample = source.at(start + i);
        }
        self.nominal = (start + n) as f64;
        n
    }

    /// Overlap-adds the next piece; false at the end of the source.
    fn next_piece<S: SampleSource + ?Sized>(&mut self, source: &S) -> bool {
        let nominal = self.nominal.round() as usize;
        let start = match self.previous {
            Some(previous) => self.best_match(source, nominal, previous + self.hop),
            None => nominal,
        };
        if start + self.frame > source.count() {
            return false;
        }
        for i in 0..self.hop {
            self.pending[i] = self.tail[i] + self.window[i] * source.at(start + i);
            self.tail[i] = self.window[i + self.hop] * source.at(start + self.hop + i);
        }
        self.pending_read = 0;
        self.previous = Some(start);
        self.nominal += self.hop as f64 * f64::from(self.speed) / f64::from(NORMAL);
        true
    }

    /// The start near `nominal` whose first half looks most like the natural
    /// continuation of the previous piece (largest cross-correlation).
    fn best_match<S: SampleSource + ?Sized>(&self, source: &S, nominal: usize, natural: usize) -> usize {
        let len = source.count();
        if natural + self.hop > len || len < self.frame {
            return nominal;
        }
        let lowest = nominal.saturating_sub(self.search);
        let highest = (nominal + self.search).min(len.saturating_sub(self.frame));
        let mut best = (nominal.min(highest.max(lowest)), f32::MIN);
        for candidate in lowest..=highest.max(lowest) {
            let score: f32 = (0..self.hop)
                .step_by(SEARCH_STRIDE)
                .map(|i| source.at(candidate + i) * source.at(natural + i))
                .sum();
            if score > best.1 {
                best = (candidate, score);
            }
        }
        best.0
    }
}
