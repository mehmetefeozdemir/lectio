//! The sketch window: draw with a mouse, trackpad or pen; save as an image
//! note (ADR-013). Strokes are kept in canvas pixels (960×600), so the saved
//! PNG doesn't depend on how big the window happens to be.

use eframe::egui::{self, Color32, Pos2, Sense, Stroke};
use lectio_core::timeline::format_offset;

use super::Buffers;
use crate::action::Action;
use crate::media::sketch::{SKETCH_HEIGHT, SKETCH_WIDTH, StrokePoints, is_blank};

const MAX_DISPLAY_WIDTH: f32 = 720.0;
const MIN_DISPLAY_WIDTH: f32 = 240.0;
/// Room around the canvas for the window frame.
const WINDOW_MARGIN: f32 = 80.0;
const PEN_WIDTH: f32 = 3.5;
/// Points closer than this (canvas pixels) to the previous one are skipped.
const MIN_POINT_GAP: f32 = 1.5;

/// A sketch being drawn. `offset_ms` is where it goes on the timeline: the
/// moment the window was opened.
pub struct SketchPad {
    pub strokes: Vec<StrokePoints>,
    pub caption: String,
    pub offset_ms: u64,
}

impl SketchPad {
    pub fn new(offset_ms: u64) -> Self {
        Self { strokes: Vec::new(), caption: String::new(), offset_ms }
    }
}

pub fn window(ctx: &egui::Context, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let Some(pad) = &mut buffers.sketch else { return };
    let mut open = true;
    // The canvas width comes from the screen, not from the window: a new
    // egui window starts narrow and would shrink the canvas to a stamp (P-016).
    let display_width = (ctx.content_rect().width() - WINDOW_MARGIN).clamp(MIN_DISPLAY_WIDTH, MAX_DISPLAY_WIDTH);
    egui::Window::new("Sketch")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
        ui.label(format!(
            "Draw with the mouse, trackpad or a pen. It is saved as an image note at {}.",
            format_offset(pad.offset_ms)
        ));
        canvas(ui, pad, display_width);
        ui.horizontal(|ui| {
            ui.label("Caption");
            let caption = ui.add(egui::TextEdit::singleline(&mut pad.caption).hint_text("optional").desired_width(320.0));
            super::widgets::name(&caption, egui::Role::TextInput, "Sketch caption");
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(!pad.strokes.is_empty(), egui::Button::new("Undo")).clicked() {
                pad.strokes.pop();
            }
            if ui.add_enabled(!pad.strokes.is_empty(), egui::Button::new("Clear")).clicked() {
                pad.strokes.clear();
            }
            if super::widgets::primary_button(ui, super::icons::SKETCH, "Save sketch", !is_blank(&pad.strokes)).clicked() {
                actions.push(Action::SaveSketch {
                    strokes: pad.strokes.clone(),
                    caption: pad.caption.clone(),
                    offset_ms: pad.offset_ms,
                });
            }
            if ui.button("Cancel").clicked() {
                actions.push(Action::CloseSketch);
            }
        });
    });
    if !open {
        actions.push(Action::CloseSketch);
    }
}

fn canvas(ui: &mut egui::Ui, pad: &mut SketchPad, display_width: f32) {
    let scale = display_width / SKETCH_WIDTH as f32;
    let size = egui::vec2(SKETCH_WIDTH as f32 * scale, SKETCH_HEIGHT as f32 * scale);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let to_canvas = |p: Pos2| [((p.x - rect.left()) / scale), ((p.y - rect.top()) / scale)];
    let to_screen = |[x, y]: [f32; 2]| Pos2::new(rect.left() + x * scale, rect.top() + y * scale);

    if response.drag_started() {
        pad.strokes.push(Vec::new());
    }
    if let (true, Some(pos)) = (response.dragged(), response.interact_pointer_pos()) {
        let point = to_canvas(pos.clamp(rect.min, rect.max));
        if let Some(stroke) = pad.strokes.last_mut() {
            let far_enough = stroke.last().is_none_or(|[x, y]| (point[0] - x).hypot(point[1] - y) >= MIN_POINT_GAP);
            if far_enough {
                stroke.push(point);
            }
        }
    }
    if let (true, Some(pos)) = (response.clicked(), response.interact_pointer_pos()) {
        pad.strokes.push(vec![to_canvas(pos)]);
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::WHITE);
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::GRAY), egui::StrokeKind::Inside);
    let ink = Stroke::new(PEN_WIDTH * scale, Color32::from_rgb(0x1A, 0x1A, 0x1A));
    for stroke in &pad.strokes {
        match stroke.as_slice() {
            [] => {}
            [single] => {
                painter.circle_filled(to_screen(*single), ink.width / 2.0, ink.color);
            }
            points => {
                painter.add(egui::Shape::line(points.iter().map(|p| to_screen(*p)).collect(), ink));
            }
        }
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Canvas, true, "Sketch canvas"));
    super::widgets::name(&response, egui::Role::Canvas, "Sketch canvas");
}
