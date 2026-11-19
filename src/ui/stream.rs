//! The audio stream: the recording as a vertical column of 10-second
//! sections (`lectio_core::stream`), each tinted with the colour of the first
//! label used in its time range. Click a section to play from its start; the
//! playing section is outlined and kept in view. While recording, a live
//! level bar shows that sound is arriving, and the column grows.

use eframe::egui::{self, CornerRadius, Rect, RichText, Role, Sense, Stroke, StrokeKind};
use lectio_core::Session;
use lectio_core::clock::RecorderPhase;
use lectio_core::stream::{audio_sections, section_at, section_labels};
use lectio_core::timeline::format_offset;

use super::capture::{SILENCE_HINT, SILENCE_HINT_S};
use super::theme::{self, palette, radius, size, space};
use super::{View, icons, widgets};
use crate::action::Action;

const SECTION_HEIGHT: f32 = 22.0;
const TIME_WIDTH: f32 = 44.0;
const LEVEL_HEIGHT: f32 = 8.0;

/// The right panel: an Audio | Transcript tab bar, then the chosen tab.
pub fn show(ui: &mut egui::Ui, view: &View, session: &Session, actions: &mut Vec<Action>) {
    // The Outline tab only exists with an AI provider (ADR-021).
    let outline_tab = view.prefs.show_outline_tab && view.study.ai_on;
    let current = if outline_tab { 2 } else { usize::from(view.prefs.show_transcript_tab) };
    let mut tabs = vec![(0, icons::WAVEFORM, "Audio"), (1, icons::TRANSCRIPT, "Transcript")];
    if view.study.ai_on {
        tabs.push((2, icons::HEADING, "Outline"));
    }
    ui.horizontal_wrapped(|ui| {
        for (index, icon, name) in tabs {
            let selected = current == index;
            let color = if selected { palette(ui).accent } else { palette(ui).text_muted };
            let text = widgets::icon_text(icon, name, theme::semibold(size::SMALL), color);
            let tab = ui.add(egui::Button::selectable(selected, text));
            widgets::name(&tab, Role::Tab, &format!("{name} tab in the right panel"));
            if tab.clicked() {
                actions.push(Action::ShowOutline(index == 2));
                if index < 2 {
                    actions.push(Action::ShowTranscript(index == 1));
                }
            }
        }
    });
    ui.add_space(space::XS);
    match current {
        2 => super::outline::show(ui, view, session, actions),
        1 => super::transcript::show(ui, view, session, actions),
        _ => audio(ui, view, session, actions),
    }
}

fn audio(ui: &mut egui::Ui, view: &View, session: &Session, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let recording = view.recorder_phase == Some(RecorderPhase::Recording);
    if view.recorder_phase.is_some() {
        live_level(ui, view, recording);
    }
    let duration = view.duration_ms.unwrap_or(view.offset_ms);
    let sections = audio_sections(duration);
    if sections.is_empty() {
        ui.add_space(space::S);
        let hint = "When you record, the audio appears here in 10-second sections.";
        ui.add(egui::Label::new(RichText::new(hint).size(size::SMALL).color(p.text_muted)).wrap());
        return;
    }
    let tints = section_labels(&sections, &session.notes);
    let playback = view.duration_ms.is_some();
    let current = section_at(&sections, view.offset_ms);
    let follow_id = ui.scope_id().with("followed-section");
    let followed = ui.ctx().data(|d| d.get_temp::<Option<usize>>(follow_id)).flatten();
    ui.add_space(space::XS);
    egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(!playback).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        for (index, section) in sections.iter().enumerate() {
            let tint = tints.get(index).copied().flatten();
            let playing = playback && current == Some(index);
            let response = section_row(ui, section.start_ms, tint, playing, playback);
            if playing && followed != Some(index) {
                response.scroll_to_me(Some(egui::Align::Center));
            }
            if playback && response.clicked() {
                actions.push(Action::PlayFrom(section.start_ms));
            }
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(follow_id, current.filter(|_| playback)));
}

fn section_row(ui: &mut egui::Ui, start_ms: u64, tint: Option<lectio_core::LabelKind>, playing: bool, clickable: bool) -> egui::Response {
    let p = palette(ui);
    let sense = if clickable { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), SECTION_HEIGHT), sense);
    let painter = ui.painter_at(rect.expand(2.0));
    let time = format_offset(start_ms);
    let time_color = if playing { p.accent } else { p.text_muted };
    painter.text(
        egui::pos2(rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        &time,
        theme::mono(size::SMALL),
        time_color,
    );
    let block = Rect::from_min_max(egui::pos2(rect.left() + TIME_WIDTH, rect.top()), rect.max);
    let (fill, marker) = match tint {
        Some(kind) => {
            let colors = theme::label_colors(kind, ui.visuals().dark_mode);
            (colors.fill, Some((kind, colors.marker)))
        }
        None => (p.surface_alt, None),
    };
    let fill = if response.hovered() && clickable { fill.gamma_multiply(0.85) } else { fill };
    painter.rect_filled(block, CornerRadius::same(radius::CONTROL), fill);
    if let Some((kind, color)) = marker {
        widgets::paint_label_marker(&painter, kind, egui::pos2(block.right() - 10.0, block.center().y), 4.0, color);
    }
    if playing {
        painter.rect_stroke(block, CornerRadius::same(radius::CONTROL), Stroke::new(2.0, p.accent), StrokeKind::Inside);
    }
    if response.has_focus() {
        painter.rect_stroke(block, CornerRadius::same(radius::CONTROL), ui.visuals().selection.stroke, StrokeKind::Outside);
    }
    if clickable {
        widgets::name(&response, Role::Button, &format!("Play from {time}"));
        return response.on_hover_text(format!("Play from {time}"));
    }
    response
}

/// A thin bar that follows the microphone level, and the silence hint.
fn live_level(ui: &mut egui::Ui, view: &View, recording: bool) {
    let p = palette(ui);
    let level = if recording { view.levels.last().copied().unwrap_or(0.0) } else { 0.0 };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), LEVEL_HEIGHT), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(4), p.track);
    let filled = Rect::from_min_size(rect.min, egui::vec2(rect.width() * level.sqrt().clamp(0.0, 1.0), rect.height()));
    ui.painter().rect_filled(filled, CornerRadius::same(4), p.record.gamma_multiply(0.8));
    response.widget_info(|| egui::WidgetInfo::labeled(Role::Meter, true, "Microphone level"));
    let status = if recording { "Listening" } else { "Not recording" };
    ui.label(RichText::new(status).size(size::SMALL).color(p.text_muted));
    if recording && view.quiet_for_s >= SILENCE_HINT_S {
        ui.add(egui::Label::new(RichText::new(SILENCE_HINT).size(size::SMALL).color(p.warning_text).background_color(p.warning_fill)).wrap());
    }
}
