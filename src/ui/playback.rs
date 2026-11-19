//! The session title line in the workspace toolbar (inline rename), and the
//! player row of the input bar (play/pause, ±10 s, time, speed, sound, timeline).

use eframe::egui::{self, Key, Modifiers, RichText, Sense, Stroke, Role};
use lectio_core::Session;
use lectio_core::stretch::{SPEEDS, remaining_at_speed_ms};
use lectio_core::timeline::format_offset;

use super::theme::{self, palette, size};
use super::{Buffers, SEEK_STEP_MS, SPEED_KEYS, View, format, icons, shortcut_hint, timeline, widgets};
use crate::action::Action;

const BIG_BUTTON: f32 = 40.0;
/// The course/date line only shows when the toolbar has room for it.
const DETAILS_MIN_WIDTH: f32 = 520.0;

/// One line in the toolbar: the title (click or the pencil to rename), then
/// course, date and length.
pub fn header(ui: &mut egui::Ui, view: &View, session: &Session, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let editing = buffers.title_edit.as_ref().is_some_and(|(id, _)| *id == session.id);
    if editing {
        return title_editor(ui, session, buffers, actions);
    }
    let title = ui.add(
        egui::Label::new(RichText::new(&session.title).font(theme::semibold(size::TITLE)).color(p.text))
            .truncate()
            .sense(Sense::click()),
    );
    let pencil = widgets::icon_button(ui, icons::EDIT, "Rename session", !view.busy_recording);
    if title.on_hover_text("Click to rename").clicked() || pencil.clicked() {
        buffers.title_edit = Some((session.id, session.title.clone()));
    }
    if ui.available_width() > DETAILS_MIN_WIDTH {
        let course = view.library.course_of(session.id).map(|c| c.name.as_str()).unwrap_or("");
        let details = format!("{course} · {}", format::session_details(session));
        ui.label(RichText::new(details).size(size::CAPTION).color(p.text_muted));
    }
}

fn title_editor(ui: &mut egui::Ui, session: &Session, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let Some((_, text)) = &mut buffers.title_edit else { return };
    let field = ui.add(egui::TextEdit::singleline(text).font(theme::semibold(size::TITLE)).desired_width(ui.available_width().min(420.0)));
    widgets::name(&field, Role::TextInput, "Session title");
    if !field.has_focus() && !field.lost_focus() {
        field.request_focus();
    }
    let cancelled = ui.input(|i| i.key_pressed(Key::Escape));
    if cancelled {
        buffers.title_edit = None;
    } else if field.lost_focus() {
        actions.push(Action::RenameSession(session.id, text.clone()));
        buffers.title_edit = None;
    }
}

/// The player: round play/pause, ±10 s, the time, speed, sound filters, and
/// the timeline.
pub fn bar(ui: &mut egui::Ui, view: &View, session: &Session, actions: &mut Vec<Action>) {
    let duration = view.duration_ms.unwrap_or(0);
    ui.horizontal(|ui| {
        play_button(ui, view, actions);
        let back = format!("Back 10 s ({})", shortcut_hint(ui.ctx(), Modifiers::COMMAND, Key::OpenBracket));
        if widgets::icon_button_sized(ui, icons::BACK, &back, view.can_play, size::ICON_LARGE).clicked() {
            actions.push(Action::SeekBy(-SEEK_STEP_MS));
        }
        let forward = format!("Forward 10 s ({})", shortcut_hint(ui.ctx(), Modifiers::COMMAND, Key::CloseBracket));
        if widgets::icon_button_sized(ui, icons::FORWARD, &forward, view.can_play, size::ICON_LARGE).clicked() {
            actions.push(Action::SeekBy(SEEK_STEP_MS));
        }
        let p = palette(ui);
        ui.label(RichText::new(format_offset(view.offset_ms)).font(theme::mono(size::BODY)).color(p.text));
        ui.label(RichText::new(format!("/ {}", format_offset(duration))).font(theme::mono(size::BODY)).color(p.text_muted));
        speed_menu(ui, view, actions);
        sound_menu(ui, view, actions);
        if let Some(seek) = timeline::show(ui, view, session, duration) {
            actions.push(seek);
        }
    });
    if let Some(problem) = view.playback_problem {
        ui.label(RichText::new(problem).size(size::SMALL).color(palette(ui).text_muted));
    }
}

/// "1.25×": the speed steps, with the time left at the chosen speed.
fn speed_menu(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let current = view.prefs.speed_percent;
    let label = format!("{}×", speed_text(current));
    let text = widgets::icon_text(icons::SPEED, &label, theme::mono(size::CAPTION), p.text);
    let menu = ui.menu_button(text, |ui| {
        widgets::section_label(ui, "Playback speed");
        for speed in SPEEDS {
            if ui.selectable_label(speed == current, format!("{}×", speed_text(speed))).clicked() {
                actions.push(Action::SetSpeed(speed));
                ui.close();
            }
        }
        let left = remaining_at_speed_ms(view.offset_ms, view.duration_ms.unwrap_or(0), current);
        ui.label(RichText::new(format!("{} left at this speed", format_offset(left))).size(size::SMALL).color(p.text_muted));
    });
    widgets::name(&menu.response, Role::Button, "Playback speed");
    let slower = shortcut_hint(ui.ctx(), SPEED_KEYS.0, SPEED_KEYS.1);
    let faster = shortcut_hint(ui.ctx(), SPEED_KEYS.0, SPEED_KEYS.2);
    menu.response.on_hover_text(format!("Playback speed; the voice keeps its pitch ({slower} slower, {faster} faster)"));
}

fn speed_text(percent: u16) -> String {
    let value = f32::from(percent) / 100.0;
    match () {
        _ if percent.is_multiple_of(100) => format!("{value:.0}"),
        _ if percent.is_multiple_of(10) => format!("{value:.1}"),
        _ => format!("{value:.2}"),
    }
}

/// Noise reduction, voice boost, normalise, and the cleaned copy.
fn sound_menu(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let filters = view.prefs.filters;
    let color = if filters.any() { p.accent } else { p.text_muted };
    let menu = ui.menu_button(widgets::icon(icons::SOUND, size::ICON_LARGE, color), |ui| {
        widgets::section_label(ui, "Sound");
        let mut next = filters;
        let mut changed = false;
        let options = [
            (&mut next.noise_reduction, "Noise reduction", "Quietens hiss and hum in the pauses between words"),
            (&mut next.voice_boost, "Voice boost", "Cuts low rumble and lifts the frequencies that make speech clear"),
            (&mut next.normalise, "Normalise volume", "Brings quiet recordings up to a comfortable level"),
        ];
        for (on, name, help) in options {
            let check = ui.checkbox(on, name).on_hover_text(help);
            widgets::name(&check, Role::CheckBox, name);
            changed |= check.changed();
        }
        if changed {
            actions.push(Action::SetFilters(next));
        }
        ui.separator();
        let busy = view.cleaning;
        let save = ui.add_enabled(filters.any() && !busy && view.can_import, egui::Button::new("Save cleaned copy"));
        widgets::name(&save, Role::Button, "Save cleaned copy");
        if save.on_hover_text("Saves the recording with these filters applied; the original is kept").clicked() {
            actions.push(Action::SaveCleanedCopy);
            ui.close();
        }
        if view.has_cleaned {
            let mut on = view.prefs.play_cleaned;
            if ui.checkbox(&mut on, "Play the cleaned copy").changed() {
                actions.push(Action::SetPlayCleaned(on));
            }
        }
    });
    widgets::name(&menu.response, Role::Button, "Sound filters");
    menu.response.on_hover_text("Sound: noise reduction, voice boost, normalise");
}

fn play_button(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let (label, icon) = if view.playing { ("Pause", icons::PAUSE) } else { ("Play", icons::PLAY) };
    let sense = if view.can_play { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(BIG_BUTTON, BIG_BUTTON), sense);
    let painter = ui.painter();
    let fill = if view.can_play { p.accent } else { p.track };
    painter.circle_filled(rect.center(), BIG_BUTTON / 2.0, fill);
    widgets::paint_centered(painter, rect, icon, theme::icon_font(20.0), if view.can_play { p.on_accent } else { p.text_muted });
    if response.has_focus() {
        painter.circle_stroke(rect.center(), BIG_BUTTON / 2.0 + 3.0, Stroke::new(2.0, p.accent));
    }
    let hint = shortcut_hint(ui.ctx(), Modifiers::COMMAND, Key::R);
    widgets::name(&response, Role::Button, label);
    if response.on_hover_text(format!("{label} ({hint})")).clicked() && view.can_play {
        actions.push(Action::TogglePlay);
    }
}
