//! Shared controls built from the design tokens (ADR-014): icon buttons with
//! tooltips and accessible names, primary buttons, pills, label chips, a
//! switch, and empty states.
//!
//! Every named control is also recorded in a per-frame list of
//! (name, rectangle). The browser test build publishes that list so tests can
//! find controls by name, because a canvas app has no DOM to search.

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Id, Rect, Response, RichText, Sense, Stroke, StrokeKind, WidgetInfo,
    Role,
};
use lectio_core::{LabelKind, LabelSet};

use super::theme::{self, palette, radius, size, space};
use super::icons;

const NAMED_WIDGETS: &str = "lectio-named-widgets";

/// Controls drawn this frame, by accessible name (for tests). Only the
/// browser build publishes them.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn named_widgets(ctx: &egui::Context) -> Vec<(String, Rect)> {
    ctx.data(|d| d.get_temp::<Vec<(String, Rect)>>(Id::unique(NAMED_WIDGETS))).unwrap_or_default()
}

/// Call at the start of each frame.
pub fn clear_named_widgets(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(Id::unique(NAMED_WIDGETS), Vec::<(String, Rect)>::new()));
}

/// Gives `response` an accessible name (read by screen readers on desktop)
/// and records it for tests.
pub fn name(response: &Response, kind: Role, name: &str) {
    let enabled = response.enabled();
    let label = name.to_owned();
    response.widget_info(move || WidgetInfo::labeled(kind, enabled, &label));
    let entry = (name.to_owned(), response.rect);
    response.ctx.data_mut(|d| d.get_temp_mut_or_default::<Vec<(String, Rect)>>(Id::unique(NAMED_WIDGETS)).push(entry));
}

/// "icon + text" as one piece of text: the icon in the icon font, the text in
/// `font` (two fonts can't share one fallback list; see `theme::fonts`).
pub fn icon_text(icon: &str, text: &str, font: egui::FontId, color: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let icon_format = TextFormat { font_id: theme::icon_font(font.size + 1.0), color, valign: egui::Align::Center, ..Default::default() };
    job.append(icon, 0.0, icon_format);
    job.append(text, font.size * 0.45, TextFormat { font_id: font, color, valign: egui::Align::Center, ..Default::default() });
    job
}

/// An icon on its own, in the icon font.
pub fn icon(icon: &str, size: f32, color: Color32) -> RichText {
    RichText::new(icon).font(theme::icon_font(size)).color(color)
}

/// A frameless icon button with a tooltip and an accessible name.
pub fn icon_button(ui: &mut egui::Ui, icon: &str, label: &str, enabled: bool) -> Response {
    icon_button_sized(ui, icon, label, enabled, size::ICON)
}

pub fn icon_button_sized(ui: &mut egui::Ui, icon: &str, label: &str, enabled: bool, icon_size: f32) -> Response {
    let text = self::icon(icon, icon_size, palette(ui).text_muted);
    let button = egui::Button::new(text).frame_when_inactive(false).min_size(egui::vec2(28.0, 28.0));
    let response = ui.add_enabled(enabled, button).on_hover_text(label);
    name(&response, Role::Button, label);
    response
}

/// An icon + text button with the subtle (grey) frame.
pub fn subtle_button(ui: &mut egui::Ui, icon: &str, label: &str, enabled: bool) -> Response {
    let job = icon_text(icon, label, theme::regular(size::CAPTION), palette(ui).text);
    let response = ui.add_enabled(enabled, egui::Button::new(job));
    name(&response, Role::Button, label);
    response
}

/// The one accent-coloured call to action on a screen.
pub fn primary_button(ui: &mut egui::Ui, icon: &str, label: &str, enabled: bool) -> Response {
    let p = palette(ui);
    let text = icon_text(icon, label, theme::semibold(size::CAPTION), p.on_accent);
    let button = egui::Button::new(text).fill(p.accent).corner_radius(CornerRadius::same(radius::CONTROL));
    let response = ui.add_enabled(enabled, button);
    name(&response, Role::Button, label);
    response
}

/// A rounded timestamp pill in mono. Clickable when `on_click` is wanted.
pub fn time_pill(ui: &mut egui::Ui, text: &str, clickable: bool, label: &str) -> Response {
    let p = palette(ui);
    let rich = RichText::new(text).font(theme::mono(size::CAPTION)).color(p.text_muted);
    let button = egui::Button::new(rich)
        .fill(p.surface_alt)
        .corner_radius(CornerRadius::same(radius::CARD))
        .sense(if clickable { Sense::click() } else { Sense::hover() });
    let response = ui.add(button);
    if clickable {
        name(&response, Role::Button, label);
        return response.on_hover_text(label);
    }
    response
}

/// A label chip: its icon and name on the label's soft colour.
pub fn label_chip(ui: &mut egui::Ui, labels: &LabelSet, kind: LabelKind) -> Response {
    let colors = theme::label_colors(kind, ui.visuals().dark_mode);
    let text = icon_text(icons::label(kind), labels.name(kind), theme::regular(size::SMALL), colors.text);
    chip(ui, text, colors.fill)
}

/// A neutral chip (e.g. the slide a note links to), optionally clickable.
pub fn neutral_chip(ui: &mut egui::Ui, chip_icon: Option<&str>, text: &str, clickable: bool, label: &str) -> Response {
    let p = palette(ui);
    let font = theme::regular(size::SMALL);
    let content: egui::WidgetText = match chip_icon {
        Some(i) => icon_text(i, text, font, p.text_muted).into(),
        None => RichText::new(text).font(font).color(p.text_muted).into(),
    };
    let button = egui::Button::new(content)
        .fill(p.surface_alt)
        .corner_radius(CornerRadius::same(radius::CONTROL))
        .small()
        .sense(if clickable { Sense::click() } else { Sense::hover() });
    let response = ui.add(button);
    if clickable {
        name(&response, Role::Button, label);
        return response.on_hover_text(label);
    }
    response
}

fn chip(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>, fill: Color32) -> Response {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(radius::CONTROL))
        .inner_margin(egui::Margin::symmetric(space::S as i8 - 2, 2))
        .show(ui, |ui| ui.add(egui::Label::new(text).extend()))
        .response
}

/// An on/off switch. Returns the response; flips `on` when clicked.
pub fn switch(ui: &mut egui::Ui, on: &mut bool, label: &str) -> Response {
    let p = palette(ui);
    let size = egui::vec2(34.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let track = if *on { p.accent } else { p.track };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(10), track);
    let x = egui::lerp(rect.left() + 10.0..=rect.right() - 10.0, t);
    painter.circle_filled(egui::pos2(x, rect.center().y), 7.0, if *on { p.on_accent } else { p.surface });
    if response.has_focus() {
        painter.rect_stroke(rect.expand(2.0), CornerRadius::same(12), ui.visuals().selection.stroke, StrokeKind::Outside);
    }
    let checked = *on;
    let text = label.to_owned();
    response.widget_info(move || WidgetInfo::selected(Role::CheckBox, true, checked, &text));
    name(&response, Role::CheckBox, label);
    response
}

/// A short message with an icon and at most one primary action.
pub fn empty_state(ui: &mut egui::Ui, icon: &str, title: &str, message: &str, action: Option<(&str, &str)>) -> bool {
    let p = palette(ui);
    let mut clicked = false;
    ui.add_space(space::XXL * 2.0);
    ui.vertical_centered(|ui| {
        ui.label(self::icon(icon, 36.0, p.text_muted));
        ui.add_space(space::S);
        ui.label(RichText::new(title).font(theme::semibold(size::TITLE)).color(p.text));
        ui.add_space(space::XS);
        ui.label(RichText::new(message).color(p.text_muted));
        if let Some((icon, label)) = action {
            ui.add_space(space::L);
            clicked = primary_button(ui, icon, label, true).clicked();
        }
    });
    clicked
}

/// A section heading in small caps-like muted text.
pub fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).font(theme::semibold(size::SMALL)).color(palette(ui).text_muted));
}

/// A card: surface fill, 1 px border, large radius.
pub fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) -> Response {
    let p = palette(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::CARD))
        .inner_margin(egui::Margin::same(space::M as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        })
        .response
}

/// Paints a label's marker shape (diamond, circle, square, triangle) so
/// labels on the timeline differ by shape as well as colour.
pub fn paint_label_marker(painter: &egui::Painter, kind: LabelKind, center: egui::Pos2, r: f32, color: Color32) {
    use egui::{Pos2, Shape};
    let pts = |v: &[(f32, f32)]| v.iter().map(|(x, y)| Pos2::new(center.x + x * r, center.y + y * r)).collect();
    match kind {
        LabelKind::Important => {
            painter.add(Shape::convex_polygon(pts(&[(0.0, -1.2), (1.2, 0.0), (0.0, 1.2), (-1.2, 0.0)]), color, Stroke::NONE));
        }
        LabelKind::Review => {
            painter.circle_filled(center, r, color);
        }
        LabelKind::Task => {
            painter.rect_filled(Rect::from_center_size(center, egui::vec2(r * 1.8, r * 1.8)), 1.0, color);
        }
        LabelKind::Heading => {
            painter.rect_filled(Rect::from_center_size(center, egui::vec2(r * 2.4, r * 0.9)), 1.0, color);
        }
        LabelKind::Definition => {
            painter.add(Shape::convex_polygon(pts(&[(0.0, -1.2), (1.1, 0.9), (-1.1, 0.9)]), color, Stroke::NONE));
        }
    }
}

/// Draws `text` centred in `rect` (used for icons inside painted shapes).
pub fn paint_centered(painter: &egui::Painter, rect: Rect, text: &str, font: egui::FontId, color: Color32) {
    painter.text(rect.center(), Align2::CENTER_CENTER, text, font, color);
}

/// A wrapped label whose words can be double-clicked (to look them up,
/// ADR-022). Returns the response (a single click still works) and the
/// double-clicked word, if any.
pub fn word_label(ui: &mut egui::Ui, text: RichText, sense: Sense) -> (Response, Option<String>) {
    let (pos, galley, response) = egui::Label::new(text).wrap().sense(sense | Sense::click()).layout_in_ui(ui);
    ui.painter().galley(pos, galley.clone(), ui.visuals().text_color());
    let word = response
        .double_clicked()
        .then(|| response.interact_pointer_pos())
        .flatten()
        .and_then(|pointer| word_around(galley.text(), galley.cursor_from_pos(pointer - pos).index.0));
    (response.on_hover_text("Double-click a word to look it up"), word)
}

/// The word (letters, digits, `-`, `'`) at character `index` of `text`.
pub fn word_around(text: &str, index: usize) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let part_of_word = |c: char| c.is_alphanumeric() || c == '-' || c == '\'';
    let at = index.min(chars.len().saturating_sub(1));
    let at = if chars.get(at).is_some_and(|c| part_of_word(*c)) { at } else { at.checked_sub(1)? };
    if !chars.get(at).is_some_and(|c| part_of_word(*c)) {
        return None;
    }
    let start = (0..=at).rev().take_while(|&i| part_of_word(chars[i])).last()?;
    let end = (at..chars.len()).take_while(|&i| part_of_word(chars[i])).last()?;
    let word: String = chars[start..=end].iter().collect::<String>().trim_matches(['-', '\'']).to_owned();
    (!word.is_empty()).then_some(word)
}

#[cfg(test)]
mod tests {
    use super::word_around;

    #[test]
    fn the_word_under_the_pointer_is_found() {
        let text = "BFS uses a queue, not a stack.";
        assert_eq!(word_around(text, 12).as_deref(), Some("queue"));
        assert_eq!(word_around(text, 16).as_deref(), Some("queue"), "just after the word");
        assert_eq!(word_around(text, 0).as_deref(), Some("BFS"));
        assert_eq!(word_around("depth-first search", 3).as_deref(), Some("depth-first"));
        assert_eq!(word_around(" , ", 1), None);
    }
}
