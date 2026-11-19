//! Reading view (SPEC Phase 2): the session's notes only — text, labels,
//! photos and sketches in timeline order — with no recorder, player or
//! slides, so a student can review at their own pace.

use eframe::egui;
use lectio_core::Session;
use lectio_core::timeline::format_offset;

use super::{View, labels, notes};
use crate::action::Action;

const READING_WIDTH: f32 = 720.0;
const READING_TEXT_SIZE: f32 = 18.0;

pub fn show(ui: &mut egui::Ui, view: &View, session: &Session, actions: &mut Vec<Action>) {
    let course = view.library.course_of(session.id).map(|c| c.name.as_str()).unwrap_or("");
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(course).weak());
        ui.label(egui::RichText::new("›").weak());
        ui.heading(&session.title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Back to the session").clicked() {
                actions.push(Action::SetReading(false));
            }
        });
    });
    ui.label(egui::RichText::new(format!("Reading view · {} notes", session.notes.len())).weak());
    ui.separator();
    if session.notes.is_empty() {
        ui.label("No notes in this session yet.");
        return;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_max_width(READING_WIDTH);
        for note in &session.notes {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(format_offset(note.offset_ms)).monospace().weak());
                for &kind in &note.labels {
                    labels::chip(ui, view.library.labels(), kind);
                }
                if let Some(text) = notes::slide_text(session, note) {
                    ui.label(egui::RichText::new(text).weak());
                }
            });
            if !note.body.is_empty() {
                ui.add(egui::Label::new(egui::RichText::new(&note.body).size(READING_TEXT_SIZE)).wrap());
            }
            if let Some(attachment) = &note.attachment {
                notes::attachment_image(ui, view, attachment, READING_WIDTH);
            }
            ui.add_space(14.0);
        }
    });
}
