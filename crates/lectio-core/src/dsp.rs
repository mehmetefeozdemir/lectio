//! Audio filters for playback and for the cleaned copy (Phase 3, ADR-017):
//! noise reduction, voice boost and normalising. All of it is plain maths on
//! `f32` samples, so it runs the same on the audio thread, in the browser and
//! in tests.
//!
//! - **Noise reduction** is a *downward expander*: when the level falls below
//!   a threshold just above the room's noise floor (the hiss between words),
//!   the audio is turned down further, so pauses get quiet while speech is
//!   left alone. It does not remove noise *under* speech (that would need
//!   spectral processing).
//! - **Voice boost** is two *biquads* (standard second-order filters from the
//!   "Audio EQ Cookbook"): a high-pass at 80 Hz removes rumble, and a +6 dB
//!   peak at 2.5 kHz lifts the consonants that make speech clear.
//! - **Normalise** applies one gain so speech sits at about −20 dBFS, limited
//!   so the loudest peak doesn't clip and the gain stays at most +20 dB.

use serde::{Deserialize, Serialize};

use crate::{Library, ModelError, SessionId};
use crate::model::{Recording, Session};

/// Which filters are on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FilterSettings {
    pub noise_reduction: bool,
    pub voice_boost: bool,
    pub normalise: bool,
}

impl FilterSettings {
    pub fn any(&self) -> bool {
        self.noise_reduction || self.voice_boost || self.normalise
    }
}

/// Target speech level for "Normalise": −20 dBFS.
const TARGET_RMS: f32 = 0.1;
/// Normalising never adds more than +20 dB.
const MAX_GAIN: f32 = 10.0;
/// …and never pushes the loudest peak above this.
const MAX_PEAK: f32 = 0.95;
/// Levels are measured in 20 ms frames.
const FRAME_MS: u32 = 20;
/// The noise floor is the level of the quietest 10% of frames.
const NOISE_PERCENTILE: f32 = 0.1;
/// Frames this many times louder than the noise floor count as speech.
const SPEECH_OVER_NOISE: f32 = 4.0;
/// The expander starts this many times above the noise floor…
const GATE_OVER_NOISE: f32 = 4.0;
/// …turns down with this exponent (an expansion ratio of 1:4)…
const GATE_EXPONENT: i32 = 3;
/// …but never by more than 30 dB.
const GATE_FLOOR: f32 = 0.031_6;
const GATE_ATTACK_MS: f32 = 2.0;
const GATE_RELEASE_MS: f32 = 60.0;
const HIGH_PASS_HZ: f32 = 80.0;
const PRESENCE_HZ: f32 = 2_500.0;
const PRESENCE_DB: f32 = 6.0;
const BUTTERWORTH_Q: f32 = 0.707;

/// A ratio in decibels (20·log10).
pub fn db(ratio: f32) -> f32 {
    20.0 * ratio.max(1e-9).log10()
}

/// Levels measured once over the whole recording.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Analysis {
    pub peak: f32,
    /// RMS of the frames that are clearly louder than the noise floor.
    pub speech_rms: f32,
    /// RMS of the quietest frames: the room's background noise.
    pub noise_floor: f32,
}

pub fn analyse(samples: &[f32], sample_rate: u32) -> Analysis {
    let frame = (sample_rate * FRAME_MS / 1_000).max(1) as usize;
    let mut frames: Vec<f32> = samples
        .chunks(frame)
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32).sqrt())
        .collect();
    if frames.is_empty() {
        return Analysis::default();
    }
    let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    frames.sort_by(f32::total_cmp);
    let noise_floor = frames[((frames.len() as f32 * NOISE_PERCENTILE) as usize).min(frames.len() - 1)];
    let speech: Vec<f32> = frames.iter().copied().filter(|&r| r > (noise_floor * SPEECH_OVER_NOISE).max(1e-4)).collect();
    let speech_rms =
        if speech.is_empty() { 0.0 } else { (speech.iter().map(|r| r * r).sum::<f32>() / speech.len() as f32).sqrt() };
    Analysis { peak, speech_rms, noise_floor }
}

/// The single gain "Normalise" applies.
pub fn normalise_gain(analysis: &Analysis) -> f32 {
    if analysis.speech_rms <= 0.0 || analysis.peak <= 0.0 {
        return 1.0;
    }
    (TARGET_RMS / analysis.speech_rms).min(MAX_GAIN).min(MAX_PEAK / analysis.peak)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BiquadKind {
    HighPass { freq: f32, q: f32 },
    Peaking { freq: f32, q: f32, gain_db: f32 },
}

/// A second-order IIR filter (transposed direct form II, `f64` inside so
/// low cut-offs stay stable).
#[derive(Debug, Clone)]
pub struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    pub fn new(kind: BiquadKind, sample_rate: u32) -> Self {
        let (freq, q) = match kind {
            BiquadKind::HighPass { freq, q } | BiquadKind::Peaking { freq, q, .. } => (freq, q),
        };
        let w0 = std::f64::consts::TAU * f64::from(freq) / f64::from(sample_rate.max(1));
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * f64::from(q));
        let (b, a0, a) = match kind {
            BiquadKind::HighPass { .. } => {
                let b = [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0];
                (b, 1.0 + alpha, [-2.0 * cos, 1.0 - alpha])
            }
            BiquadKind::Peaking { gain_db, .. } => {
                let amp = 10f64.powf(f64::from(gain_db) / 40.0);
                let b = [1.0 + alpha * amp, -2.0 * cos, 1.0 - alpha * amp];
                (b, 1.0 + alpha / amp, [-2.0 * cos, 1.0 - alpha / amp])
            }
        };
        Self { b: b.map(|x| x / a0), a: a.map(|x| x / a0), z: [0.0; 2] }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let x = f64::from(x);
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y as f32
    }

    pub fn reset(&mut self) {
        self.z = [0.0; 2];
    }
}

/// The downward expander behind "Noise reduction".
#[derive(Debug, Clone)]
struct Expander {
    threshold: f32,
    envelope: f32,
    gain: f32,
    attack: f32,
    release: f32,
}

impl Expander {
    fn new(noise_floor: f32, sample_rate: u32) -> Self {
        let coefficient = |ms: f32| (-1.0 / (ms / 1_000.0 * sample_rate as f32)).exp();
        Self {
            threshold: (noise_floor * GATE_OVER_NOISE).max(1e-4),
            envelope: 0.0,
            gain: 1.0,
            attack: coefficient(GATE_ATTACK_MS),
            release: coefficient(GATE_RELEASE_MS),
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let level = x.abs();
        let k = if level > self.envelope { self.attack } else { self.release };
        self.envelope = k * self.envelope + (1.0 - k) * level;
        let target = if self.envelope >= self.threshold {
            1.0
        } else {
            (self.envelope / self.threshold).powi(GATE_EXPONENT).max(GATE_FLOOR)
        };
        let k = if target > self.gain { self.attack } else { self.release };
        self.gain = k * self.gain + (1.0 - k) * target;
        x * self.gain
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
        self.gain = 1.0;
    }
}

/// The filters chosen in `FilterSettings`, ready to run. `process` never
/// allocates, so it is safe on the audio thread.
#[derive(Debug, Clone)]
pub struct FilterChain {
    enabled: bool,
    expander: Option<Expander>,
    high_pass: Option<Biquad>,
    presence: Option<Biquad>,
    gain: f32,
}

impl FilterChain {
    pub fn new(settings: FilterSettings, analysis: &Analysis, sample_rate: u32) -> Self {
        let boost = settings.voice_boost;
        Self {
            enabled: settings.any(),
            expander: settings.noise_reduction.then(|| Expander::new(analysis.noise_floor, sample_rate)),
            high_pass: boost.then(|| Biquad::new(BiquadKind::HighPass { freq: HIGH_PASS_HZ, q: BUTTERWORTH_Q }, sample_rate)),
            presence: boost.then(|| {
                Biquad::new(BiquadKind::Peaking { freq: PRESENCE_HZ, q: 1.0, gain_db: PRESENCE_DB }, sample_rate)
            }),
            gain: if settings.normalise { normalise_gain(analysis) } else { 1.0 },
        }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        if !self.enabled {
            return;
        }
        for sample in samples {
            let mut x = *sample;
            if let Some(e) = &mut self.expander {
                x = e.process(x);
            }
            if let Some(f) = &mut self.high_pass {
                x = f.process(x);
            }
            if let Some(f) = &mut self.presence {
                x = f.process(x);
            }
            *sample = (x * self.gain).clamp(-1.0, 1.0);
        }
    }

    /// Forget filter state (after a seek, so old audio doesn't ring on).
    pub fn reset(&mut self) {
        if let Some(e) = &mut self.expander {
            e.reset();
        }
        for f in [&mut self.high_pass, &mut self.presence].into_iter().flatten() {
            f.reset();
        }
    }
}

/// A cleaned copy's name: letters, digits, `-` and `_`, ending in `.wav`
/// (no folders, no `..`).
fn valid_audio_name(name: &str) -> bool {
    let stem = name.strip_suffix(".wav").unwrap_or("");
    !stem.is_empty() && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl Library {
    /// Remembers (or forgets, with `None`) the session's cleaned audio copy.
    pub fn with_cleaned_copy(&self, session: SessionId, file_name: Option<&str>) -> Result<Library, ModelError> {
        if file_name.is_some_and(|n| !valid_audio_name(n)) {
            return Err(ModelError::InvalidBlobKey);
        }
        self.map_session(session, |s| {
            let recording = s.recording.as_ref().ok_or(ModelError::NoRecording)?;
            let recording = Recording { cleaned_file: file_name.map(str::to_owned), ..recording.clone() };
            Ok(Session { recording: Some(recording), ..s.clone() })
        })
    }
}
