//! Native entry point: command-line flags and window options.
//! Data folder: the OS app-data folder, or `LECTIO_DATA_DIR` if set.

use std::path::PathBuf;
use std::process::ExitCode;

use eframe::egui;

use crate::app::{LectioApp, Startup};
use crate::platform::native::NativePlatform;
use crate::self_test;

const WINDOW_TITLE: &str = "Lectio";
const DEFAULT_WINDOW_SIZE: [f32; 2] = [1120.0, 740.0];
const MIN_WINDOW_SIZE: [f32; 2] = [640.0, 420.0];
const USAGE: &str = "Usage: lectio [--screenshot <file.png> | --self-test-audio <seconds>]";

#[derive(Debug)]
enum Mode {
    Normal,
    Screenshot(PathBuf),
    SelfTestAudio(u64),
}

pub fn run() -> ExitCode {
    let mode = match parse_args(std::env::args().skip(1).collect()) {
        Ok(mode) => mode,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let screenshot_path = match mode {
        Mode::SelfTestAudio(seconds) => return self_test::run(seconds),
        Mode::Screenshot(path) => Some(path),
        Mode::Normal => None,
    };
    let options = native_options(screenshot_path.is_some());
    let result = eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(move |cc| {
            let app = match screenshot_path {
                Some(path) => LectioApp::new(cc, Startup::Sample).with_screenshot(path),
                None => LectioApp::new(cc, Startup::Saved(Ok(Box::new(NativePlatform::open())))),
            };
            Ok(Box::new(app))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Lectio could not start: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: Vec<String>) -> Result<Mode, String> {
    match args.as_slice() {
        [] => Ok(Mode::Normal),
        [flag, path] if flag == "--screenshot" => Ok(Mode::Screenshot(PathBuf::from(path))),
        [flag] if flag == "--screenshot" => Err("--screenshot needs a file name".into()),
        [flag, seconds] if flag == "--self-test-audio" => parse_seconds(seconds).map(Mode::SelfTestAudio),
        [flag] if flag == "--self-test-audio" => Err("--self-test-audio needs a number of seconds".into()),
        [flag, ..] if flag == "--help" || flag == "-h" => {
            Err("Lectio: lecture notes timestamped to the audio.".into())
        }
        other => Err(format!("Unknown arguments: {}", other.join(" "))),
    }
}

fn parse_seconds(text: &str) -> Result<u64, String> {
    let max = self_test::MAX_SECONDS;
    match text.parse::<u64>() {
        Ok(seconds) if (1..=max).contains(&seconds) => Ok(seconds),
        _ => Err(format!("--self-test-audio needs a whole number of seconds from 1 to {max}, got {text:?}")),
    }
}

fn native_options(for_screenshot: bool) -> eframe::NativeOptions {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(WINDOW_TITLE)
        .with_inner_size(DEFAULT_WINDOW_SIZE)
        .with_min_inner_size(MIN_WINDOW_SIZE);
    if for_screenshot {
        // Never take focus away from whatever the user is doing.
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }

    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    #[cfg(target_os = "macos")]
    if for_screenshot {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            builder.with_activate_ignoring_other_apps(false);
        }));
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn self_test_seconds_must_be_between_1_and_the_maximum() {
        assert!(matches!(parse_args(args(&["--self-test-audio", "5"])), Ok(Mode::SelfTestAudio(5))));
        assert!(parse_args(args(&["--self-test-audio", "0"])).is_err());
        assert!(parse_args(args(&["--self-test-audio", "abc"])).is_err());
        assert!(parse_args(args(&["--self-test-audio", "601"])).is_err());
    }

    #[test]
    fn unknown_flags_are_rejected() {
        assert!(parse_args(args(&["--frobnicate"])).is_err());
        assert!(matches!(parse_args(args(&[])), Ok(Mode::Normal)));
    }
}
