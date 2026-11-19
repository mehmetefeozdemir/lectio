//! The audio timeline: a bar with the played part filled in, a marker for
//! every note, and a playhead. Click or drag to seek; clicks near a marker
//! snap to that note. With keyboard focus, ←/→ seek 5 s and Home/End jump to
//! the ends. All position maths is in `lectio_core::timeline`.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind};
use lectio_core::timeline::{format_offset, fraction_to_offset, offset_to_fraction};
use lectio_core::{Note, Session};

use super::{SEEK_STEP_MS, View, theme};
use crate::action::Action;

const HEIGHT: f32 = 54.0;
const TRACK_HEIGHT: f32 = 10.0;
/// How close (in points) a click must be to a marker to snap to it.
const SNAP_DISTANCE: f32 = 6.0;

/// Draws the timeline and returns a seek request, if any.
pub fn show(ui: &mut egui::Ui, view: &View, session: &Session, duration_ms: u64) -> Option<Action> {
    let size = egui::vec2(ui.available_width(), HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), egui::vec2(rect.width() - 16.0, TRACK_HEIGHT));
    let x_of = |offset: u64| track.left() + offset_to_fraction(offset, duration_ms) * track.width();

    let label = format!(
        "Recording position {} of {}",
        format_offset(view.offset_ms),
        format_offset(duration_ms)
    );
    response.widget_info(|| egui::WidgetInfo::slider(true, view.offset_ms as f64 / 1_000.0, &label));

    paint(ui, &response, rect, track, view, session, &x_of);
    let hovered = response.hover_pos().and_then(|p| nearest_note(session, p, &x_of));
    if let Some(note) = hovered {
        response.clone().on_hover_ui_at_pointer(|ui| note_tooltip(ui, view, note));
    }

    if let Some(pos) = response.interact_pointer_pos().filter(|_| response.clicked() || response.dragged()) {
        let snapped = nearest_note(session, pos, &x_of).map(|n| n.offset_ms);
        let fraction = (pos.x - track.left()) / track.width();
        return Some(Action::Seek(snapped.unwrap_or_else(|| fraction_to_offset(fraction, duration_ms))));
    }
    keyboard_seek(ui, &response, duration_ms)
}

fn paint(
    ui: &egui::Ui,
    response: &egui::Response,
    rect: Rect,
    track: Rect,
    view: &View,
    session: &Session,
    x_of: &dyn Fn(u64) -> f32,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(track, 5.0, theme::track_fill(ui));
    let played = Rect::from_min_max(track.min, Pos2::new(x_of(view.offset_ms), track.max.y));
    painter.rect_filled(played, 5.0, theme::progress_fill(ui));

    for note in &session.notes {
        let x = x_of(note.offset_ms);
        let color = note.labels.first().map_or(theme::NOTE_MARKER, |&k| theme::label_stroke(k));
        painter.line_segment([Pos2::new(x, rect.top() + 8.0), Pos2::new(x, rect.bottom() - 8.0)], Stroke::new(2.0, color));
        if !note.labels.is_empty() {
            // A flag on top marks labelled moments, so they differ by shape too.
            let top = rect.top() + 8.0;
            let flag = vec![Pos2::new(x, top), Pos2::new(x + 9.0, top + 4.0), Pos2::new(x, top + 8.0)];
            painter.add(egui::Shape::convex_polygon(flag, color, Stroke::NONE));
        }
    }

    let head = Pos2::new(x_of(view.offset_ms), track.center().y);
    painter.circle_filled(head, 8.0, ui.visuals().strong_text_color());
    painter.circle_filled(head, 5.0, theme::progress_fill(ui));
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
    ui.label(egui::RichText::new(format_offset(note.offset_ms)).monospace());
    if !note.body.is_empty() {
        ui.label(&note.body);
    }
    let names: Vec<&str> = note.labels.iter().map(|&k| view.library.labels().name(k)).collect();
    if !names.is_empty() {
        ui.label(egui::RichText::new(names.join(", ")).color(Color32::GRAY));
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
