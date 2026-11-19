//! Arithmetic used by the real-time audio callback (ADR-009).
//!
//! The callback runs on the OS audio thread with a hard deadline, so it may
//! not allocate, lock or block. These helpers are plain integer maths, which
//! keeps them safe to call there and easy to test here.

/// How many raw samples the ring buffer between the audio callback and the
/// writer thread should hold: `seconds` of audio at the device's format.
/// Never zero, so a strange device report can't make a useless buffer.
pub fn ring_capacity_samples(sample_rate: u32, channels: u16, seconds: u32) -> usize {
    let samples = sample_rate as usize * usize::from(channels) * seconds as usize;
    samples.max(1)
}

/// How many of `incoming` interleaved samples to write when the ring has
/// `free` slots: as many as fit, rounded down to whole frames, so a stereo
/// pair is never split by a drop. The rest is counted as dropped.
pub fn whole_frame_samples(free: usize, incoming: usize, channels: u16) -> usize {
    let channels = usize::from(channels.max(1));
    let fit = free.min(incoming);
    fit - fit % channels
}
