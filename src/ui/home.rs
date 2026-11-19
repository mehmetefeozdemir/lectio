//! The list screens: Home (every session, newest first), Collections (the
//! courses), Trash (restorable for 30 days), and the Tasks and Search
//! placeholders. Opening a session goes to the workspace.

use eframe::egui::{self, CornerRadius, Key, RichText, Role, Sense, Stroke};
use lectio_core::trash::TRASH_DAYS;
use lectio_core::{Course, LabelKind, Session};

use super::theme::{self, palette, radius, size, space};
use super::{Buffers, View, format, icons, widgets};
use crate::action::{Action, Screen};
use crate::platform::now_unix_ms;

const DAY_MS: u64 = 86_400_000;
/// Labels in the order the input bar shows them.
pub const LABEL_ORDER: [LabelKind; 5] =
    [LabelKind::Heading, LabelKind::Important, LabelKind::Review, LabelKind::Task, LabelKind::Definition];

pub fn show(ui: &mut egui::Ui, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    match view.screen {
        Screen::Collections => collections(ui, view, buffers, actions),
        Screen::Trash => trash(ui, view, actions),
        Screen::Tasks => {
            let message = "Notes labelled Task will be collected here, across all sessions (Phase 4).";
            widgets::empty_state(ui, icons::TASKS, "Tasks are coming soon", message, None);
        }
        Screen::Search => {
            let message = "Searching every note and slide arrives in Phase 3.";
            widgets::empty_state(ui, icons::SEARCH, "Search is coming soon", message, None);
        }
        Screen::Home | Screen::Workspace => home(ui, view, buffers, actions),
    }
}

fn page_title(ui: &mut egui::Ui, title: &str, detail: &str, right: impl FnOnce(&mut egui::Ui)) {
    let p = palette(ui);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).font(theme::semibold(size::HEADING)).color(p.text));
            ui.label(RichText::new(detail).size(size::CAPTION).color(p.text_muted));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
    });
    ui.add_space(space::L);
}

fn home(ui: &mut egui::Ui, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let sessions = view.library.live_sessions();
    let detail = match sessions.len() {
        1 => "1 session".to_owned(),
        n => format!("{n} sessions"),
    };
    let courses = view.library.courses();
    page_title(ui, "Sessions", &detail, |ui| {
        if !courses.is_empty() && widgets::primary_button(ui, icons::PLUS, "New session", true).clicked() {
            match courses {
                [only] => actions.push(Action::AddSession(only.id)),
                _ => buffers.picking_course = !buffers.picking_course,
            }
        }
    });
    if buffers.picking_course {
        course_picker(ui, view, buffers, actions);
    }
    if courses.is_empty() {
        first_course(ui, buffers, actions);
        return;
    }
    if sessions.is_empty() {
        let message = "Press \"New session\", then record the lecture and take notes as it goes.";
        widgets::empty_state(ui, icons::MICROPHONE, "No sessions yet", message, None);
        return;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (course, session) in sessions {
            session_card(ui, view, course, session, actions);
        }
    });
}

/// "New session" with several courses: pick which one it belongs to.
fn course_picker(ui: &mut egui::Ui, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    widgets::card(ui, |ui| {
        ui.label(RichText::new("Which course is this session for?").color(p.text));
        ui.add_space(space::XS);
        ui.horizontal_wrapped(|ui| {
            for course in view.library.courses() {
                let label = format!("New session in {}", course.name);
                let button = ui.button(widgets::icon_text(icons::COLLECTIONS, &course.name, theme::regular(size::CAPTION), p.text));
                widgets::name(&button, Role::Button, &label);
                if button.clicked() {
                    actions.push(Action::AddSession(course.id));
                    buffers.picking_course = false;
                }
            }
            if widgets::icon_button(ui, icons::CLOSE, "Cancel", true).clicked() {
                buffers.picking_course = false;
            }
        });
    });
    ui.add_space(space::M);
}

fn first_course(ui: &mut egui::Ui, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let where_saved = if cfg!(target_arch = "wasm32") { "in this browser" } else { "on this computer" };
    let message = format!("Add a course, then record a lecture or import its slides. Notes stay {where_saved}.");
    if buffers.adding_course {
        ui.add_space(space::XXL);
        ui.vertical_centered(|ui| {
            ui.set_max_width(360.0);
            new_course_field(ui, buffers, actions);
        });
    } else if widgets::empty_state(ui, icons::DEFINITION, "Start your first course", &message, Some((icons::PLUS, "Add a course"))) {
        buffers.adding_course = true;
    }
}

pub fn new_course_field(ui: &mut egui::Ui, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let field = ui.add(
        egui::TextEdit::singleline(&mut buffers.new_course).hint_text("Course name, e.g. CSCA67").desired_width(f32::INFINITY),
    );
    widgets::name(&field, Role::TextInput, "Course name");
    if !field.has_focus() && !field.lost_focus() {
        field.request_focus();
    }
    if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
        actions.push(Action::AddCourse(buffers.new_course.clone()));
        buffers.adding_course = false;
    }
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        buffers.adding_course = false;
        buffers.new_course.clear();
    }
}

/// One session: title, course · date · length, and how many notes carry
/// each label. Click to open; "⋯" to move it to the trash.
fn session_card(ui: &mut egui::Ui, view: &View, course: &Course, session: &Session, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let frame = egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::CARD))
        .inner_margin(egui::Margin::symmetric(space::M as i8, space::S as i8 + 2));
    let inner = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(&session.title).font(theme::semibold(size::BODY)).color(p.text));
                let line = format!("{} · {}", course.name, format::session_details(session));
                ui.label(RichText::new(line).size(size::SMALL).color(p.text_muted));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                session_menu(ui, session, actions);
                label_counts(ui, view, session);
            });
        });
    });
    let rect = inner.response.rect;
    let response = ui.interact(rect, ui.scope_id().with(("open", session.id)), Sense::click());
    if response.hovered() {
        ui.painter().rect_stroke(rect, CornerRadius::same(radius::CARD), Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(rect, CornerRadius::same(radius::CARD), ui.visuals().selection.stroke, egui::StrokeKind::Inside);
    }
    widgets::name(&response, Role::ListItem, &format!("Session: {}", session.title));
    if response.on_hover_text("Open this session").clicked() {
        actions.push(Action::SelectSession(session.id));
    }
    ui.add_space(space::S);
}

fn label_counts(ui: &mut egui::Ui, view: &View, session: &Session) {
    let dark = ui.visuals().dark_mode;
    for kind in LABEL_ORDER.into_iter().rev() {
        let count = session.notes.iter().filter(|n| n.labels.contains(&kind)).count();
        if count == 0 {
            continue;
        }
        let colors = theme::label_colors(kind, dark);
        let text = widgets::icon_text(icons::label(kind), &count.to_string(), theme::mono(size::SMALL), colors.text);
        egui::Frame::new()
            .fill(colors.fill)
            .corner_radius(CornerRadius::same(radius::CONTROL))
            .inner_margin(egui::Margin::symmetric(space::XS as i8 + 2, 2))
            .show(ui, |ui| ui.label(text))
            .response
            .on_hover_text(format!("{count} × {}", view.library.labels().name(kind)));
    }
}

fn session_menu(ui: &mut egui::Ui, session: &Session, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let menu = ui.menu_button(widgets::icon(icons::MORE, size::ICON, p.text_muted), |ui| {
        let text = widgets::icon_text(icons::TRASH, "Move to trash", theme::regular(size::CAPTION), p.danger_text);
        if ui.button(text).on_hover_text(format!("Restorable from Trash for {TRASH_DAYS} days")).clicked() {
            actions.push(Action::TrashSession(session.id));
        }
    });
    widgets::name(&menu.response, Role::Button, &format!("Actions for {}", session.title));
}

fn collections(ui: &mut egui::Ui, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let detail = format!("{} course(s)", view.library.courses().len());
    page_title(ui, "Collections", &detail, |ui| {
        if widgets::subtle_button(ui, icons::PLUS, "New course", true).clicked() {
            buffers.adding_course = true;
        }
    });
    if buffers.adding_course {
        new_course_field(ui, buffers, actions);
        ui.add_space(space::M);
    }
    let live = view.library.live_sessions();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for course in view.library.courses() {
            ui.horizontal(|ui| {
                ui.label(widgets::icon_text(icons::COLLECTIONS, &course.name, theme::semibold(size::TITLE), p.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = format!("New session in {}", course.name);
                    if widgets::subtle_button(ui, icons::PLUS, &label, true).clicked() {
                        actions.push(Action::AddSession(course.id));
                    }
                });
            });
            ui.add_space(space::S);
            let sessions: Vec<_> = live.iter().filter(|(c, _)| c.id == course.id).collect();
            if sessions.is_empty() {
                ui.label(RichText::new("No sessions yet.").color(p.text_muted));
            }
            for (course, session) in sessions {
                session_card(ui, view, course, session, actions);
            }
            ui.add_space(space::L);
        }
    });
}

fn trash(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let detail = format!("Sessions here are deleted for good after {TRASH_DAYS} days.");
    page_title(ui, "Trash", &detail, |_| {});
    let trashed = view.library.trashed_sessions();
    if trashed.is_empty() {
        widgets::empty_state(ui, icons::TRASH, "Trash is empty", "Sessions you move to the trash appear here.", None);
        return;
    }
    let now = now_unix_ms();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (course, session) in trashed {
            widgets::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&session.title).font(theme::semibold(size::BODY)).color(p.text));
                        let line = format!("{} · {} · {}", course.name, format::session_details(session), days_left(session, now));
                        ui.label(RichText::new(line).size(size::SMALL).color(p.text_muted));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = format!("Restore {}", session.title);
                        let button = ui.button(widgets::icon_text(icons::UNDO, "Restore", theme::regular(size::CAPTION), p.text));
                        widgets::name(&button, Role::Button, &label);
                        if button.clicked() {
                            actions.push(Action::RestoreSession(session.id));
                        }
                    });
                });
            });
            ui.add_space(space::S);
        }
    });
}

/// "12 days left" before a trashed session is deleted for good.
fn days_left(session: &Session, now_ms: u64) -> String {
    let deleted = session.deleted_unix_ms.unwrap_or(now_ms);
    let expires = deleted + TRASH_DAYS * DAY_MS;
    match expires.saturating_sub(now_ms).div_ceil(DAY_MS) {
        0 | 1 => "deleted within a day".to_owned(),
        days => format!("{days} days left"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_left_counts_down_from_thirty() {
        let (lib, course) = lectio_core::Library::default().with_course("C").unwrap();
        let (lib, id) = lib.with_session(course, "S", 0).unwrap();
        let lib = lib.with_session_trashed(id, 0).unwrap();
        let trashed = lib.session(id).unwrap();
        assert_eq!(days_left(trashed, 0), "30 days left");
        assert_eq!(days_left(trashed, 29 * DAY_MS + 1), "deleted within a day");
        assert_eq!(days_left(trashed, 10 * DAY_MS), "20 days left");
    }
}
