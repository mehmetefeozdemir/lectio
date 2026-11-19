//! The navigation column on the list screens: Home, Collections, Tasks,
//! Search and Trash. Below `NARROW` (or when collapsed) it shows icons only.

use eframe::egui::{self, CornerRadius, Role, Sense};

use super::theme::{self, palette, radius, size, space};
use super::{View, icons, widgets};
use crate::action::{Action, Screen};

const ROW_HEIGHT: f32 = 34.0;

pub fn show(ui: &mut egui::Ui, view: &View, compact: bool, actions: &mut Vec<Action>) {
    let trashed = view.library.trashed_sessions().len();
    let trash_label = if trashed > 0 { format!("Trash ({trashed})") } else { "Trash".to_owned() };
    let items: [(Screen, &str, &str); 6] = [
        (Screen::Home, icons::HOME, "Home"),
        (Screen::Collections, icons::COLLECTIONS, "Collections"),
        (Screen::Tasks, icons::TASKS, "Tasks"),
        (Screen::Study, icons::DEFINITION, "Study"),
        (Screen::Search, icons::SEARCH, "Search"),
        (Screen::Trash, icons::TRASH, &trash_label),
    ];
    ui.add_space(space::S);
    for (screen, icon, label) in items {
        // The accessible name stays "Trash" so tests and screen readers can find it.
        let name = if screen == Screen::Trash { "Trash" } else { label };
        if row(ui, icon, label, name, view.screen == screen, compact).clicked() {
            actions.push(Action::GoTo(screen));
        }
    }
}

fn row(ui: &mut egui::Ui, icon: &str, label: &str, name: &str, selected: bool, compact: bool) -> egui::Response {
    let p = palette(ui);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    let fill = if selected {
        p.accent_soft
    } else if response.hovered() {
        p.hover
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(radius::CONTROL), fill);
    let color = if selected { p.accent } else { p.text };
    let text: egui::WidgetText = if compact {
        widgets::icon(icon, size::ICON_LARGE, color).into()
    } else {
        widgets::icon_text(icon, label, theme::regular(size::BODY), color).into()
    };
    let galley = text.into_galley(ui, Some(egui::TextWrapMode::Truncate), rect.width(), egui::TextStyle::Body);
    let x = if compact { rect.center().x - galley.size().x / 2.0 } else { rect.left() + space::S };
    ui.painter().galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0), galley, color);
    if response.has_focus() {
        ui.painter().rect_stroke(rect, CornerRadius::same(radius::CONTROL), ui.visuals().selection.stroke, egui::StrokeKind::Inside);
    }
    let label_text = name.to_owned();
    response.widget_info(move || egui::WidgetInfo::selected(Role::Tab, true, selected, &label_text));
    widgets::name(&response, Role::Tab, name);
    if compact { response.on_hover_text(label) } else { response }
}
