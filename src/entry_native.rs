//! Native entry point: command-line flags and window options.
//! Data folder: the OS app-data folder, or `LECTIO_DATA_DIR` if set.

use std::path::PathBuf;
use std::process::ExitCode;

use eframe::egui;

use lectio_core::prefs::SpeechModel;

use crate::action::Screen;
use crate::app::{LectioApp, Startup};
use crate::platform::native::NativePlatform;
use crate::self_test;

const WINDOW_TITLE: &str = "Lectio";
const DEFAULT_WINDOW_SIZE: [f32; 2] = [1120.0, 740.0];
const MIN_WINDOW_SIZE: [f32; 2] = [640.0, 420.0];
const USAGE: &str = "Usage: lectio [--screenshot <file.png> | --self-test-audio <seconds> | \
                     --transcribe <file.wav> [--model tiny|base|small] [--language <code>]]";

#[derive(Debug)]
enum Mode {
    Normal,
    Screenshot(PathBuf),
    SelfTestAudio(u64),
    Transcribe { wav: PathBuf, model: SpeechModel, language: Option<String> },
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
        Mode::Transcribe { wav, model, language } => return transcribe(&wav, model, language.as_deref()),
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
            // Test aid: LECTIO_START=home|trash|collections opens on that screen (screenshots).
            let app = match std::env::var("LECTIO_START").as_deref() {
                Ok("home") => app.with_screen(Screen::Home),
                Ok("trash") => app.with_screen(Screen::Trash),
                Ok("collections") => app.with_screen(Screen::Collections),
                Ok("search") => app.with_search(&std::env::var("LECTIO_SEARCH").unwrap_or_default()),
                _ => app,
            };
            // Test aid: LECTIO_PANELS=feed,audio shows only those workspace panels.
            let app = match std::env::var("LECTIO_PANELS") {
                Ok(panels) => app.with_panels(&panels),
                Err(_) => app,
            };
            // Test aid: LECTIO_THEME=dark|light forces a theme (screenshots).
            match std::env::var("LECTIO_THEME").as_deref() {
                Ok("dark") => cc.egui_ctx.set_theme(egui::ThemePreference::Dark),
                Ok("light") => cc.egui_ctx.set_theme(egui::ThemePreference::Light),
                _ => {}
            }
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
        [flag, wav, rest @ ..] if flag == "--transcribe" => parse_transcribe(wav, rest),
        [flag] if flag == "--transcribe" => Err("--transcribe needs a WAV file (16 kHz mono)".into()),
        [flag, ..] if flag == "--help" || flag == "-h" => {
            Err("Lectio: lecture notes timestamped to the audio.".into())
        }
        other => Err(format!("Unknown arguments: {}", other.join(" "))),
    }
}

fn parse_transcribe(wav: &str, rest: &[String]) -> Result<Mode, String> {
    let mut model = SpeechModel::Tiny;
    let mut language = None;
    for pair in rest.chunks(2) {
        match pair {
            [flag, value] if flag == "--model" => {
                model = SpeechModel::ALL
                    .into_iter()
                    .find(|m| m.name() == value)
                    .ok_or("--model must be tiny, base or small")?;
            }
            [flag, value] if flag == "--language" => language = Some(value.clone()),
            other => return Err(format!("Unknown arguments: {}", other.join(" "))),
        }
    }
    Ok(Mode::Transcribe { wav: PathBuf::from(wav), model, language })
}

/// Transcribes a file and prints the real-time factor.
fn transcribe(wav: &std::path::Path, model: SpeechModel, language: Option<&str>) -> ExitCode {
    let models = crate::platform::native::models_dir();
    match crate::platform::native::transcribe_cli(&models, wav, model, language) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Transcription failed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parse_seconds(text: &str) -> Result<u64, String> {
    let max = self_test::MAX_SECONDS;
    match text.parse::<u64>() {
        Ok(seconds) if (1..=max).contains(&seconds) => Ok(seconds),
        _ => Err(format!("--self-test-audio needs a whole number of seconds from 1 to {max}, got {text:?}")),
    }
}

/// `LECTIO_WINDOW_SIZE=1280x800` overrides the start size (layout checks).
fn window_size() -> [f32; 2] {
    let parsed = std::env::var("LECTIO_WINDOW_SIZE").ok().and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some([w.trim().parse().ok()?, h.trim().parse().ok()?])
    });
    parsed.unwrap_or(DEFAULT_WINDOW_SIZE)
}

fn native_options(for_screenshot: bool) -> eframe::NativeOptions {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(WINDOW_TITLE)
        .with_inner_size(window_size())
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
