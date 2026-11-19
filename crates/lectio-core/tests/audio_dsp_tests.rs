//! Phase 3: playback filters (noise reduction, voice boost, normalise) and
//! pitch-preserving speed with the seek/skip maths.

use std::f32::consts::TAU;

use lectio_core::dsp::{Analysis, Biquad, BiquadKind, FilterChain, FilterSettings, analyse, db, normalise_gain};
use lectio_core::stretch::{SKIP_MS, TimeStretcher, faster, remaining_at_speed_ms, skip, slower};

const RATE: u32 = 16_000;

fn sine(freq: f32, amplitude: f32, seconds: f32) -> Vec<f32> {
    let n = (seconds * RATE as f32) as usize;
    (0..n).map(|i| amplitude * (TAU * freq * i as f32 / RATE as f32).sin()).collect()
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

/// Gain (dB) a filter applies to a steady sine, measured after it settles.
fn gain_db(kind: BiquadKind, freq: f32) -> f32 {
    let input = sine(freq, 0.5, 1.0);
    let mut filter = Biquad::new(kind, RATE);
    let output: Vec<f32> = input.iter().map(|&x| filter.process(x)).collect();
    let settled = input.len() / 2;
    db(rms(&output[settled..]) / rms(&input[settled..]))
}

/// Frequency estimated from upward zero crossings.
fn frequency(samples: &[f32]) -> f32 {
    let crossings = samples.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    crossings as f32 * RATE as f32 / samples.len() as f32
}

#[test]
fn the_high_pass_removes_rumble_and_keeps_speech() {
    let kind = BiquadKind::HighPass { freq: 80.0, q: 0.707 };

    assert!(gain_db(kind, 30.0) < -8.0);
    assert!(gain_db(kind, 1_000.0).abs() < 0.5);
}

#[test]
fn voice_boost_lifts_the_presence_band_only() {
    let kind = BiquadKind::Peaking { freq: 2_500.0, q: 1.0, gain_db: 6.0 };

    assert!((gain_db(kind, 2_500.0) - 6.0).abs() < 0.5);
    assert!(gain_db(kind, 200.0).abs() < 1.0);
}

#[test]
fn analysis_separates_the_noise_floor_from_speech() {
    let mut samples: Vec<f32> = (0..RATE).map(|i| if i % 2 == 0 { 0.001 } else { -0.001 }).collect();
    samples.extend(sine(300.0, 0.5, 1.0));

    let a = analyse(&samples, RATE);

    assert!((a.peak - 0.5).abs() < 0.01);
    assert!(a.noise_floor < 0.002, "noise floor {}", a.noise_floor);
    assert!((a.speech_rms - 0.354).abs() < 0.02, "speech rms {}", a.speech_rms);
}

#[test]
fn normalising_aims_for_minus_20_dbfs_without_clipping_or_huge_gain() {
    let quiet = Analysis { peak: 0.05, speech_rms: 0.01, noise_floor: 0.001 };
    let loud_peaks = Analysis { peak: 0.9, speech_rms: 0.05, noise_floor: 0.001 };
    let already = Analysis { peak: 0.5, speech_rms: 0.1, noise_floor: 0.001 };
    let silent = Analysis { peak: 0.0, speech_rms: 0.0, noise_floor: 0.0 };

    assert!((normalise_gain(&quiet) - 10.0).abs() < 1e-3, "capped at +20 dB");
    assert!((normalise_gain(&loud_peaks) - 0.95 / 0.9).abs() < 1e-3, "limited by the peak");
    assert!((normalise_gain(&already) - 1.0).abs() < 1e-3);
    assert_eq!(normalise_gain(&silent), 1.0);
}

#[test]
fn with_every_filter_off_the_audio_is_unchanged() {
    let input = sine(440.0, 0.3, 0.2);
    let mut output = input.clone();
    let analysis = analyse(&input, RATE);

    FilterChain::new(FilterSettings::default(), &analysis, RATE).process(&mut output);

    assert_eq!(output, input);
}

#[test]
fn noise_reduction_quietens_pauses_but_not_speech() {
    let noise: Vec<f32> = (0..RATE).map(|i| ((i * 7_919 % 1_000) as f32 / 1_000.0 - 0.5) * 0.004).collect();
    let speech = sine(300.0, 0.3, 1.0);
    let mut samples = noise.clone();
    samples.extend(&speech);
    let analysis = analyse(&samples, RATE);
    let settings = FilterSettings { noise_reduction: true, ..FilterSettings::default() };

    let mut output = samples.clone();
    FilterChain::new(settings, &analysis, RATE).process(&mut output);

    let n = RATE as usize;
    let quiet_tail = n / 2..n;
    let speech_tail = n + n / 2..2 * n;
    assert!(db(rms(&output[quiet_tail.clone()]) / rms(&samples[quiet_tail])) < -10.0);
    assert!(db(rms(&output[speech_tail.clone()]) / rms(&samples[speech_tail])).abs() < 1.0);
}

#[test]
fn speed_steps_go_from_half_to_double() {
    assert_eq!(faster(100), 125);
    assert_eq!(faster(200), 200);
    assert_eq!(slower(100), 75);
    assert_eq!(slower(50), 50);
    assert_eq!(faster(110), 125, "an odd value snaps to the next step");
}

#[test]
fn skipping_ten_seconds_stays_inside_the_recording() {
    assert_eq!(SKIP_MS, 10_000);
    assert_eq!(skip(30_000, 10_000, 60_000), 40_000);
    assert_eq!(skip(55_000, 10_000, 60_000), 60_000);
    assert_eq!(skip(4_000, -10_000, 60_000), 0);
}

#[test]
fn time_left_depends_on_the_speed() {
    assert_eq!(remaining_at_speed_ms(0, 60_000, 200), 30_000);
    assert_eq!(remaining_at_speed_ms(30_000, 60_000, 50), 60_000);
    assert_eq!(remaining_at_speed_ms(70_000, 60_000, 100), 0);
}

#[test]
fn at_normal_speed_the_stretcher_passes_audio_through() {
    let source = sine(440.0, 0.5, 0.5);
    let mut stretcher = TimeStretcher::new(RATE);
    let mut out = vec![0.0; 4_000];

    let written = stretcher.render(&source[..], &mut out);

    assert_eq!(written, 4_000);
    assert_eq!(out, source[..4_000]);
    assert_eq!(stretcher.position() as usize, 4_000);
}

#[test]
fn double_speed_halves_the_time_and_keeps_the_pitch() {
    let source = sine(440.0, 0.5, 3.0);
    let mut stretcher = TimeStretcher::new(RATE);
    stretcher.set_speed(200);
    let mut out = vec![0.0; RATE as usize];

    stretcher.render(&source[..], &mut out);

    let advanced_s = stretcher.position() / RATE as f64;
    assert!((advanced_s - 2.0).abs() < 0.05, "advanced {advanced_s} s");
    let f = frequency(&out[1_000..]);
    assert!((f - 440.0).abs() < 15.0, "pitch {f} Hz");
}

#[test]
fn half_speed_doubles_the_time_and_keeps_the_pitch() {
    let source = sine(440.0, 0.5, 3.0);
    let mut stretcher = TimeStretcher::new(RATE);
    stretcher.set_speed(50);
    let mut out = vec![0.0; RATE as usize];

    stretcher.render(&source[..], &mut out);

    let advanced_s = stretcher.position() / RATE as f64;
    assert!((advanced_s - 0.5).abs() < 0.05, "advanced {advanced_s} s");
    let f = frequency(&out[1_000..]);
    assert!((f - 440.0).abs() < 15.0, "pitch {f} Hz");
}

#[test]
fn seeking_moves_the_stretcher_and_the_end_stops_it() {
    let source = sine(440.0, 0.5, 1.0);
    let mut stretcher = TimeStretcher::new(RATE);
    stretcher.set_speed(150);
    stretcher.seek(12_000.0);
    assert_eq!(stretcher.position(), 12_000.0);

    let mut out = vec![0.0; RATE as usize];
    let written = stretcher.render(&source[..], &mut out);

    assert!(written < out.len(), "stops at the end of the recording");
    assert!(stretcher.position() >= 15_000.0);
}

#[test]
fn the_stretcher_reads_16_bit_samples_too() {
    let source: Vec<i16> = (0..8_000).map(|i| ((i % 100) * 100) as i16).collect();
    let mut stretcher = TimeStretcher::new(RATE);
    let mut out = vec![0.0; 100];

    stretcher.render(&source[..], &mut out);

    assert!((out[50] - 5_000.0 / f32::from(i16::MAX)).abs() < 1e-4);
}
