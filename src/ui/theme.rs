//! Lectio's look: larger text for readability, and one colour per label.
//! Label chips use dark text on light fills (contrast above 7:1) and always
//! show the label's name, so colour is never the only signal.

use eframe::egui::{self, Color32, FontId, TextStyle};

pub const SAMPLE_BADGE: Color32 = Color32::from_rgb(0xE6, 0xDD, 0xFF);
pub const WARNING_FILL: Color32 = Color32::from_rgb(0xFF, 0xE9, 0xA8);
pub const ERROR_FILL: Color32 = Color32::from_rgb(0xFF, 0xC9, 0xC2);
pub const RECORD_RED: Color32 = Color32::from_rgb(0xD9, 0x2D, 0x20);
pub const NOTE_MARKER: Color32 = Color32::from_rgb(0x8A, 0x94, 0xA6);

const BODY_SIZE: f32 = 16.0;
const BUTTON_SIZE: f32 = 15.0;
const HEADING_SIZE: f32 = 22.0;
const SMALL_SIZE: f32 = 13.0;
const MONO_SIZE: f32 = 15.0;

pub fn install(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::proportional(HEADING_SIZE)),
            (TextStyle::Body, FontId::proportional(BODY_SIZE)),
            (TextStyle::Button, FontId::proportional(BUTTON_SIZE)),
            (TextStyle::Small, FontId::proportional(SMALL_SIZE)),
            (TextStyle::Monospace, FontId::monospace(MONO_SIZE)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
    });
}

/// Near-black used on top of the light label and banner fills.
pub fn chip_text() -> Color32 {
    Color32::from_rgb(0x1A, 0x1A, 0x1A)
}

pub fn label_fill(kind: lectio_core::LabelKind) -> Color32 {
    use lectio_core::LabelKind as K;
    match kind {
        K::Important => Color32::from_rgb(0xFF, 0xB3, 0xA7),
        K::Review => Color32::from_rgb(0xFF, 0xD8, 0x75),
        K::Task => Color32::from_rgb(0x9F, 0xE3, 0xAE),
        K::Definition => Color32::from_rgb(0xA9, 0xC8, 0xFF),
    }
}

/// Stronger version of the label colour, for thin marker lines.
pub fn label_stroke(kind: lectio_core::LabelKind) -> Color32 {
    use lectio_core::LabelKind as K;
    match kind {
        K::Important => Color32::from_rgb(0xD8, 0x3B, 0x2B),
        K::Review => Color32::from_rgb(0xC9, 0x8A, 0x00),
        K::Task => Color32::from_rgb(0x1F, 0x9D, 0x4C),
        K::Definition => Color32::from_rgb(0x2F, 0x6F, 0xE0),
    }
}

pub fn highlight_fill(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x2C, 0x3A, 0x55)
    } else {
        Color32::from_rgb(0xE3, 0xEC, 0xFF)
    }
}

pub fn track_fill(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_gray(0x3A)
    } else {
        Color32::from_gray(0xDD)
    }
}

pub fn progress_fill(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x6E, 0x8F, 0xD6)
    } else {
        Color32::from_rgb(0x9D, 0xB6, 0xEC)
    }
}
