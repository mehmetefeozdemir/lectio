//! The "Rename labels" window. Only names change; each label keeps its
//! identity, so existing notes keep their labels. (Chips live in `widgets`.)

use eframe::egui::{self, RichText};

use super::theme::{self, palette, size};
use super::{Buffers, icons, widgets};
use crate::action::Action;

pub fn editor_window(ctx: &egui::Context, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let Some(names) = &mut buffers.label_names else { return };
    let mut open = true;
    egui::Window::new("Rename labels")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new("Give the labels names that fit how you study.").color(palette(ui).text_muted));
            egui::Grid::new("label-names").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                for (kind, name) in names.iter_mut() {
                    let colors = theme::label_colors(*kind, ui.visuals().dark_mode);
                    ui.label(widgets::icon_text(icons::label(*kind), kind.default_name(), theme::regular(size::CAPTION), colors.marker));
                    ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
                    ui.end_row();
                }
            });
            ui.horizontal(|ui| {
                if widgets::primary_button(ui, icons::TAG, "Save names", true).clicked() {
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
