//! The log-mel spectrogram Whisper reads: 80 frequency bands × 3000 frames
//! (10 ms each) for one 30-second window.
//!
//! A *mel spectrogram* measures how much energy the audio has in each band
//! of frequencies, with bands spaced the way human hearing is (narrow at low
//! pitches, wide at high ones). This follows OpenAI's reference code:
//! a 400-sample Hann window every 160 samples (centred, reflect-padded),
//! power spectrum, Slaney-style mel filters, log10, clamp to 8 below the
//! loudest value, then scale to roughly −1..1. Written here (instead of
//! using candle's helper) because candle's version starts threads, which a
//! browser build can't do.

use std::sync::Arc;

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

pub const SAMPLE_RATE: u32 = 16_000;
pub const N_FFT: usize = 400;
pub const HOP: usize = 160;
/// Samples in one 30-second window.
pub const WINDOW_SAMPLES: usize = 30 * SAMPLE_RATE as usize;
/// Frames in one window.
pub const FRAMES: usize = WINDOW_SAMPLES / HOP;
const F_SP: f64 = 200.0 / 3.0;
const MIN_LOG_HZ: f64 = 1_000.0;

fn log_step() -> f64 {
    6.4f64.ln() / 27.0
}

/// Slaney mel scale: linear below 1 kHz, logarithmic above.
fn hz_to_mel(hz: f64) -> f64 {
    if hz >= MIN_LOG_HZ { MIN_LOG_HZ / F_SP + (hz / MIN_LOG_HZ).ln() / log_step() } else { hz / F_SP }
}

fn mel_to_hz(mel: f64) -> f64 {
    let min_log_mel = MIN_LOG_HZ / F_SP;
    if mel >= min_log_mel { MIN_LOG_HZ * (log_step() * (mel - min_log_mel)).exp() } else { mel * F_SP }
}

/// Triangular mel filters, `n_mels` rows × (N_FFT/2 + 1) columns, each
/// normalised by its width ("slaney" area normalisation, like librosa).
pub fn mel_filters(n_mels: usize) -> Vec<f32> {
    let bins = N_FFT / 2 + 1;
    let nyquist = f64::from(SAMPLE_RATE) / 2.0;
    let fft_freqs: Vec<f64> = (0..bins).map(|i| i as f64 * nyquist / (bins - 1) as f64).collect();
    let top = hz_to_mel(nyquist);
    let edges: Vec<f64> = (0..n_mels + 2).map(|i| mel_to_hz(top * i as f64 / (n_mels + 1) as f64)).collect();
    let mut filters = vec![0.0_f32; n_mels * bins];
    for m in 0..n_mels {
        let (lower, centre, upper) = (edges[m], edges[m + 1], edges[m + 2]);
        let norm = 2.0 / (upper - lower);
        for (k, &f) in fft_freqs.iter().enumerate() {
            let rising = (f - lower) / (centre - lower);
            let falling = (upper - f) / (upper - centre);
            filters[m * bins + k] = (rising.min(falling).max(0.0) * norm) as f32;
        }
    }
    filters
}

/// Computes spectrograms; keeps the FFT plan and filters between windows.
pub struct MelSpectrogram {
    n_mels: usize,
    filters: Vec<f32>,
    window: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
}

impl MelSpectrogram {
    pub fn new(n_mels: usize) -> Self {
        let window = (0..N_FFT).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N_FFT as f32).cos()).collect();
        Self { n_mels, filters: mel_filters(n_mels), window, fft: FftPlanner::new().plan_fft_forward(N_FFT) }
    }

    pub fn n_mels(&self) -> usize {
        self.n_mels
    }

    /// `n_mels × FRAMES` values (row-major, one row per band) for up to 30 s
    /// of 16 kHz audio; shorter audio is padded with silence.
    pub fn compute(&self, pcm: &[f32]) -> Vec<f32> {
        let mut padded = vec![0.0_f32; WINDOW_SAMPLES];
        let n = pcm.len().min(WINDOW_SAMPLES);
        padded[..n].copy_from_slice(&pcm[..n]);
        let centred = reflect_pad(&padded, N_FFT / 2);
        let bins = N_FFT / 2 + 1;
        let mut out = vec![0.0_f32; self.n_mels * FRAMES];
        let mut buffer = vec![Complex32::new(0.0, 0.0); N_FFT];
        let mut power = vec![0.0_f32; bins];
        for frame in 0..FRAMES {
            let start = frame * HOP;
            for (i, slot) in buffer.iter_mut().enumerate() {
                *slot = Complex32::new(centred[start + i] * self.window[i], 0.0);
            }
            self.fft.process(&mut buffer);
            for (k, p) in power.iter_mut().enumerate() {
                *p = buffer[k].norm_sqr();
            }
            for m in 0..self.n_mels {
                let row = &self.filters[m * bins..(m + 1) * bins];
                let energy: f32 = row.iter().zip(&power).map(|(w, p)| w * p).sum();
                out[m * FRAMES + frame] = energy.max(1e-10).log10();
            }
        }
        let loudest = out.iter().copied().fold(f32::MIN, f32::max);
        for v in &mut out {
            *v = (v.max(loudest - 8.0) + 4.0) / 4.0;
        }
        out
    }
}

/// Mirrors `pad` samples onto each end (like numpy's "reflect").
fn reflect_pad(samples: &[f32], pad: usize) -> Vec<f32> {
    let n = samples.len();
    let mut out = Vec::with_capacity(n + 2 * pad);
    out.extend((1..=pad).rev().map(|i| samples[i.min(n - 1)]));
    out.extend_from_slice(samples);
    out.extend((1..=pad).map(|i| samples[n.saturating_sub(1 + i)]));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mel_scale_round_trips_and_bends_at_1_khz() {
        for hz in [0.0, 300.0, 1_000.0, 4_000.0, 8_000.0] {
            assert!((mel_to_hz(hz_to_mel(hz)) - hz).abs() < 1e-6);
        }
        assert!((hz_to_mel(1_000.0) - 15.0).abs() < 1e-9);
    }

    #[test]
    fn filters_are_triangles_that_cover_the_spectrum_in_order() {
        let filters = mel_filters(80);
        let bins = N_FFT / 2 + 1;
        assert_eq!(filters.len(), 80 * bins);
        assert!(filters.iter().all(|&w| w >= 0.0));
        let peak_bin =
            |m: usize| (0..bins).max_by(|&a, &b| filters[m * bins + a].total_cmp(&filters[m * bins + b])).unwrap();
        assert!(peak_bin(10) < peak_bin(40) && peak_bin(40) < peak_bin(79));
    }

    #[test]
    fn a_tone_lights_up_the_band_around_its_frequency() {
        let tone: Vec<f32> =
            (0..SAMPLE_RATE as usize).map(|i| (std::f32::consts::TAU * 1_000.0 * i as f32 / 16_000.0).sin() * 0.5).collect();
        let mel = MelSpectrogram::new(80).compute(&tone);
        let frame = 50;
        let loudest = (0..80).max_by(|&a, &b| mel[a * FRAMES + frame].total_cmp(&mel[b * FRAMES + frame])).unwrap();
        let filters = mel_filters(80);
        let bins = N_FFT / 2 + 1;
        let one_khz_bin = 1_000 * N_FFT / SAMPLE_RATE as usize;
        assert!(filters[loudest * bins + one_khz_bin] > 0.0, "band {loudest} should cover 1 kHz");
    }

    #[test]
    fn values_are_scaled_into_whispers_range() {
        let mel = MelSpectrogram::new(80).compute(&[0.0; 1_000]);
        assert_eq!(mel.len(), 80 * FRAMES);
        assert!(mel.iter().all(|v| v.is_finite() && (-1.5..=1.5).contains(v)));
    }
}
