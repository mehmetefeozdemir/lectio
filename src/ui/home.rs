//! The list screens: Home (every session, newest first), Collections (the
//! courses), Trash (restorable for 30 days), and the Tasks placeholder.
//! Search has its own file (`search.rs`). Opening a session goes to the
//! workspace.

use eframe::egui::{self, CornerRadius, Key, RichText, Role, Sense, Stroke};
use lectio_core::collections::{HomeFilter, SessionSort};
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
        Screen::Collections => super::collections::page(ui, view, buffers, actions),
        Screen::Trash => trash(ui, view, actions),
        Screen::Tasks => super::tasks::page(ui, view, buffers, actions),
        Screen::Study => super::study::page(ui, view, buffers, actions),
        Screen::Search => super::search::page(ui, view, buffers, actions),
        Screen::Home | Screen::Workspace => home(ui, view, buffers, actions),
    }
}

pub(super) fn page_title(ui: &mut egui::Ui, title: &str, detail: &str, right: impl FnOnce(&mut egui::Ui)) {
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
    let (filter, sort) = (view.prefs.home_filter, view.prefs.home_sort);
    let any_sessions = !view.library.live_sessions().is_empty();
    let sessions = view.library.home_sessions(&filter, sort);
    let detail = match sessions.len() {
        1 => "1 session".to_owned(),
        n => format!("{n} sessions"),
    };
    let courses = view.library.courses();
    page_title(ui, "Sessions", &detail, |ui| {
        let new_session = !courses.is_empty() && widgets::primary_button(ui, icons::PLUS, "New session", true).clicked();
        due_chip(ui, view, actions);
        if new_session {
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
    if !any_sessions {
        let message = "Press \"New session\", then record the lecture and take notes as it goes.";
        widgets::empty_state(ui, icons::MICROPHONE, "No sessions yet", message, None);
        return;
    }
    filter_bar(ui, view, filter, sort, actions);
    if buffers.adding_collection.is_some_and(|with| with.is_some()) {
        super::collections::new_collection_field(ui, buffers, actions);
        ui.add_space(space::S);
    }
    if sessions.is_empty() {
        let shown = widgets::empty_state(ui, icons::FILTER, "No sessions match", "Nothing here matches this filter.", Some((icons::CLOSE, "Show all sessions")));
        if shown {
            actions.push(Action::SetHomeFilter(HomeFilter::All));
        }
        return;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (course, session) in sessions {
            session_card(ui, view, course, session, buffers, actions);
        }
    });
}

/// What a filter is called in the menu.
fn filter_name(view: &View, filter: HomeFilter) -> String {
    match filter {
        HomeFilter::All => "All sessions".into(),
        HomeFilter::Course(id) => view.library.course(id).map_or("A course".into(), |c| format!("Course: {}", c.name)),
        HomeFilter::Collection(id) => view.library.collection(id).map_or("A collection".into(), |c| format!("Collection: {}", c.name)),
        HomeFilter::Label(kind) => format!("Has a note labelled {}", view.library.labels().name(kind)),
    }
}

/// Show: all / one course / one collection / one label · Sort (ADR-026).
fn filter_bar(ui: &mut egui::Ui, view: &View, filter: HomeFilter, sort: SessionSort, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let mut choices = vec![HomeFilter::All];
    choices.extend(view.library.courses().iter().map(|c| HomeFilter::Course(c.id)));
    choices.extend(view.library.collections().iter().map(|c| HomeFilter::Collection(c.id)));
    choices.extend(LABEL_ORDER.into_iter().filter(|k| *k != LabelKind::Heading).map(HomeFilter::Label));
    ui.horizontal_wrapped(|ui| {
        ui.label(widgets::icon(icons::FILTER, size::ICON, p.text_muted));
        let combo = egui::ComboBox::from_id_salt("home-filter").selected_text(filter_name(view, filter)).show_ui(ui, |ui| {
            for choice in choices {
                if ui.selectable_label(choice == filter, filter_name(view, choice)).clicked() {
                    actions.push(Action::SetHomeFilter(choice));
                }
            }
        });
        widgets::name(&combo.response, Role::ComboBox, "Show sessions");
        ui.add_space(space::S);
        ui.label(widgets::icon(icons::SORT, size::ICON, p.text_muted));
        let combo = egui::ComboBox::from_id_salt("home-sort").selected_text(sort.name()).show_ui(ui, |ui| {
            for choice in SessionSort::ALL {
                if ui.selectable_label(choice == sort, choice.name()).clicked() {
                    actions.push(Action::SetHomeSort(choice));
                }
            }
        });
        widgets::name(&combo.response, Role::ComboBox, "Sort sessions");
    });
    ui.add_space(space::S);
}

/// "3 cards due" next to "New session" (Phase 4a); opens the Study page.
fn due_chip(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let due = view.study.due_cards;
    if due == 0 {
        return;
    }
    let text = format!("{due} card{} due", if due == 1 { "" } else { "s" });
    if widgets::neutral_chip(ui, Some(icons::DEFINITION), &text, true, "Review flashcards").clicked() {
        actions.push(Action::GoTo(Screen::Study));
    }
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
pub(super) fn session_card(
    ui: &mut egui::Ui,
    view: &View,
    course: &Course,
    session: &Session,
    buffers: &mut Buffers,
    actions: &mut Vec<Action>,
) {
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
                session_menu(ui, view, session, buffers, actions);
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

fn session_menu(ui: &mut egui::Ui, view: &View, session: &Session, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let menu = ui.menu_button(widgets::icon(icons::MORE, size::ICON, p.text_muted), |ui| {
        super::collections::menu_items(ui, view, session, buffers, actions);
        ui.separator();
        let text = widgets::icon_text(icons::TRASH, "Move to trash", theme::regular(size::CAPTION), p.danger_text);
        if ui.button(text).on_hover_text(format!("Restorable from Trash for {TRASH_DAYS} days")).clicked() {
            actions.push(Action::TrashSession(session.id));
        }
    });
    widgets::name(&menu.response, Role::Button, &format!("Actions for {}", session.title));
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
