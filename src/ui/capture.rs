//! Capture view: record a lecture and take notes on it.

use eframe::egui::{self, Key, Modifiers};
use lectio_core::Session;
use lectio_core::clock::{RecorderCommand, RecorderPhase};
use lectio_core::timeline::format_offset;

use super::{View, composer, notes, shortcut_hint, theme};
use crate::action::Action;

pub fn show(ui: &mut egui::Ui, view: &View, session: &Session, phase: RecorderPhase, actions: &mut Vec<Action>) {
    super::playback::title(ui, view, session);
    egui::Frame::group(ui.style()).inner_margin(12.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        recorder_panel(ui, view, phase, actions);
    });
    ui.add_space(6.0);
    composer::show(ui, view, actions);
    ui.separator();
    notes::show(ui, view, session, notes::Mode::Capture, actions);
}

fn recorder_panel(ui: &mut egui::Ui, view: &View, phase: RecorderPhase, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        status_light(ui, phase);
        let status = match phase {
            RecorderPhase::Idle => "Ready to record",
            RecorderPhase::Recording => "Recording",
            RecorderPhase::Paused => "Paused",
            RecorderPhase::Stopped => "Saving…",
        };
        ui.label(egui::RichText::new(status).strong());
        ui.label(egui::RichText::new(format_offset(view.offset_ms)).monospace().size(28.0))
            .on_hover_text("Time recorded so far. Pauses don't count.");
    });
    ui.horizontal(|ui| {
        let main_key = shortcut_hint(ui.ctx(), Modifiers::COMMAND, Key::R);
        let stop_key = shortcut_hint(ui.ctx(), Modifiers::COMMAND | Modifiers::SHIFT, Key::R);
        let (label, command) = match phase {
            RecorderPhase::Idle => ("Start recording", Some(RecorderCommand::Start)),
            RecorderPhase::Recording => ("Pause", Some(RecorderCommand::Pause)),
            RecorderPhase::Paused => ("Resume", Some(RecorderCommand::Resume)),
            RecorderPhase::Stopped => ("Saving…", None),
        };
        let main = egui::Button::new(label).shortcut_text(main_key);
        if ui.add_enabled(command.is_some(), main).clicked() {
            actions.extend(command.map(Action::Recorder));
        }
        let can_stop = matches!(phase, RecorderPhase::Recording | RecorderPhase::Paused);
        let stop = egui::Button::new("Stop and save").shortcut_text(stop_key);
        if ui.add_enabled(can_stop, stop).on_hover_text("Finish this recording").clicked() {
            actions.push(Action::Recorder(RecorderCommand::Stop));
        }
    });
    if phase == RecorderPhase::Idle {
        let note = if cfg!(target_arch = "wasm32") {
            "Audio is stored in this browser when you stop recording."
        } else {
            "Audio is saved on this computer as you record."
        };
        ui.label(egui::RichText::new(note).weak().small());
    }
    audio_loss_warning(ui, view);
}

/// Only appears if something was actually lost, so a healthy recording
/// shows no noise.
fn audio_loss_warning(ui: &mut egui::Ui, view: &View) {
    if view.audio_dropped_ms > 0 {
        let text = format!(
            "{} of audio could not be saved because the computer was too busy. Closing other apps may help.",
            format_loss(view.audio_dropped_ms)
        );
        ui.label(egui::RichText::new(text).color(theme::chip_text()).background_color(theme::WARNING_FILL));
    }
    if view.audio_overruns > 0 {
        let text = format!("The system reported {} audio glitch(es) during this recording.", view.audio_overruns);
        ui.label(egui::RichText::new(text).weak().small());
    }
}

fn format_loss(ms: u64) -> String {
    if ms < 1_000 { format!("{ms} ms") } else { format!("{:.1} s", ms as f64 / 1_000.0) }
}

fn status_light(ui: &mut egui::Ui, phase: RecorderPhase) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
    let color = match phase {
        RecorderPhase::Recording => theme::RECORD_RED,
        RecorderPhase::Paused => theme::label_stroke(lectio_core::LabelKind::Review),
        _ => ui.visuals().weak_text_color(),
    };
    ui.painter().circle_filled(rect.center(), 7.0, color);
}
