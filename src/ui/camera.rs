//! The camera window (browser only, ADR-013): live preview and a button to
//! take the photo, which becomes a photo note at the current moment.

use eframe::egui;

use super::View;
use crate::action::Action;

const PREVIEW_WIDTH: f32 = 640.0;

pub fn window(ctx: &egui::Context, view: &View, actions: &mut Vec<Action>) {
    let Some(camera) = &view.material.camera else { return };
    let mut open = true;
    egui::Window::new("Take a photo")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .min_width(PREVIEW_WIDTH.min(ctx.content_rect().width() - 80.0))
        .open(&mut open)
        .show(ctx, |ui| {
        if let Some(problem) = &camera.problem {
            ui.label(egui::RichText::new(problem).color(super::theme::palette(ui).danger_text));
        }
        match &camera.preview {
            Some(texture) => {
                let image = egui::Image::from_texture(texture).max_width(PREVIEW_WIDTH).alt_text("Camera preview");
                ui.add(image);
            }
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Waiting for the camera. If the browser asks, allow it.");
                });
            }
        }
        ui.horizontal(|ui| {
            let take = super::widgets::primary_button(ui, super::icons::CAMERA, "Take the photo", camera.preview.is_some());
            if take.clicked() {
                actions.push(Action::TakePhoto);
            }
            if ui.button("Cancel").clicked() {
                actions.push(Action::CloseCamera);
            }
        });
    });
    if !open {
        actions.push(Action::CloseCamera);
    }
}
