//! The Search page (ADR-018): one box searching notes, headings, transcripts,
//! slide titles and slide text in every session. Each result names where it
//! came from and opens the session at that moment (and slide).

use eframe::egui::{self, CornerRadius, RichText, Role, Sense, Stroke};
use lectio_core::search::{Hit, HitKind};
use lectio_core::timeline::format_offset;

use super::theme::{self, palette, radius, size, space};
use super::{Buffers, View, home, icons, widgets};
use crate::action::Action;

pub fn page(ui: &mut egui::Ui, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    home::page_title(ui, "Search", "Notes, headings, transcripts and slides in every session", |_| {});
    let hint = widgets::icon_text(icons::SEARCH, "Search everything…", theme::regular(size::BODY), p.text_muted);
    let field = ui.add(egui::TextEdit::singleline(&mut buffers.search).hint_text(hint).desired_width(f32::INFINITY));
    widgets::name(&field, Role::SearchInput, "Search everything");
    if !field.has_focus() && buffers.search.is_empty() && !field.lost_focus() {
        field.request_focus();
    }
    if field.changed() || (buffers.search != view.speech.query) {
        actions.push(Action::SearchFor(buffers.search.clone()));
    }
    ui.add_space(space::M);
    if buffers.search.trim().is_empty() {
        let message = "Type a word. Every word must appear; the last one can be unfinished.";
        ui.label(RichText::new(message).color(p.text_muted));
        return;
    }
    if view.speech.hits.is_empty() {
        ui.label(RichText::new("Nothing found.").color(p.text_muted));
        return;
    }
    let count = view.speech.hits.len();
    widgets::section_label(ui, &format!("{count} result{}", if count == 1 { "" } else { "s" }));
    ui.add_space(space::XS);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for hit in view.speech.hits {
            result(ui, view, hit, actions);
        }
    });
}

fn kind_text(kind: HitKind) -> (&'static str, &'static str) {
    match kind {
        HitKind::Heading => (icons::HEADING, "Heading"),
        HitKind::Note => (icons::EDIT, "Note"),
        HitKind::SlideTitle => (icons::SLIDES, "Slide in the feed"),
        HitKind::SlideText => (icons::SLIDES, "Slide text"),
        HitKind::Transcript => (icons::TRANSCRIPT, "Transcript"),
    }
}

fn result(ui: &mut egui::Ui, view: &View, hit: &Hit, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let session = view.library.session(hit.session);
    let title = session.map_or("", |s| s.title.as_str());
    let course = view.library.course_of(hit.session).map_or("", |c| c.name.as_str());
    let (icon, kind) = kind_text(hit.kind);
    let frame = egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::CARD))
        .inner_margin(egui::Margin::symmetric(space::M as i8, space::S as i8));
    let inner = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(widgets::icon_text(icon, kind, theme::semibold(size::SMALL), p.accent));
            let mut place = format!("{title} · {course}");
            if let Some(offset) = hit.offset_ms {
                place.push_str(&format!(" · {}", format_offset(offset)));
            }
            if let Some(link) = hit.slide {
                place.push_str(&format!(" · slide {}", link.page));
            }
            ui.label(RichText::new(place).size(size::SMALL).color(p.text_muted));
        });
        ui.add(egui::Label::new(RichText::new(&hit.snippet).color(p.text)).wrap());
    });
    let rect = inner.response.rect;
    let response = ui.interact(rect, ui.scope_id().with((hit.session, hit.offset_ms, hit.slide.map(|s| s.page))), Sense::click());
    if response.hovered() {
        ui.painter().rect_stroke(rect, CornerRadius::same(radius::CARD), Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
    }
    widgets::name(&response, Role::ListItem, &format!("Result in {title}: {}", hit.snippet));
    if response.on_hover_text("Open at this moment").clicked() {
        actions.push(Action::OpenHit { session: hit.session, offset_ms: hit.offset_ms, slide: hit.slide });
    }
    ui.add_space(space::S);
}
