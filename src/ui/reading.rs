//! Reading view: the session as a document in a centred column (max 720 px):
//! headings as section titles, notes, photos and sketches, and the slides
//! posted to the feed. No audio. "Copy all" and "Export Markdown" give the
//! same text as `lectio_core::export::session_markdown`.

use eframe::egui::{self, RichText};
use lectio_core::{NoteKind, Session};
use lectio_core::timeline::format_offset;

use super::theme::{self, palette, size, space};
use super::{View, format, icons, notes, widgets};
use crate::material::PageView;
use crate::action::Action;

const COLUMN_WIDTH: f32 = 720.0;
const READING_SIZE: f32 = 17.0;
const LINE_HEIGHT: f32 = 27.0;

pub fn show(ui: &mut egui::Ui, view: &View, session: &Session, actions: &mut Vec<Action>) {
    let p = palette(ui);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let width = ui.available_width().min(COLUMN_WIDTH);
        let margin = ((ui.available_width() - width) / 2.0).max(space::L);
        ui.horizontal(|ui| {
            ui.add_space(margin);
            ui.vertical(|ui| {
                ui.set_width(width);
                ui.add_space(space::XL);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&session.title).font(theme::semibold(size::HEADING)).color(p.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::subtle_button(ui, icons::CLOSE, "Back to the session", true).clicked() {
                            actions.push(Action::SetReading(false));
                        }
                        if widgets::subtle_button(ui, icons::EXPORT, "Export Markdown", view.can_import).clicked() {
                            actions.push(Action::ExportMarkdown);
                        }
                        if widgets::subtle_button(ui, icons::COPY, "Copy all", true).clicked() {
                            actions.push(Action::CopyAll);
                        }
                    });
                });
                let course = view.library.course_of(session.id).map(|c| c.name.as_str()).unwrap_or("");
                let line = format!("{course} · {} · {} notes", format::session_details(session), session.notes.len());
                ui.label(RichText::new(line).size(size::CAPTION).color(p.text_muted));
                ui.add_space(space::XL);
                if session.notes.is_empty() {
                    ui.label(RichText::new("No notes in this session yet.").color(p.text_muted));
                }
                for note in &session.notes {
                    entry(ui, view, session, note);
                }
                ui.add_space(space::XXL);
            });
        });
    });
}

fn entry(ui: &mut egui::Ui, view: &View, session: &Session, note: &lectio_core::Note) {
    let p = palette(ui);
    if note.is_heading() {
        ui.add_space(space::M);
        ui.label(RichText::new(&note.body).font(theme::semibold(size::HEADING)).color(p.text));
        ui.add_space(space::M);
        return;
    }
    if note.kind == NoteKind::Slide {
        return slide_entry(ui, view, session, note);
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format_offset(note.offset_ms)).font(theme::mono(size::SMALL)).color(p.text_muted));
        for &kind in &note.labels {
            widgets::label_chip(ui, view.library.labels(), kind);
        }
        if let Some(text) = notes::slide_text(session, note) {
            ui.label(widgets::icon_text(icons::SLIDES, &text, theme::regular(size::SMALL), p.text_muted));
        }
    });
    if !note.body.is_empty() {
        let text = RichText::new(&note.body).size(READING_SIZE).line_height(Some(LINE_HEIGHT)).color(p.text);
        ui.add(egui::Label::new(text).wrap());
    }
    if let Some(attachment) = &note.attachment {
        ui.add_space(space::XS);
        notes::attachment_image(ui, view, attachment, ui.available_width());
    }
    ui.add_space(space::XL);
}

/// A posted slide: its title and time, then the page at column width.
fn slide_entry(ui: &mut egui::Ui, view: &View, session: &Session, note: &lectio_core::Note) {
    let p = palette(ui);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format_offset(note.offset_ms)).font(theme::mono(size::SMALL)).color(p.text_muted));
        ui.label(widgets::icon_text(icons::SLIDES, &note.body, theme::semibold(size::BODY), p.text));
    });
    let page = note.slide.and_then(|link| Some((session.decks.iter().find(|d| d.id == link.deck)?, link.page)));
    if let Some((deck, number)) = page
        && let PageView::Ready(texture) = view.material.page(deck.id, number)
    {
        ui.add_space(space::XS);
        let alt = format!("{}, slide {number}", deck.title);
        ui.add(egui::Image::from_texture(texture).max_width(ui.available_width()).alt_text(alt));
    }
    ui.add_space(space::XL);
}
