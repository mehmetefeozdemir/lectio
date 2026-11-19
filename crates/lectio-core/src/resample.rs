//! Turning microphone audio into Lectio's stored format: 16 kHz, mono,
//! 16-bit (see ADR-005). Pure arithmetic, so it is tested without a mic.

/// Average of one multi-channel frame (e.g. left + right) as a mono sample.
pub fn downmix(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    frame.iter().sum::<f32>() / frame.len() as f32
}

/// Float sample (-1.0..=1.0) → 16-bit integer, clipping anything louder.
pub fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

/// Raw interleaved microphone samples (any channel count, any rate) → 16 kHz
/// mono, fed in chunks of any size. A chunk may end in the middle of a
/// frame (e.g. after the left sample of a stereo pair); the partial frame is
/// carried over, so chunked calls give exactly the same output as one big
/// call. Runs on the writer thread, never on the real-time callback (P-007).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpeechConverter {
    channels: usize,
    frame_sum: f32,
    frame_filled: usize,
    downsampler: Downsampler,
}

impl SpeechConverter {
    pub fn new(channels: u16, input_rate: u32, output_rate: u32) -> Self {
        Self {
            channels: usize::from(channels.max(1)),
            frame_sum: 0.0,
            frame_filled: 0,
            downsampler: Downsampler::new(input_rate, output_rate),
        }
    }

    /// Appends converted samples to `out` and returns the state for the next
    /// chunk. Allocates nothing itself; `out` grows only if it lacks capacity.
    pub fn process(&self, interleaved: &[f32], out: &mut Vec<f32>) -> SpeechConverter {
        let mut state = *self;
        for &sample in interleaved {
            state.frame_sum += sample;
            state.frame_filled += 1;
            if state.frame_filled == state.channels {
                let mono = state.frame_sum / state.channels as f32;
                state.downsampler = state.downsampler.process(&[mono], out);
                state.frame_sum = 0.0;
                state.frame_filled = 0;
            }
        }
        state
    }
}

/// Lowers the sample rate by averaging each group of input samples into one
/// output sample (a "box filter"). It is cheap and good enough for speech.
/// It keeps its position between calls, because audio arrives in chunks of
/// whatever size the sound card picks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Downsampler {
    ratio: f64,
    phase: f64,
    sum: f32,
    count: u32,
}

impl Downsampler {
    pub fn new(input_rate: u32, output_rate: u32) -> Self {
        let ratio = if output_rate == 0 { 1.0 } else { input_rate as f64 / output_rate as f64 };
        Self { ratio: ratio.max(1.0), phase: 0.0, sum: 0.0, count: 0 }
    }

    /// Appends the downsampled audio for `input` to `out` and returns the
    /// downsampler's state for the next chunk.
    pub fn process(&self, input: &[f32], out: &mut Vec<f32>) -> Downsampler {
        let mut state = *self;
        for &sample in input {
            state.sum += sample;
            state.count += 1;
            state.phase += 1.0;
            if state.phase >= state.ratio {
                out.push(state.sum / state.count as f32);
                state.phase -= state.ratio;
                state.sum = 0.0;
                state.count = 0;
            }
        }
        state
    }
}
