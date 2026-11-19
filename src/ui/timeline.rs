//! The audio timeline: a thin track with the played part in the accent
//! colour, a small marker per note (label notes use the label's shape and
//! colour: diamond, circle, square, triangle), and a playhead. Click or drag
//! to seek; clicks near a marker snap to it. With keyboard focus, ←/→ seek
//! 5 s and Home/End jump to the ends. Position maths: `lectio_core::timeline`.

use eframe::egui::{self, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, StrokeKind};
use lectio_core::timeline::{format_offset, fraction_to_offset, offset_to_fraction};
use lectio_core::{Note, Session};

use super::theme::{self, palette, size};
use super::{SEEK_STEP_MS, View, widgets};
use crate::action::Action;

const HEIGHT: f32 = 36.0;
const TRACK_HEIGHT: f32 = 4.0;
const MARKER_RADIUS: f32 = 4.5;
/// How close (in points) a click must be to a marker to snap to it.
const SNAP_DISTANCE: f32 = 6.0;

/// Draws the timeline and returns a seek request, if any.
pub fn show(ui: &mut egui::Ui, view: &View, session: &Session, duration_ms: u64) -> Option<Action> {
    let size = egui::vec2(ui.available_width(), HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), egui::vec2(rect.width() - 16.0, TRACK_HEIGHT));
    let x_of = |offset: u64| track.left() + offset_to_fraction(offset, duration_ms) * track.width();

    let label = format!("Recording position {} of {}", format_offset(view.offset_ms), format_offset(duration_ms));
    response.widget_info(|| egui::WidgetInfo::slider(true, view.offset_ms as f64 / 1_000.0, &label));
    widgets::name(&response, egui::Role::Slider, "Timeline");

    paint(ui, &response, rect, track, view, session, &x_of);
    if let Some(note) = response.hover_pos().and_then(|p| nearest_note(session, p, &x_of)) {
        response.clone().on_hover_ui_at_pointer(|ui| note_tooltip(ui, view, note));
    }
    if let Some(pos) = response.interact_pointer_pos().filter(|_| response.clicked() || response.dragged()) {
        let snapped = nearest_note(session, pos, &x_of).map(|n| n.offset_ms);
        let fraction = (pos.x - track.left()) / track.width();
        return Some(Action::Seek(snapped.unwrap_or_else(|| fraction_to_offset(fraction, duration_ms))));
    }
    keyboard_seek(ui, &response, duration_ms)
}

fn paint(ui: &egui::Ui, response: &egui::Response, rect: Rect, track: Rect, view: &View, session: &Session, x_of: &dyn Fn(u64) -> f32) {
    let p = palette(ui);
    let dark = ui.visuals().dark_mode;
    let painter = ui.painter_at(rect);
    let round = CornerRadius::same(2);
    painter.rect_filled(track, round, p.track);
    painter.rect_filled(Rect::from_min_max(track.min, Pos2::new(x_of(view.offset_ms), track.max.y)), round, p.accent);

    for note in &session.notes {
        let center = Pos2::new(x_of(note.offset_ms), track.center().y - 10.0);
        match note.labels.first() {
            Some(&kind) => {
                let color = theme::label_colors(kind, dark).marker;
                widgets::paint_label_marker(&painter, kind, center, MARKER_RADIUS, color);
            }
            None => {
                painter.circle_filled(center, MARKER_RADIUS - 1.5, p.marker);
            }
        }
    }
    let head = Pos2::new(x_of(view.offset_ms), track.center().y);
    painter.circle_filled(head, 7.0, p.surface);
    painter.circle_stroke(head, 6.0, Stroke::new(2.5, p.accent));
    if response.has_focus() {
        painter.rect_stroke(rect.shrink(1.0), 6.0, ui.visuals().selection.stroke, StrokeKind::Inside);
    }
}

fn nearest_note<'a>(session: &'a Session, pos: Pos2, x_of: &dyn Fn(u64) -> f32) -> Option<&'a Note> {
    session
        .notes
        .iter()
        .map(|n| (n, (x_of(n.offset_ms) - pos.x).abs()))
        .filter(|(_, d)| *d <= SNAP_DISTANCE)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(n, _)| n)
}

fn note_tooltip(ui: &mut egui::Ui, view: &View, note: &Note) {
    let p = palette(ui);
    ui.label(RichText::new(format_offset(note.offset_ms)).font(theme::mono(size::CAPTION)).color(p.text_muted));
    if !note.body.is_empty() {
        ui.label(&note.body);
    }
    let names: Vec<&str> = note.labels.iter().map(|&k| view.library.labels().name(k)).collect();
    if !names.is_empty() {
        ui.label(RichText::new(names.join(", ")).size(size::SMALL).color(p.text_muted));
    }
}

fn keyboard_seek(ui: &egui::Ui, response: &egui::Response, duration_ms: u64) -> Option<Action> {
    if !response.has_focus() {
        return None;
    }
    ui.input(|i| {
        if i.key_pressed(egui::Key::ArrowLeft) {
            Some(Action::SeekBy(-SEEK_STEP_MS))
        } else if i.key_pressed(egui::Key::ArrowRight) {
            Some(Action::SeekBy(SEEK_STEP_MS))
        } else if i.key_pressed(egui::Key::Home) {
            Some(Action::Seek(0))
        } else if i.key_pressed(egui::Key::End) {
            Some(Action::Seek(duration_ms))
        } else {
            None
        }
    })
}
