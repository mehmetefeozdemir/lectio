//! Playback view: the audio timeline with note markers, and the notes.

use eframe::egui::{self, Key, Modifiers};
use lectio_core::Session;
use lectio_core::timeline::format_offset;

use super::{SEEK_STEP_MS, View, composer, notes, shortcut_hint, timeline};
use crate::action::Action;

/// Course name and session title above either view.
pub fn title(ui: &mut egui::Ui, view: &View, session: &Session) {
    let course = view.library.course_of(session.id).map(|c| c.name.as_str()).unwrap_or("");
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(course).weak());
        ui.label(egui::RichText::new("›").weak());
        ui.heading(&session.title);
    });
    ui.add_space(4.0);
}

pub fn show(ui: &mut egui::Ui, view: &View, session: &Session, actions: &mut Vec<Action>) {
    title(ui, view, session);
    let duration = view.duration_ms.unwrap_or(0);
    egui::Frame::group(ui.style()).inner_margin(12.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        transport(ui, view, duration, actions);
        if let Some(seek) = timeline::show(ui, view, session, duration) {
            actions.push(seek);
        }
        if let Some(problem) = view.playback_problem {
            ui.label(egui::RichText::new(problem).weak().small());
        }
    });
    ui.add_space(6.0);
    composer::show(ui, view, actions);
    ui.separator();
    notes::show(ui, view, session, notes::Mode::Playback, actions);
}

fn transport(ui: &mut egui::Ui, view: &View, duration: u64, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        let play_label = if view.playing { "Pause" } else { "Play" };
        let play = egui::Button::new(play_label).shortcut_text(shortcut_hint(ui.ctx(), Modifiers::COMMAND, Key::R));
        if ui.add_enabled(view.can_play, play).clicked() {
            actions.push(Action::TogglePlay);
        }
        let back = format!("Back 5 s ({})", shortcut_hint(ui.ctx(), Modifiers::COMMAND, Key::OpenBracket));
        if ui.add_enabled(view.can_play, egui::Button::new("−5 s")).on_hover_text(back).clicked() {
            actions.push(Action::SeekBy(-SEEK_STEP_MS));
        }
        let fwd = format!("Forward 5 s ({})", shortcut_hint(ui.ctx(), Modifiers::COMMAND, Key::CloseBracket));
        if ui.add_enabled(view.can_play, egui::Button::new("+5 s")).on_hover_text(fwd).clicked() {
            actions.push(Action::SeekBy(SEEK_STEP_MS));
        }
        let clock = format!("{} / {}", format_offset(view.offset_ms), format_offset(duration));
        ui.label(egui::RichText::new(clock).monospace().size(20.0));
    });
}
