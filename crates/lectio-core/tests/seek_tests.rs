//! Timestamp and seek logic: converting between milliseconds, samples and
//! timeline positions, and choosing what to highlight during playback.

use lectio_core::timeline::{
    active_note_index, clamp_offset, format_offset, fraction_to_offset, ms_to_samples,
    offset_to_fraction, samples_to_ms,
};

#[test]
fn ms_and_samples_convert_both_ways_at_16_khz() {
    assert_eq!(ms_to_samples(1_000, 16_000), 16_000);
    assert_eq!(ms_to_samples(83_250, 16_000), 1_332_000);
    assert_eq!(samples_to_ms(1_332_000, 16_000), 83_250);
}

#[test]
fn samples_to_ms_rounds_down_so_a_note_never_points_past_the_audio() {
    assert_eq!(samples_to_ms(15, 16_000), 0);
    assert_eq!(samples_to_ms(16, 16_000), 1);
}

#[test]
fn conversions_survive_a_three_hour_lecture_without_overflow() {
    let three_hours_ms = 3 * 60 * 60 * 1_000;
    let samples = ms_to_samples(three_hours_ms, 48_000);
    assert_eq!(samples_to_ms(samples, 48_000), three_hours_ms);
}

#[test]
fn zero_sample_rate_is_treated_as_no_audio() {
    assert_eq!(samples_to_ms(1_000, 0), 0);
    assert_eq!(ms_to_samples(1_000, 0), 0);
}

#[test]
fn seek_target_is_clamped_to_the_recording() {
    assert_eq!(clamp_offset(5_000, 10_000), 5_000);
    assert_eq!(clamp_offset(50_000, 10_000), 10_000);
}

#[test]
fn marker_position_is_a_fraction_of_the_timeline() {
    assert_eq!(offset_to_fraction(0, 10_000), 0.0);
    assert_eq!(offset_to_fraction(2_500, 10_000), 0.25);
    assert_eq!(offset_to_fraction(10_000, 10_000), 1.0);
    assert_eq!(offset_to_fraction(20_000, 10_000), 1.0, "markers never leave the bar");
    assert_eq!(offset_to_fraction(500, 0), 0.0, "empty recording does not divide by zero");
}

#[test]
fn clicking_the_timeline_maps_back_to_an_offset() {
    assert_eq!(fraction_to_offset(0.5, 10_000), 5_000);
    assert_eq!(fraction_to_offset(-0.2, 10_000), 0);
    assert_eq!(fraction_to_offset(1.7, 10_000), 10_000);
    assert_eq!(fraction_to_offset(f32::NAN, 10_000), 0);
}

#[test]
fn active_note_is_the_last_one_at_or_before_the_playhead() {
    let offsets = [1_000, 5_000, 9_000];

    assert_eq!(active_note_index(&offsets, 0), None);
    assert_eq!(active_note_index(&offsets, 1_000), Some(0));
    assert_eq!(active_note_index(&offsets, 8_999), Some(1));
    assert_eq!(active_note_index(&offsets, 60_000), Some(2));
    assert_eq!(active_note_index(&[], 60_000), None);
}

#[test]
fn offsets_are_shown_as_minutes_and_seconds() {
    assert_eq!(format_offset(0), "0:00");
    assert_eq!(format_offset(83_250), "1:23");
    assert_eq!(format_offset(3_600_000 + 61_000), "1:01:01");
}
