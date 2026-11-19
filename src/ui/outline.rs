//! The Outline tab of the right panel (Phase 4a, ADR-021): section titles
//! an AI provider wrote, each with the moment and slide it came from.
//! Clicking a time plays from there; the outline can be added to the feed
//! as Heading notes.

use eframe::egui::{self, RichText, Role};
use lectio_core::Session;
use lectio_core::sources::Citation;
use lectio_core::timeline::format_offset;

use super::theme::{self, palette, size, space};
use super::{View, icons, widgets};
use crate::action::Action;

pub fn show(ui: &mut egui::Ui, view: &View, session: &Session, actions: &mut Vec<Action>) {
    let p = palette(ui);
    match view.study.job {
        Some((what, done, total)) => {
            let text = format!("Writing the {what}: part {} of {total}…", (done + 1).min(total));
            let bar = ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).text(text).desired_height(18.0));
            widgets::name(&bar, Role::ProgressIndicator, "AI progress");
            if let Some((loaded, all)) = view.study.download {
                ui.label(RichText::new(format!("Downloading the study model: {}%", loaded * 100 / all.max(1))).size(size::SMALL).color(p.text_muted));
            }
        }
        None => {
            ui.horizontal_wrapped(|ui| {
                let label = if session.outline.is_some() { "Write again" } else { "Write an outline" };
                if widgets::primary_button(ui, icons::HEADING, label, true).clicked() {
                    actions.push(Action::GenerateOutline);
                }
                if session.outline.is_some() && widgets::subtle_button(ui, icons::TO_FEED, "Add to feed as headings", view.can_take_notes).clicked() {
                    actions.push(Action::OutlineToFeed);
                }
            });
        }
    }
    ui.add_space(space::S);
    let Some(outline) = &session.outline else {
        let hint = "The outline lists the lecture's sections with the moment each starts, from your notes, slides and transcript.";
        ui.add(egui::Label::new(RichText::new(hint).size(size::SMALL).color(p.text_muted)).wrap());
        return;
    };
    ui.label(RichText::new(format!("Written by {}", outline.model)).size(size::SMALL).color(p.text_muted));
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for item in &outline.items {
            ui.horizontal_top(|ui| {
                if item.level > 1 {
                    ui.add_space(space::L);
                }
                citations(ui, session, &item.cites, actions);
                let font = if item.level > 1 { theme::regular(size::CAPTION) } else { theme::semibold(size::BODY) };
                ui.add(egui::Label::new(RichText::new(&item.title).font(font).color(p.text)).wrap());
            });
            ui.add_space(space::XS);
        }
    });
}

/// Time / slide chips that open the place an item came from.
pub fn citations(ui: &mut egui::Ui, session: &Session, cites: &[Citation], actions: &mut Vec<Action>) {
    for cite in cites.iter().take(2) {
        let text = match (cite.offset_ms, cite.slide) {
            (Some(offset), _) => format_offset(offset),
            (None, Some(link)) => format!("slide {}", link.page),
            (None, None) => continue,
        };
        let label = format!("Go to {text}");
        if widgets::time_pill(ui, &text, true, &label).clicked() {
            actions.push(Action::OpenHit { session: session.id, offset_ms: cite.offset_ms, slide: cite.slide });
        }
    }
}
