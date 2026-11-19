//! "Screenshot to feed" (ADR-019): the captured screen in a window; drag over
//! the part to keep (or keep all of it), then add it to the feed.

use eframe::egui::{self, Pos2, Rect, RichText, Role, Sense, Stroke, StrokeKind};

use super::theme::{palette, size, space};
use super::{Buffers, View, icons, widgets};
use crate::action::Action;

const PREVIEW_MAX: [f32; 2] = [900.0, 560.0];

pub fn window(ctx: &egui::Context, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let Some(texture) = view.screenshot else {
        buffers.crop = None;
        return;
    };
    let mut open = true;
    egui::Window::new("Screenshot to feed").collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
        match texture {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Taking the screenshot…");
                });
            }
            Some(texture) => picker(ui, texture, buffers, actions),
        }
    });
    if !open {
        actions.push(Action::CloseScreenshot);
    }
}

fn picker(ui: &mut egui::Ui, texture: &egui::TextureHandle, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    ui.label(RichText::new("Drag over the part to keep. Nothing selected keeps the whole screen.").size(size::CAPTION).color(p.text_muted));
    ui.add_space(space::S);
    let image_size = texture.size_vec2();
    let scale = (PREVIEW_MAX[0] / image_size.x).min(PREVIEW_MAX[1] / image_size.y).min(1.0);
    let (rect, response) = ui.allocate_exact_size(image_size * scale, Sense::drag());
    ui.painter().image(texture.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), egui::Color32::WHITE);
    let to_fraction = |pos: Pos2| Pos2::new(((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0), ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0));
    if let Some(pos) = response.interact_pointer_pos() {
        if response.drag_started() {
            buffers.crop = Some((to_fraction(pos), to_fraction(pos)));
        } else if let Some((_, end)) = &mut buffers.crop {
            *end = to_fraction(pos);
        }
    }
    let selection = buffers.crop.map(|(a, b)| Rect::from_two_pos(a, b)).filter(|r| r.width() > 0.01 && r.height() > 0.01);
    if let Some(sel) = selection {
        let on_screen = Rect::from_min_max(rect.lerp_inside(sel.min.to_vec2()), rect.lerp_inside(sel.max.to_vec2()));
        let shade = egui::Color32::from_black_alpha(110);
        for part in outside(rect, on_screen) {
            ui.painter().rect_filled(part, 0.0, shade);
        }
        ui.painter().rect_stroke(on_screen, 0.0, Stroke::new(2.0, p.accent), StrokeKind::Outside);
    }
    widgets::name(&response, Role::Canvas, "Screenshot area");
    ui.add_space(space::S);
    ui.horizontal(|ui| {
        let region = selection.unwrap_or(Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)));
        let label = if selection.is_some() { "Add selection to feed" } else { "Add whole screen to feed" };
        if widgets::primary_button(ui, icons::SCREENSHOT, label, true).clicked() {
            actions.push(Action::SaveScreenshot { x: region.min.x, y: region.min.y, w: region.width(), h: region.height() });
            buffers.crop = None;
        }
        if widgets::subtle_button(ui, icons::CLOSE, "Cancel", true).clicked() {
            actions.push(Action::CloseScreenshot);
            buffers.crop = None;
        }
    });
}

/// The four bands of `outer` around `inner` (to darken what isn't kept).
fn outside(outer: Rect, inner: Rect) -> [Rect; 4] {
    [
        Rect::from_min_max(outer.min, Pos2::new(outer.max.x, inner.min.y)),
        Rect::from_min_max(Pos2::new(outer.min.x, inner.max.y), outer.max),
        Rect::from_min_max(Pos2::new(outer.min.x, inner.min.y), Pos2::new(inner.min.x, inner.max.y)),
        Rect::from_min_max(Pos2::new(inner.max.x, inner.min.y), Pos2::new(outer.max.x, inner.max.y)),
    ]
}
