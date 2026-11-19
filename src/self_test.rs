//! `lectio --self-test-audio <seconds>`: record from the default microphone
//! with no window, then print what the capture pipeline did. One command for
//! anyone to check live capture (PROBLEMS P-007) on a real machine.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use crate::platform::native::Recorder;
use crate::platform::{RecorderStats, STORE_SAMPLE_RATE};

pub const MAX_SECONDS: u64 = 600;
const PROGRESS_EVERY: Duration = Duration::from_secs(1);

pub fn run(seconds: u64) -> ExitCode {
    let path = std::env::temp_dir().join("lectio-self-test.wav");
    let build = if cfg!(debug_assertions) { "debug" } else { "release" };
    println!("Lectio audio self-test ({build} build): recording {seconds} s from the default microphone...");

    let recorder = match Recorder::start(&path) {
        Ok(recorder) => recorder,
        Err(e) => {
            eprintln!("FAIL: {e}");
            return ExitCode::FAILURE;
        }
    };
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(seconds) {
        std::thread::sleep(PROGRESS_EVERY);
        let s = recorder.stats();
        let elapsed = started.elapsed().as_secs();
        println!("  {elapsed:>3} s: {} frames captured, {} dropped", s.frames_captured, s.frames_dropped);
    }
    let stats = recorder.stats();
    let written = match recorder.finish() {
        Ok(written) => written,
        Err(e) => {
            eprintln!("FAIL: {e}");
            return ExitCode::FAILURE;
        }
    };
    report(&stats, written, seconds, &path);
    verdict(&stats)
}

fn report(s: &RecorderStats, written: u64, seconds: u64, path: &std::path::Path) {
    let expected = seconds * u64::from(s.device_sample_rate);
    let deadline_us = s.max_callback_frames * 1_000_000 / u64::from(s.device_sample_rate.max(1));
    let written_s = written as f64 / f64::from(STORE_SAMPLE_RATE);
    println!();
    println!("Device:            {} ({} Hz, {} ch, {})", s.device_name, s.device_sample_rate, s.channels, s.sample_format);
    println!("Frames captured:   {} (about {expected} expected for {seconds} s)", s.frames_captured);
    println!("Frames dropped:    {}", s.frames_dropped);
    println!("Overruns (xrun):   {}", s.overruns);
    println!("Callbacks:         {}", s.callbacks);
    println!("Slowest callback:  {} us (deadline for {} frames: ~{deadline_us} us)", s.max_callback_us, s.max_callback_frames);
    println!("Written to file:   {written} samples at {STORE_SAMPLE_RATE} Hz = {written_s:.2} s");
    println!("Peak level:        {:.3}", s.peak);
    println!("WAV file:          {}", path.display());
}

fn verdict(s: &RecorderStats) -> ExitCode {
    println!();
    if s.frames_captured == 0 {
        println!("FAIL: no audio arrived from the microphone.");
        return ExitCode::FAILURE;
    }
    if s.peak == 0.0 {
        println!("WARNING: the audio is pure silence. macOS gives silence when microphone access is denied:");
        println!("         System Settings > Privacy & Security > Microphone > allow your terminal app.");
    }
    if s.frames_dropped > 0 || s.overruns > 0 {
        println!("FAIL: audio was lost ({} frames dropped, {} overruns).", s.frames_dropped, s.overruns);
        return ExitCode::FAILURE;
    }
    println!("PASS: no dropped frames and no overruns.");
    ExitCode::SUCCESS
}
