//! The note box: type a note, press Enter to save it at the moment you
//! started typing (see `NoteDraft` in lectio-core).

use eframe::egui;
use lectio_core::timeline::format_offset;

use super::{View, labels};
use crate::action::Action;

pub fn show(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let enabled = view.can_take_notes;
    let mut text = view.draft.text().to_owned();
    let anchor = view.draft.anchor_ms(view.offset_ms);
    let hint = if enabled {
        "Type a note and press Enter. It is stamped with the moment you started typing."
    } else {
        "Start recording to take notes."
    };

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format_offset(anchor)).monospace().strong())
            .on_hover_text("The time this note will be saved at");
        let field = ui.add_enabled(
            enabled,
            egui::TextEdit::singleline(&mut text).hint_text(hint).desired_width(ui.available_width() - 110.0),
        );
        if field.changed() {
            actions.push(Action::DraftChanged(text.clone()));
        }
        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let can_save = enabled && !text.trim().is_empty();
        if ui.add_enabled(can_save, egui::Button::new("Save note")).clicked() || (entered && can_save) {
            actions.push(Action::SubmitNote);
            field.request_focus();
        }
    });
    labels::moment_buttons(ui, view.library.labels(), enabled, actions);
    material_buttons(ui, view, actions);
}

/// Phase 2: add a photo (file, or camera where available) or a sketch at
/// the current moment.
fn material_buttons(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let enabled = view.can_take_notes && view.can_import;
    ui.horizontal_wrapped(|ui| {
        ui.label("Add to the timeline:");
        let photo = ui.add_enabled(enabled, egui::Button::new("Photo from file…"));
        if photo.on_hover_text("Pick a photo (PNG or JPEG), e.g. of the board").clicked() {
            actions.push(Action::AddPhotoFromFile);
        }
        if view.camera_available {
            let camera = ui.add_enabled(enabled, egui::Button::new("Take photo…"));
            if camera.on_hover_text("Use this device's camera").clicked() {
                actions.push(Action::OpenCamera);
            }
        }
        let sketch = ui.add_enabled(enabled, egui::Button::new("Sketch…"));
        if sketch.on_hover_text("Draw a quick diagram; it is saved as an image").clicked() {
            actions.push(Action::OpenSketch);
        }
    });
}
