//! The capture pipeline after P-006: the real-time callback only copies raw
//! interleaved samples into a ring buffer; the writer thread converts them.
//! Ring-buffer reads arrive in arbitrary chunks (a chunk can even end in the
//! middle of a stereo frame), so chunked conversion must match one big call.

use lectio_core::realtime::{ring_capacity_samples, whole_frame_samples};
use lectio_core::resample::SpeechConverter;

/// A deterministic "microphone" signal: different on each channel.
fn interleaved(frames: usize, channels: usize) -> Vec<f32> {
    (0..frames * channels)
        .map(|i| {
            let frame = (i / channels) as f32;
            let channel = (i % channels) as f32;
            ((frame * 0.013).sin() * 0.8 + channel * 0.05).clamp(-1.0, 1.0)
        })
        .collect()
}

fn convert_in_chunks(input: &[f32], channels: u16, in_rate: u32, chunk_sizes: &[usize]) -> Vec<f32> {
    let mut out = Vec::new();
    let mut converter = SpeechConverter::new(channels, in_rate, 16_000);
    let mut rest = input;
    let mut sizes = chunk_sizes.iter().cycle();
    while !rest.is_empty() {
        let n = (*sizes.next().unwrap()).min(rest.len());
        converter = converter.process(&rest[..n], &mut out);
        rest = &rest[n..];
    }
    out
}

fn convert_at_once(input: &[f32], channels: u16, in_rate: u32) -> Vec<f32> {
    let mut out = Vec::new();
    SpeechConverter::new(channels, in_rate, 16_000).process(input, &mut out);
    out
}

#[test]
fn chunked_conversion_matches_one_big_call_for_stereo_48k() {
    let input = interleaved(48_000, 2);

    let once = convert_at_once(&input, 2, 48_000);
    let chunked = convert_in_chunks(&input, 2, 48_000, &[1, 3, 512, 7, 1_023, 2]);

    assert_eq!(once.len(), 16_000);
    assert_eq!(chunked, once, "chunked output must be bit-identical");
}

#[test]
fn chunked_conversion_matches_one_big_call_for_44_1k_and_odd_channel_counts() {
    for channels in [1u16, 2, 3, 6] {
        let input = interleaved(44_100, usize::from(channels));
        let once = convert_at_once(&input, channels, 44_100);
        let chunked = convert_in_chunks(&input, channels, 44_100, &[5, 441, 13, 4_096]);
        assert_eq!(chunked, once, "{channels} channels");
    }
}

#[test]
fn every_split_point_gives_the_same_output() {
    // Exhaustive on a small input: split once at every possible index,
    // including in the middle of a stereo frame.
    let input = interleaved(300, 2);
    let once = convert_at_once(&input, 2, 48_000);
    for split in 0..=input.len() {
        let mut out = Vec::new();
        let c = SpeechConverter::new(2, 48_000, 16_000).process(&input[..split], &mut out);
        c.process(&input[split..], &mut out);
        assert_eq!(out, once, "split at {split}");
    }
}

#[test]
fn stereo_frame_split_across_two_chunks_is_averaged_as_one_frame() {
    let mut out = Vec::new();
    let c = SpeechConverter::new(2, 16_000, 16_000).process(&[1.0], &mut out);
    assert!(out.is_empty(), "half a frame produces nothing yet");
    c.process(&[0.0], &mut out);
    assert_eq!(out, vec![0.5]);
}

#[test]
fn mono_16k_input_passes_through_unchanged() {
    let mut out = Vec::new();
    SpeechConverter::new(1, 16_000, 16_000).process(&[0.1, -0.2, 0.3], &mut out);
    assert_eq!(out, vec![0.1, -0.2, 0.3]);
}

#[test]
fn zero_channels_is_treated_as_mono() {
    let mut out = Vec::new();
    SpeechConverter::new(0, 16_000, 16_000).process(&[0.25, 0.5], &mut out);
    assert_eq!(out, vec![0.25, 0.5]);
}

#[test]
fn ring_holds_the_requested_seconds_of_raw_audio() {
    assert_eq!(ring_capacity_samples(48_000, 2, 2), 192_000);
    assert_eq!(ring_capacity_samples(44_100, 1, 2), 88_200);
}

#[test]
fn ring_capacity_is_never_zero() {
    assert!(ring_capacity_samples(0, 0, 0) > 0);
}

#[test]
fn only_whole_frames_are_written_when_the_ring_is_nearly_full() {
    // 5 free slots, 8 incoming stereo samples: write 2 whole frames (4 samples).
    assert_eq!(whole_frame_samples(5, 8, 2), 4);
    // Plenty of room: write everything.
    assert_eq!(whole_frame_samples(1_000, 8, 2), 8);
    // No room for even one frame.
    assert_eq!(whole_frame_samples(1, 8, 2), 0);
    // Mono and a defensive zero-channel case.
    assert_eq!(whole_frame_samples(3, 8, 1), 3);
    assert_eq!(whole_frame_samples(3, 8, 0), 3);
}
