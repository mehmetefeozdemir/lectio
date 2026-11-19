//! Converting microphone audio (often 44.1 or 48 kHz, stereo) to the 16 kHz
//! mono speech format Lectio stores.

use lectio_core::resample::{Downsampler, downmix, to_i16};

#[test]
fn downsampling_48k_to_16k_keeps_one_sample_in_three() {
    let mut out = Vec::new();
    let ds = Downsampler::new(48_000, 16_000);
    let ds = ds.process(&vec![0.5; 48_000], &mut out);
    let _ = ds;
    assert_eq!(out.len(), 16_000);
    assert!(out.iter().all(|s| (*s - 0.5).abs() < 1e-6));
}

#[test]
fn downsampling_is_continuous_across_callback_chunks() {
    // 44.1 kHz audio arriving in uneven chunks must still give 16 000 samples per second.
    let mut out = Vec::new();
    let mut ds = Downsampler::new(44_100, 16_000);
    for chunk in [441usize, 1_000, 512, 42_147] {
        ds = ds.process(&vec![0.0; chunk], &mut out);
    }
    assert!((out.len() as i64 - 16_000).abs() <= 1, "got {}", out.len());
}

#[test]
fn same_rate_passes_samples_through() {
    let mut out = Vec::new();
    Downsampler::new(16_000, 16_000).process(&[0.1, 0.2, 0.3], &mut out);
    assert_eq!(out, vec![0.1, 0.2, 0.3]);
}

#[test]
fn stereo_frames_are_averaged_to_mono() {
    assert_eq!(downmix(&[1.0, 0.0]), 0.5);
    assert_eq!(downmix(&[]), 0.0);
}

#[test]
fn float_samples_are_clipped_into_16_bit_range() {
    assert_eq!(to_i16(0.0), 0);
    assert_eq!(to_i16(1.0), i16::MAX);
    assert_eq!(to_i16(4.0), i16::MAX);
    assert_eq!(to_i16(-4.0), -i16::MAX);
}
