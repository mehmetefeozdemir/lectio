//! The note input of the input bar: one text field, attach icons for photo,
//! camera (where available) and sketch, the five labels as compact coloured
//! chips (shortcut in the tooltip), and "Post next slide". A note is stamped with the
//! moment the user started typing (`NoteDraft` in lectio-core).

use eframe::egui::{self, CornerRadius, Key, Modifiers, Role};
use lectio_core::LabelKind;
use lectio_core::timeline::format_offset;

use super::theme::{self, palette, radius, size, space};
use super::home::LABEL_ORDER;
use super::{LABEL_KEYS, POST_SLIDE, View, icons, shortcut_hint, widgets};
use crate::action::Action;

/// Room right of the text field for the attach icons and the send button.
const ATTACH_WIDTH: f32 = 6.0 * 36.0 + 40.0;
/// Below this width the label chips and "Post next slide" take two rows.
const NARROW_BAR: f32 = 820.0;

pub fn show(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let enabled = view.can_take_notes;
    input_row(ui, view, enabled, actions);
    // A right-aligned button can't share a row that wraps: at 200% zoom it
    // covered the last label chip. Narrow windows get it on its own row.
    if ui.available_width() < NARROW_BAR {
        ui.horizontal_wrapped(|ui| label_chips(ui, view, enabled, actions));
        ui.horizontal(|ui| post_slide_button(ui, view, actions));
    } else {
        ui.horizontal(|ui| {
            label_chips(ui, view, enabled, actions);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| post_slide_button(ui, view, actions));
        });
    }
}

/// Adds the next slide to the feed at the current time.
fn post_slide_button(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let enabled = view.can_take_notes && view.material.viewer.deck.is_some();
    let hint = shortcut_hint(ui.ctx(), POST_SLIDE.0, POST_SLIDE.1);
    let text = widgets::icon_text(icons::SLIDES, "Post next slide", theme::semibold(size::SMALL), p.accent);
    let button = ui.add_enabled(enabled, egui::Button::new(text).small());
    widgets::name(&button, Role::Button, "Post next slide");
    let why = if view.material.viewer.deck.is_none() { "Import slides first. " } else { "" };
    if button.on_hover_text(format!("{why}Turn to the next slide and add it to the feed now ({hint})")).clicked() {
        actions.push(Action::PostSlide { next: true });
    }
}

fn input_row(ui: &mut egui::Ui, view: &View, enabled: bool, actions: &mut Vec<Action>) {
    let mut text = view.draft.text().to_owned();
    let anchor = format_offset(view.draft.anchor_ms(view.offset_ms));
    let formula = view.paper.formula_mode;
    if formula && enabled {
        super::formula::preview(ui, &text);
    }
    let hint = match (enabled, formula) {
        (false, _) => "Start recording to take notes",
        (true, true) => r"LaTeX formula, e.g. \frac{a}{b}  (Enter to save)",
        (true, false) => "Write a note…  (Enter to save)",
    };
    ui.horizontal(|ui| {
        widgets::time_pill(ui, &anchor, false, "Time the note will be saved at").on_hover_text("The time this note will be saved at");
        let width = ui.available_width() - ATTACH_WIDTH;
        let edit = egui::TextEdit::singleline(&mut text).hint_text(hint).desired_width(width);
        let edit = if formula { edit.font(egui::TextStyle::Monospace) } else { edit };
        let field = ui.add_enabled(enabled, edit);
        widgets::name(&field, Role::TextInput, if formula { "Formula (LaTeX)" } else { "Note text" });
        if field.changed() {
            actions.push(Action::DraftChanged(text.clone()));
        }
        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        let can_save = enabled && !text.trim().is_empty();
        attach_icons(ui, view, actions);
        let send = widgets::icon_button_sized(ui, icons::SEND, "Save note (Enter)", can_save, size::ICON_LARGE);
        if send.clicked() || (entered && can_save) {
            actions.push(Action::SubmitNote);
            field.request_focus();
        }
    });
}

/// One-tap labels: icon + name on the label's soft colour. Heading needs
/// text (it turns the typed note into a section title).
fn label_chips(ui: &mut egui::Ui, view: &View, enabled: bool, actions: &mut Vec<Action>) {
    let dark = ui.visuals().dark_mode;
    ui.spacing_mut().item_spacing.x = space::XS + 2.0;
    let has_text = !view.draft.text().trim().is_empty();
    for kind in LABEL_ORDER {
        let colors = theme::label_colors(kind, dark);
        let name = view.library.labels().name(kind);
        let text = widgets::icon_text(icons::label(kind), name, theme::regular(size::SMALL), colors.text);
        let button = egui::Button::new(text).fill(colors.fill).corner_radius(CornerRadius::same(radius::CONTROL)).small();
        let key = LabelKind::ALL.iter().position(|k| *k == kind).map(|i| LABEL_KEYS[i]).unwrap_or(Key::Num1);
        let hint = shortcut_hint(ui.ctx(), Modifiers::COMMAND, key);
        let is_heading = kind == LabelKind::Heading;
        let tip = if is_heading {
            format!("Save the typed text as a section heading ({hint})")
        } else {
            format!("Mark this moment as {name} ({hint}). If you are typing a note, it is saved with this label.")
        };
        let response = ui.add_enabled(enabled && (has_text || !is_heading), button).on_hover_text(tip);
        widgets::name(&response, Role::Button, &format!("Label this moment: {name}"));
        if response.clicked() {
            actions.push(Action::LabelMoment(kind));
        }
    }
}

fn attach_icons(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let enabled = view.can_take_notes && view.can_import;
    let formula = view.paper.formula_mode;
    let tip = if formula { "Back to text notes" } else { "Write a formula (LaTeX), with a live preview" };
    if widgets::toggle_icon(ui, icons::FORMULA, "Formula mode", tip, formula, view.can_take_notes).clicked() {
        actions.push(Action::SetFormulaMode(!formula));
    }
    if widgets::icon_button(ui, icons::IMAGE, "Add a photo from a file", enabled).clicked() {
        actions.push(Action::AddPhotoFromFile);
    }
    if view.camera_available && widgets::icon_button(ui, icons::CAMERA, "Take a photo with the camera", enabled).clicked() {
        actions.push(Action::OpenCamera);
    }
    if widgets::icon_button(ui, icons::SKETCH, "Add a sketch", enabled).clicked() {
        actions.push(Action::OpenSketch);
    }
    let label = "Screenshot part of the screen into the feed";
    if view.screenshots && widgets::icon_button(ui, icons::SCREENSHOT, label, enabled).clicked() {
        actions.push(Action::TakeScreenshot);
    }
}
