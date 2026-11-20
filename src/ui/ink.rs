//! Drawing on slides (ADR-024): strokes painted over a page image, the
//! drawing tools, and turning a pointer drag into a stroke. Points are page
//! fractions (0..1), so the same strokes fit the slides panel at any zoom,
//! the full-screen view and the paper view.

use eframe::egui::{self, Color32, CornerRadius, Painter, Pos2, Rect, Role, Sense, Shape, Stroke};
use lectio_core::DeckId;
use lectio_core::ink::{InkColor, InkStroke, MAX_POINTS};

use super::theme::{palette, radius};
use super::{Buffers, icons, widgets};
use crate::action::{Action, InkPen};
use crate::app::InkState;
use crate::media::ink::{alpha, in_drawing_order, stroke_width};

/// Skip pointer moves shorter than this (page fraction): keeps strokes small.
const MIN_STEP: f32 = 0.002;
/// Shown while drawing, before the stroke is saved.
const LIVE_PEN_WIDTH: f32 = 0.003;
const LIVE_HIGHLIGHTER_WIDTH: f32 = 0.016;

fn color(c: InkColor, a: u8) -> Color32 {
    let [r, g, b] = c.rgb();
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn to_screen(rect: Rect, [x, y]: [f32; 2]) -> Pos2 {
    rect.min + egui::vec2(x * rect.width(), y * rect.height())
}

fn line(painter: &Painter, rect: Rect, points: &[[f32; 2]], width: f32, fill: Color32) {
    let screen: Vec<Pos2> = points.iter().map(|p| to_screen(rect, *p)).collect();
    match screen.as_slice() {
        [] => {}
        [dot] => {
            painter.circle_filled(*dot, width / 2.0, fill);
        }
        _ => {
            painter.add(Shape::line(screen, Stroke::new(width, fill)));
        }
    }
}

/// Paints a page's strokes over `rect` (highlighters under pens).
pub fn paint(painter: &Painter, rect: Rect, strokes: &[InkStroke]) {
    for stroke in in_drawing_order(strokes) {
        line(painter, rect, &stroke.points, stroke_width(stroke, rect.width()), color(stroke.color, alpha(stroke)));
    }
}

/// Takes pointer input over the page while drawing is on.
pub fn input(ui: &mut egui::Ui, rect: Rect, deck: DeckId, page: u32, state: InkState, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let response = ui.interact(rect, ui.scope_id().with(("ink", deck, page)), Sense::click_and_drag());
    widgets::name(&response, Role::Canvas, &format!("Drawing area on slide {page}"));
    let response = response.on_hover_cursor(egui::CursorIcon::Crosshair);
    let Some(pos) = response.interact_pointer_pos() else {
        return;
    };
    let at = [((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0), ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0)];
    if state.pen == InkPen::Eraser {
        if response.dragged() || response.clicked() {
            actions.push(Action::EraseInk { deck, page, at });
        }
        return;
    }
    if response.drag_started() || response.clicked() {
        buffers.ink_stroke.clear();
    }
    let far_enough = buffers.ink_stroke.last().is_none_or(|l| (l[0] - at[0]).abs() + (l[1] - at[1]).abs() > MIN_STEP);
    if far_enough && buffers.ink_stroke.len() < MAX_POINTS {
        buffers.ink_stroke.push(at);
    }
    if response.drag_stopped() || response.clicked() {
        let points = std::mem::take(&mut buffers.ink_stroke);
        actions.push(Action::AddInkStroke { deck, page, points });
    }
    let width = if state.pen == InkPen::Highlighter { LIVE_HIGHLIGHTER_WIDTH } else { LIVE_PEN_WIDTH };
    let a = if state.pen == InkPen::Highlighter { crate::media::ink::HIGHLIGHT_ALPHA } else { 255 };
    line(ui.painter(), rect, &buffers.ink_stroke, (width * rect.width()).max(1.0), color(state.color, a));
}

/// Draw on/off, then (when on) pen, highlighter, eraser, colours, undo, clear.
pub fn toolbar(ui: &mut egui::Ui, state: InkState, deck: DeckId, page: u32, has_ink: bool, enabled: bool, actions: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        let tip = if state.on { "Stop drawing" } else { "Draw or highlight on this slide" };
        if widgets::toggle_icon(ui, icons::PEN, "Draw on this slide", tip, state.on, enabled).clicked() {
            actions.push(Action::SetInking(!state.on));
        }
        if !state.on {
            return;
        }
        let tools = [(InkPen::Pen, icons::SKETCH, "Pen"), (InkPen::Highlighter, icons::HIGHLIGHTER, "Highlighter"), (InkPen::Eraser, icons::ERASER, "Eraser")];
        for (pen, icon, name) in tools {
            if widgets::toggle_icon(ui, icon, name, name, state.pen == pen, true).clicked() {
                actions.push(Action::SetInkTool(pen));
            }
        }
        ui.separator();
        for c in InkColor::ALL {
            swatch(ui, c, state.color == c && state.pen != InkPen::Eraser, actions);
        }
        ui.separator();
        if widgets::icon_button(ui, icons::UNDO_STROKE, "Undo the last stroke", has_ink).clicked() {
            actions.push(Action::UndoInk { deck, page });
        }
        if widgets::icon_button(ui, icons::CLEAR, "Clear this slide's drawing", has_ink).clicked() {
            actions.push(Action::ClearInk { deck, page });
        }
    });
}

fn swatch(ui: &mut egui::Ui, c: InkColor, selected: bool, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::click());
    let ring = if selected { Stroke::new(2.0, p.accent) } else { Stroke::new(1.0, p.border) };
    ui.painter().rect(rect.shrink(3.0), CornerRadius::same(radius::CONTROL), color(c, 255), ring, egui::StrokeKind::Outside);
    if response.has_focus() {
        ui.painter().rect_stroke(rect, CornerRadius::same(radius::CONTROL), ui.visuals().selection.stroke, egui::StrokeKind::Outside);
    }
    let name = format!("Ink colour: {}", c.name());
    widgets::name(&response, Role::Button, &name);
    // After `name`, so screen readers also hear whether it is the chosen colour.
    response.widget_info(move || egui::WidgetInfo::selected(Role::Button, true, selected, &name));
    if response.on_hover_text(c.name()).clicked() {
        actions.push(Action::SetInkColor(c));
    }
}
