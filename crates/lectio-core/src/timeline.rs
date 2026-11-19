//! Timestamp and seek logic.
//!
//! Notes store their position as milliseconds (`offset_ms`); the audio player
//! works in samples; the timeline widget works in fractions of its width.
//! These pure functions convert between the three, so seeking is testable
//! without a speaker or a window.

/// Milliseconds → sample index. Uses u128 in the middle so a long lecture at
/// a high sample rate can't overflow.
pub fn ms_to_samples(offset_ms: u64, sample_rate: u32) -> u64 {
    (offset_ms as u128 * sample_rate as u128 / 1_000) as u64
}

/// Sample index → milliseconds, rounded down, so a note taken "now" never
/// points past the audio that has actually been recorded.
pub fn samples_to_ms(samples: u64, sample_rate: u32) -> u64 {
    if sample_rate == 0 {
        return 0;
    }
    (samples as u128 * 1_000 / sample_rate as u128) as u64
}

/// Keeps a seek target inside the recording.
pub fn clamp_offset(offset_ms: u64, duration_ms: u64) -> u64 {
    offset_ms.min(duration_ms)
}

/// Where a marker sits on the timeline, from 0.0 (start) to 1.0 (end).
pub fn offset_to_fraction(offset_ms: u64, duration_ms: u64) -> f32 {
    if duration_ms == 0 {
        return 0.0;
    }
    (clamp_offset(offset_ms, duration_ms) as f64 / duration_ms as f64) as f32
}

/// The offset under a click at `fraction` of the timeline's width.
pub fn fraction_to_offset(fraction: f32, duration_ms: u64) -> u64 {
    if !fraction.is_finite() {
        return 0;
    }
    let f = fraction.clamp(0.0, 1.0) as f64;
    (f * duration_ms as f64).round() as u64
}

/// Index of the note being "played": the last note at or before the
/// playhead. `offsets` must be sorted (the model keeps notes sorted).
pub fn active_note_index(offsets: &[u64], position_ms: u64) -> Option<usize> {
    offsets.partition_point(|&o| o <= position_ms).checked_sub(1)
}

/// `83250` → `"1:23"`, `3661000` → `"1:01:01"`.
pub fn format_offset(offset_ms: u64) -> String {
    let total_s = offset_ms / 1_000;
    let (h, m, s) = (total_s / 3_600, (total_s / 60) % 60, total_s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}
