//! Label chips, the one-tap "label this moment" row, and the rename window.

use eframe::egui::{self, Key, Modifiers, Stroke};
use lectio_core::{LabelKind, LabelSet};

use super::{Buffers, shortcut_hint, theme};
use crate::action::Action;

const NUMBER_KEYS: [Key; 4] = [Key::Num1, Key::Num2, Key::Num3, Key::Num4];

/// A small coloured tag showing a label's name. Never wraps inside the chip
/// (a narrow column moves the whole chip to the next line instead).
pub fn chip(ui: &mut egui::Ui, labels: &LabelSet, kind: LabelKind) {
    let text = egui::RichText::new(labels.name(kind)).small().color(theme::chip_text());
    egui::Frame::new()
        .fill(theme::label_fill(kind))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| ui.add(egui::Label::new(text).extend()));
}

/// "Label this moment" buttons, one per label, with Cmd/Ctrl+1..4 hints.
pub fn moment_buttons(ui: &mut egui::Ui, labels: &LabelSet, enabled: bool, actions: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Label this moment:");
        for (kind, key) in LabelKind::ALL.into_iter().zip(NUMBER_KEYS) {
            let hint = shortcut_hint(ui.ctx(), Modifiers::COMMAND, key);
            let button = egui::Button::new(egui::RichText::new(labels.name(kind)).color(theme::chip_text()))
                .fill(theme::label_fill(kind))
                .stroke(Stroke::new(1.0, theme::label_stroke(kind)))
                .shortcut_text(egui::RichText::new(hint).color(theme::chip_text()).small());
            let response = ui.add_enabled(enabled, button).on_hover_text(format!(
                "Mark the current moment as {}. If you are typing a note, it is saved with this label.",
                labels.name(kind)
            ));
            if response.clicked() {
                actions.push(Action::LabelMoment(kind));
            }
        }
    });
}

/// Window for renaming the four labels. Only names change; each label keeps
/// its identity, so existing notes keep their labels.
pub fn editor_window(ctx: &egui::Context, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let Some(names) = &mut buffers.label_names else { return };
    let mut open = true;
    egui::Window::new("Rename labels")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("Give the labels names that fit how you study.");
            egui::Grid::new("label-names").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                for (kind, name) in names.iter_mut() {
                    ui.label(kind.default_name());
                    ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
                    ui.end_row();
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    actions.push(Action::SaveLabels(names.clone()));
                }
                if ui.button("Cancel").clicked() {
                    actions.push(Action::CloseLabelEditor);
                }
            });
        });
    if !open {
        actions.push(Action::CloseLabelEditor);
    }
}
