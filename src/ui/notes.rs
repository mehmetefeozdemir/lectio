//! The feed: everything taken in the session, in time order, like a message
//! thread. Notes, photos, sketches and posted slides are cards with a time
//! pill; Heading notes are section titles. Per-item actions (labels, slide
//! link, delete) live in one quiet "⋯" menu. During playback the playing
//! item is highlighted and scrolled into view.

use eframe::egui::{self, CornerRadius, RichText, Role};
use lectio_core::material::{Attachment, AttachmentKind, SlideLink};
use lectio_core::timeline::{active_note_index, format_offset};
use lectio_core::{LabelKind, Note, NoteKind, Session};

use super::theme::{self, palette, radius, size, space};
use super::{Buffers, View, icons, widgets};
use crate::action::Action;
use crate::material::{ImageState, PageView};

const THUMBNAIL_WIDTH: f32 = 320.0;
const SLIDE_THUMBNAIL_WIDTH: f32 = 280.0;
/// Cards don't stretch across very wide windows (easier to read).
const BUBBLE_MAX_WIDTH: f32 = 760.0;
const MENU_WIDTH: f32 = 36.0;

pub fn feed(ui: &mut egui::Ui, view: &View, session: &Session, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    widgets::section_label(ui, &format!("Feed · {}", session.notes.len()));
    if session.notes.is_empty() {
        let hint = if view.can_take_notes {
            "Type below and press Enter. Notes, photos, sketches and slides stack here in time order."
        } else {
            "Start recording, then type notes below. They line up with the audio."
        };
        ui.add_space(space::S);
        ui.label(RichText::new(hint).color(p.text_muted));
        return;
    }
    let playback = view.duration_ms.is_some();
    let active = playback.then(|| active_note_index(&session.note_offsets(), view.offset_ms)).flatten();
    let follow_id = ui.scope_id().with("followed-note");
    // Re-follow when the playing item changes or the content height changes
    // (images loading move items), but not while both stay put, so the user
    // can scroll freely during playback.
    let follow: Follow = ui.ctx().data(|d| d.get_temp(follow_id)).unwrap_or_default();
    let must_follow = follow.must_follow(active);
    let shown_slide = view.material.shown_slide(session);
    // A clear edge under the header instead of egui's scroll fade, which
    // painted over the top card and made it look hidden under the header
    // (PROBLEMS P-025).
    let edge = ui.cursor().top() + space::XS;
    ui.painter().hline(ui.max_rect().x_range(), edge, egui::Stroke::new(1.0, p.border));
    ui.add_space(space::XS * 2.0);
    ui.spacing_mut().scroll.fade.strength = 0.0;
    let scroll = egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(!playback).show(ui, |ui| {
        ui.add_space(space::S);
        for (index, note) in session.notes.iter().enumerate() {
            let is_active = active == Some(index);
            let item = Item { note, playback, active: is_active, shown_slide };
            let response = if note.is_heading() {
                heading(ui, view, &item, actions)
            } else {
                bubble(ui, view, session, &item, buffers, actions)
            };
            // The feed follows playback: when the playing item changes, scroll to it.
            if is_active && must_follow {
                response.scroll_to_me(Some(egui::Align::Center));
            }
        }
    });
    let follow = follow.after_frame(active, must_follow, scroll.content_size.y.round() as i32);
    ui.ctx().data_mut(|d| d.insert_temp(follow_id, follow));
}

/// Which item the feed last scrolled to, and the content height then.
#[derive(Clone, Copy, Default)]
struct Follow {
    index: Option<usize>,
    height_then: i32,
    last_height: i32,
}

impl Follow {
    fn must_follow(&self, active: Option<usize>) -> bool {
        active.is_some() && (active != self.index || self.last_height != self.height_then)
    }

    fn after_frame(self, active: Option<usize>, followed: bool, height: i32) -> Self {
        let (index, height_then) = if followed { (active, height) } else { (self.index, self.height_then) };
        Self { index, height_then, last_height: height }
    }
}

/// What every feed item needs besides the note itself.
struct Item<'a> {
    note: &'a Note,
    playback: bool,
    active: bool,
    shown_slide: Option<SlideLink>,
}

/// "Slide 4 · Week 3" for a note's link, if it has one.
pub fn slide_text(session: &Session, note: &Note) -> Option<String> {
    let link = note.slide?;
    let deck = session.decks.iter().find(|d| d.id == link.deck)?;
    Some(format!("Slide {} · {}", link.page, deck.title))
}

/// A photo or sketch as a rounded thumbnail, or its loading/failed state.
pub fn attachment_image(ui: &mut egui::Ui, view: &View, attachment: &Attachment, max_width: f32) {
    let what = match attachment.kind {
        AttachmentKind::Photo => "Photo",
        AttachmentKind::Sketch => "Sketch",
    };
    let p = palette(ui);
    match view.material.image(&attachment.blob) {
        Some(ImageState::Ready(texture)) => {
            let max = egui::vec2(max_width.min(ui.available_width()), max_width * 0.75);
            let image = egui::Image::from_texture(texture).max_size(max).corner_radius(CornerRadius::same(radius::CARD)).alt_text(what);
            ui.add(image);
        }
        Some(ImageState::Failed(problem)) => {
            ui.label(RichText::new(format!("{what} could not be shown: {problem}")).size(size::SMALL).color(p.text_muted));
        }
        _ => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(format!("Loading {}…", what.to_lowercase())).size(size::SMALL).color(p.text_muted));
            });
        }
    }
}

/// A Heading note: a section title across the feed, no bubble.
fn heading(ui: &mut egui::Ui, view: &View, item: &Item, actions: &mut Vec<Action>) -> egui::Response {
    let p = palette(ui);
    ui.add_space(space::M);
    let inner = ui.horizontal(|ui| {
        let time = format_offset(item.note.offset_ms);
        if widgets::time_pill(ui, &time, item.playback, &format!("Play from {time}")).clicked() && item.playback {
            actions.push(Action::PlayFrom(item.note.offset_ms));
        }
        let color = if item.active { p.accent } else { p.text };
        let title = egui::Label::new(RichText::new(&item.note.body).font(theme::semibold(size::TITLE)).color(color)).wrap();
        let width = (ui.available_width() - MENU_WIDTH).max(80.0);
        ui.scope(|ui| {
            ui.set_max_width(width);
            ui.add(title);
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| note_menu(ui, view, item.note, item.shown_slide, actions));
    });
    let r = inner.response.rect;
    ui.painter().hline(r.x_range(), r.bottom() + 3.0, egui::Stroke::new(1.0, p.border));
    ui.add_space(space::S);
    inner.response
}

/// A note, photo, sketch or slide as a card, newest at the bottom.
fn bubble(ui: &mut egui::Ui, view: &View, session: &Session, item: &Item, buffers: &mut Buffers, actions: &mut Vec<Action>) -> egui::Response {
    let p = palette(ui);
    let note = item.note;
    let (fill, stroke) = if item.active { (p.accent_soft, p.accent) } else { (p.surface, p.border) };
    let frame = egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(CornerRadius::same(radius::CARD))
        .inner_margin(egui::Margin::symmetric(space::M as i8, space::S as i8));
    let inner = frame.show(ui, |ui| {
        ui.set_width(ui.available_width().min(BUBBLE_MAX_WIDTH));
        ui.horizontal_top(|ui| {
            let time = format_offset(note.offset_ms);
            let pill = widgets::time_pill(ui, &time, item.playback, &format!("Play from {time}"));
            if item.playback && pill.clicked() {
                actions.push(Action::PlayFrom(note.offset_ms));
            }
            let content_width = (ui.available_width() - MENU_WIDTH).max(120.0);
            ui.vertical(|ui| {
                ui.set_width(content_width);
                if note.kind == NoteKind::Slide {
                    slide_item(ui, view, session, note, buffers, actions);
                } else {
                    body(ui, note, item.playback, actions);
                }
                chips(ui, view, session, note, actions);
                if let Some(attachment) = &note.attachment {
                    attachment_image(ui, view, attachment, THUMBNAIL_WIDTH.min(content_width));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| note_menu(ui, view, note, item.shown_slide, actions));
        });
    });
    ui.add_space(space::S);
    inner.response
}

/// A slide posted to the feed: its page as a thumbnail and an editable title.
fn slide_item(ui: &mut egui::Ui, view: &View, session: &Session, note: &Note, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let editing = buffers.note_edit.as_ref().is_some_and(|(id, _)| *id == note.id);
    if editing {
        slide_title_editor(ui, note, buffers, actions);
    } else {
        ui.horizontal(|ui| {
            let text = widgets::icon_text(icons::SLIDES, &note.body, theme::semibold(size::BODY), p.text);
            let title = ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text("Click to rename");
            let pencil = widgets::icon_button(ui, icons::EDIT, "Rename slide", true);
            if title.clicked() || pencil.clicked() {
                buffers.note_edit = Some((note.id, note.body.clone()));
            }
        });
    }
    let Some(link) = note.slide else { return };
    let Some(deck) = session.decks.iter().find(|d| d.id == link.deck) else { return };
    match view.material.page(deck.id, link.page) {
        PageView::Ready(texture) => {
            let alt = format!("{}, slide {}", deck.title, link.page);
            let image = egui::Image::from_texture(texture)
                .max_width(SLIDE_THUMBNAIL_WIDTH.min(ui.available_width()))
                .corner_radius(CornerRadius::same(radius::CONTROL))
                .alt_text(alt)
                .sense(egui::Sense::click());
            let response = egui::Frame::new()
                .stroke(egui::Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(radius::CONTROL))
                .show(ui, |ui| ui.add(image))
                .inner
                .on_hover_text("Show this slide");
            widgets::name(&response, egui::Role::Image, &format!("Slide {} thumbnail", link.page));
            super::ink::paint(ui.painter(), response.rect, deck.ink_on(link.page));
            if response.clicked() {
                actions.push(Action::GoToSlide(link));
            }
        }
        PageView::Loading => {
            ui.spinner();
        }
        PageView::Failed(problem) => {
            ui.label(RichText::new(problem).size(size::SMALL).color(p.text_muted));
        }
    }
}

fn slide_title_editor(ui: &mut egui::Ui, note: &Note, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let Some((_, text)) = &mut buffers.note_edit else { return };
    let field = ui.add(egui::TextEdit::singleline(text).desired_width(ui.available_width()));
    widgets::name(&field, egui::Role::TextInput, "Slide title");
    if !field.has_focus() && !field.lost_focus() {
        field.request_focus();
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        buffers.note_edit = None;
    } else if field.lost_focus() {
        actions.push(Action::EditNoteText(note.id, text.clone()));
        buffers.note_edit = None;
    }
}

fn body(ui: &mut egui::Ui, note: &Note, playback: bool, actions: &mut Vec<Action>) {
    let p = palette(ui);
    if note.kind == NoteKind::Math {
        let response = super::formula::show(ui, &note.body, size::BODY * 1.3, egui::Sense::click());
        if response.clicked() && playback {
            actions.push(Action::PlayFrom(note.offset_ms));
        }
        return;
    }
    let placeholder = match note.attachment.as_ref().map(|a| a.kind) {
        Some(AttachmentKind::Photo) => "Photo",
        Some(AttachmentKind::Sketch) => "Sketch",
        None => "Marked moment",
    };
    let text = if note.body.is_empty() {
        RichText::new(placeholder).italics().color(p.text_muted)
    } else {
        RichText::new(&note.body).color(p.text)
    };
    let (response, word) = widgets::word_label(ui, text, egui::Sense::click());
    if let Some(word) = word.filter(|_| !note.body.is_empty()) {
        actions.push(Action::LookUp { word, session: None, offset_ms: Some(note.offset_ms) });
    } else if response.clicked() && playback {
        actions.push(Action::PlayFrom(note.offset_ms));
    }
}

fn chips(ui: &mut egui::Ui, view: &View, session: &Session, note: &Note, actions: &mut Vec<Action>) {
    let slide_chip = note.slide.filter(|_| note.kind != NoteKind::Slide);
    if note.labels.is_empty() && slide_chip.is_none() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = space::XS + 2.0;
        for &kind in &note.labels {
            widgets::label_chip(ui, view.library.labels(), kind);
        }
        if let (Some(link), Some(text)) = (slide_chip, slide_text(session, note)) {
            let chip = widgets::neutral_chip(ui, Some(icons::SLIDES), &format!("Slide {}", link.page), true, &format!("Show {text}"));
            if chip.clicked() {
                actions.push(Action::GoToSlide(link));
            }
        }
    });
}

fn note_menu(ui: &mut egui::Ui, view: &View, note: &Note, shown_slide: Option<SlideLink>, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let menu = ui.menu_button(widgets::icon(icons::MORE, size::ICON, p.text_muted), |ui| {
        widgets::section_label(ui, "Labels");
        for kind in LabelKind::ALL {
            let mut on = note.labels.contains(&kind);
            let text = widgets::icon_text(icons::label(kind), view.library.labels().name(kind), theme::regular(size::CAPTION), p.text);
            if ui.checkbox(&mut on, text).changed() {
                actions.push(Action::ToggleLabel(note.id, kind));
            }
        }
        let shown = shown_slide.filter(|s| note.slide != Some(*s));
        if shown.is_some() || note.slide.is_some() {
            ui.separator();
        }
        if let Some(shown) = shown
            && ui.button(widgets::icon_text(icons::LINK, &format!("Link to slide {} (on screen)", shown.page), theme::regular(size::CAPTION), p.text)).clicked()
        {
            actions.push(Action::LinkNote(note.id, Some(shown)));
        }
        if note.slide.is_some() && ui.button(widgets::icon_text(icons::UNLINK, "Remove slide link", theme::regular(size::CAPTION), p.text)).clicked() {
            actions.push(Action::LinkNote(note.id, None));
        }
        ui.separator();
        let delete = widgets::icon_text(icons::TRASH, "Delete note", theme::regular(size::CAPTION), p.danger_text);
        if ui.button(delete).clicked() {
            actions.push(Action::DeleteNote(note.id));
        }
    });
    let label = format!("Actions for the note at {}", format_offset(note.offset_ms));
    widgets::name(&menu.response, Role::Button, &label);
    menu.response.on_hover_text("Labels, slide link, delete");
}

