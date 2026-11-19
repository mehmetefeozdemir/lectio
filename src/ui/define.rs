//! The definition pop-up (Phase 4a, ADR-022): what a double-clicked word
//! means, from the offline dictionary (or online, if allowed), with "Add
//! to feed" for each sense.

use eframe::egui::{self, RichText};

use super::theme::{self, palette, size, space};
use super::{View, icons, widgets};
use crate::action::Action;

pub fn window(ctx: &egui::Context, view: &View, actions: &mut Vec<Action>) {
    let Some(lookup) = view.study.lookup else { return };
    let mut open = true;
    egui::Window::new(format!("Define \"{}\"", lookup.word))
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .open(&mut open)
        .show(ctx, |ui| {
            let p = palette(ui);
            match &lookup.result {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Looking it up…");
                    });
                }
                Some(Err(message)) => {
                    ui.add(egui::Label::new(RichText::new(message).color(p.text_muted)).wrap());
                    if !view.study.dictionary_ready && widgets::subtle_button(ui, icons::SETTINGS, "Dictionary settings", true).clicked() {
                        actions.push(Action::SetAiSettingsOpen(true));
                    }
                }
                Some(Ok((senses, source))) => {
                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        for (index, sense) in senses.iter().enumerate().take(8) {
                            ui.horizontal_top(|ui| {
                                ui.label(RichText::new(&sense.pos).font(theme::semibold(size::SMALL)).color(p.accent));
                                ui.vertical(|ui| {
                                    ui.add(egui::Label::new(RichText::new(&sense.definition).color(p.text)).wrap());
                                    if !sense.example.is_empty() {
                                        let example = format!("“{}”", sense.example);
                                        ui.label(RichText::new(example).italics().size(size::SMALL).color(p.text_muted));
                                    }
                                    let label = format!("Add meaning {} to the feed", index + 1);
                                    if widgets::subtle_button(ui, icons::DEFINITION, &label, view.can_take_notes).clicked() {
                                        actions.push(Action::DefinitionToFeed(index));
                                    }
                                });
                            });
                            ui.add_space(space::S);
                        }
                    });
                    ui.label(RichText::new(format!("Source: {source}")).size(size::SMALL).color(p.text_muted));
                }
            }
        });
    if !open {
        actions.push(Action::CloseLookUp);
    }
}
