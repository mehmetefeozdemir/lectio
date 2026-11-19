//! The list of notes for one session, in timeline order. Phase 2 adds
//! photo/sketch thumbnails, a "Slide N" chip that shows the linked slide,
//! and linking/unlinking a note to the slide on screen.

use eframe::egui;
use lectio_core::material::{Attachment, AttachmentKind, SlideLink};
use lectio_core::timeline::{active_note_index, format_offset};
use lectio_core::{LabelKind, Note, Session};

use super::{View, labels, theme};
use crate::action::Action;
use crate::material::ImageState;

const THUMBNAIL_MAX: egui::Vec2 = egui::vec2(320.0, 180.0);
/// Room kept on the right of each note for its "Labels" and "Delete" buttons.
const BUTTONS_WIDTH: f32 = 140.0;
const MIN_TEXT_WIDTH: f32 = 120.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// While recording: newest note at the bottom, no seeking.
    Capture,
    /// With a recording: click a note's time to play from there.
    Playback,
}

pub fn show(ui: &mut egui::Ui, view: &View, session: &Session, mode: Mode, actions: &mut Vec<Action>) {
    ui.label(egui::RichText::new(format!("Notes ({})", session.notes.len())).strong());
    if session.notes.is_empty() {
        ui.label(egui::RichText::new("No notes yet.").weak());
        return;
    }
    let active = match mode {
        Mode::Playback => active_note_index(&session.note_offsets(), view.offset_ms),
        Mode::Capture => None,
    };
    let shown_slide = view.material.shown_slide(session);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(mode == Mode::Capture)
        .show(ui, |ui| {
            for (index, note) in session.notes.iter().enumerate() {
                let row = Row { note, mode, active: active == Some(index), shown_slide };
                row.show(ui, view, session, actions);
            }
        });
}

/// "Slide 4 · Week 3" for a note's link, if it has one.
pub fn slide_text(session: &Session, note: &Note) -> Option<String> {
    let link = note.slide?;
    let deck = session.decks.iter().find(|d| d.id == link.deck)?;
    Some(format!("Slide {} · {}", link.page, deck.title))
}

/// A photo or sketch, scaled to fit `max_width`, or its loading/failed state.
pub fn attachment_image(ui: &mut egui::Ui, view: &View, attachment: &Attachment, max_width: f32) {
    let what = match attachment.kind {
        AttachmentKind::Photo => "Photo",
        AttachmentKind::Sketch => "Sketch",
    };
    match view.material.image(&attachment.blob) {
        Some(ImageState::Ready(texture)) => {
            let max = egui::vec2(max_width.min(ui.available_width()), max_width * 0.75);
            ui.add(egui::Image::from_texture(texture).max_size(max).corner_radius(4.0).alt_text(what));
        }
        Some(ImageState::Failed(problem)) => {
            ui.label(egui::RichText::new(format!("{what} could not be shown: {problem}")).weak());
        }
        _ => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new(format!("Loading {}…", what.to_lowercase())).weak());
            });
        }
    }
}

struct Row<'n> {
    note: &'n Note,
    mode: Mode,
    active: bool,
    shown_slide: Option<SlideLink>,
}

impl Row<'_> {
    fn show(&self, ui: &mut egui::Ui, view: &View, session: &Session, actions: &mut Vec<Action>) {
        let fill = if self.active { theme::highlight_fill(ui) } else { egui::Color32::TRANSPARENT };
        egui::Frame::new().fill(fill).corner_radius(6.0).inner_margin(6.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Three columns: time | text, chips and image | buttons. The middle
            // column gets a fixed width so it wraps instead of squeezing chips.
            ui.horizontal_top(|ui| {
                self.time_cell(ui, actions);
                let text_width = (ui.available_width() - BUTTONS_WIDTH).max(MIN_TEXT_WIDTH);
                ui.vertical(|ui| {
                    ui.set_width(text_width);
                    self.body_cell(ui, actions);
                    if !self.note.labels.is_empty() || self.note.slide.is_some() {
                        ui.horizontal_wrapped(|ui| {
                            for &kind in &self.note.labels {
                                labels::chip(ui, view.library.labels(), kind);
                            }
                            self.slide_chip(ui, session, actions);
                        });
                    }
                    if let Some(attachment) = &self.note.attachment {
                        attachment_image(ui, view, attachment, THUMBNAIL_MAX.x.min(text_width));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if ui.small_button("Delete").on_hover_text("Delete this note").clicked() {
                        actions.push(Action::DeleteNote(self.note.id));
                    }
                    self.note_menu(ui, view, actions);
                });
            });
        });
    }

    fn time_cell(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let offset = self.note.offset_ms;
        let time = egui::RichText::new(format_offset(offset)).monospace();
        match self.mode {
            Mode::Playback => {
                let clicked = ui.button(time).on_hover_text(format!("Play from {}", format_offset(offset))).clicked();
                if clicked {
                    actions.push(Action::PlayFrom(offset));
                }
            }
            Mode::Capture => {
                ui.label(time);
            }
        }
    }

    fn body_cell(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let placeholder = match self.note.attachment.as_ref().map(|a| a.kind) {
            Some(AttachmentKind::Photo) => "Photo",
            Some(AttachmentKind::Sketch) => "Sketch",
            None => "Marked moment",
        };
        let text = if self.note.body.is_empty() {
            egui::RichText::new(placeholder).italics().weak()
        } else {
            egui::RichText::new(&self.note.body)
        };
        let playback = self.mode == Mode::Playback;
        let sense = if playback { egui::Sense::click() } else { egui::Sense::hover() };
        if ui.add(egui::Label::new(text).wrap().sense(sense)).clicked() && playback {
            actions.push(Action::PlayFrom(self.note.offset_ms));
        }
    }

    fn slide_chip(&self, ui: &mut egui::Ui, session: &Session, actions: &mut Vec<Action>) {
        let (Some(link), Some(text)) = (self.note.slide, slide_text(session, self.note)) else { return };
        let chip = egui::Button::new(egui::RichText::new(format!("Slide {}", link.page)).small()).small();
        if ui.add(chip).on_hover_text(format!("Show {text}")).clicked() {
            actions.push(Action::GoToSlide(link));
        }
    }

    fn note_menu(&self, ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
        let note = self.note;
        ui.menu_button("Labels", |ui| {
            for kind in LabelKind::ALL {
                let mut on = note.labels.contains(&kind);
                if ui.checkbox(&mut on, view.library.labels().name(kind)).changed() {
                    actions.push(Action::ToggleLabel(note.id, kind));
                }
            }
            let shown = self.shown_slide.filter(|s| note.slide != Some(*s));
            if shown.is_some() || note.slide.is_some() {
                ui.separator();
            }
            if let Some(shown) = shown
                && ui.button(format!("Link to slide {} (on screen)", shown.page)).clicked()
            {
                actions.push(Action::LinkNote(note.id, Some(shown)));
            }
            if note.slide.is_some() && ui.button("Remove slide link").clicked() {
                actions.push(Action::LinkNote(note.id, None));
            }
        })
        .response
        .on_hover_text("Labels and slide link for this note");
    }
}
