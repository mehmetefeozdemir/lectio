//! The Tasks page (Phase 4a): every Task-labelled note in every course,
//! open ones first, with done, a due date, a course filter, and a jump to
//! the moment it was written.

use eframe::egui::{self, CornerRadius, RichText, Role, Stroke};
use lectio_core::tasks::{TaskFilter, TaskRow, tasks};
use lectio_core::timeline::format_offset;

use super::theme::{self, palette, radius, size, space};
use super::{Buffers, View, format, home, icons, widgets};
use crate::action::Action;

const DAY_MS: u64 = 86_400_000;

pub fn page(ui: &mut egui::Ui, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let filter = TaskFilter { course: buffers.task_course, include_done: buffers.task_show_done };
    let rows = tasks(view.library, &filter);
    let open = rows.iter().filter(|r| !r.done).count();
    home::page_title(ui, "Tasks", &format!("{open} open · notes labelled Task in every course"), |_| {});
    ui.horizontal(|ui| {
        let shown = buffers.task_course.and_then(|c| view.library.course(c)).map_or("All courses", |c| c.name.as_str());
        let combo = egui::ComboBox::from_id_salt("task-course").selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut buffers.task_course, None, "All courses");
            for course in view.library.courses() {
                ui.selectable_value(&mut buffers.task_course, Some(course.id), &course.name);
            }
        });
        widgets::name(&combo.response, Role::ComboBox, "Course filter");
        widgets::switch(ui, &mut buffers.task_show_done, "Show done tasks");
        ui.label(RichText::new("Show done").size(size::CAPTION).color(p.text));
    });
    ui.add_space(space::M);
    if rows.is_empty() {
        let message = "Label a note Task (Cmd/Ctrl+3) while taking notes, and it appears here.";
        widgets::empty_state(ui, icons::TASKS, "No tasks", message, None);
        return;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for row in &rows {
            task_row(ui, view, row, actions);
        }
    });
}

fn task_row(ui: &mut egui::Ui, view: &View, row: &TaskRow, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let overdue = !row.done && row.due_unix_ms.is_some_and(|d| d < view.now_ms);
    let frame = egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, if overdue { p.danger_text } else { p.border }))
        .corner_radius(CornerRadius::same(radius::CARD))
        .inner_margin(egui::Margin::symmetric(space::M as i8, space::S as i8));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let mut done = row.done;
            let check = ui.checkbox(&mut done, "");
            widgets::name(&check, Role::CheckBox, &format!("Done: {}", row.note.body));
            if check.changed() {
                actions.push(Action::SetTaskDone { session: row.session.id, note: row.note.id, done });
            }
            ui.vertical(|ui| {
                let text = if row.note.body.is_empty() { "(task)" } else { row.note.body.as_str() };
                let color = if row.done { p.text_muted } else { p.text };
                let mut rich = RichText::new(text).font(theme::regular(size::BODY)).color(color);
                if row.done {
                    rich = rich.strikethrough();
                }
                ui.add(egui::Label::new(rich).wrap());
                let place = format!("{} · {} · {}", row.course.name, row.session.title, format_offset(row.note.offset_ms));
                ui.label(RichText::new(place).size(size::SMALL).color(p.text_muted));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let open = format!("Open {} at {}", row.session.title, format_offset(row.note.offset_ms));
                if widgets::icon_button(ui, icons::PLAY, &open, true).clicked() {
                    actions.push(Action::OpenHit { session: row.session.id, offset_ms: Some(row.note.offset_ms), slide: row.note.slide });
                }
                due_menu(ui, view, row, overdue, actions);
            });
        });
    });
    ui.add_space(space::S);
}

/// "Due Nov 21" with quick choices (today, tomorrow, in a week, none).
fn due_menu(ui: &mut egui::Ui, view: &View, row: &TaskRow, overdue: bool, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let text = match row.due_unix_ms {
        Some(due) => format!("Due {}", format::short_date(due)),
        None => "No due date".to_owned(),
    };
    let color = if overdue { p.danger_text } else { p.text_muted };
    let menu = ui.menu_button(RichText::new(text).size(size::SMALL).color(color), |ui| {
        let today_end = (view.now_ms / DAY_MS + 1) * DAY_MS - 1;
        for (label, due) in [
            ("Today", Some(today_end)),
            ("Tomorrow", Some(today_end + DAY_MS)),
            ("In a week", Some(today_end + 7 * DAY_MS)),
            ("No due date", None),
        ] {
            if ui.button(label).clicked() {
                actions.push(Action::SetTaskDue { session: row.session.id, note: row.note.id, due_unix_ms: due });
                ui.close();
            }
        }
    });
    widgets::name(&menu.response, Role::Button, &format!("Due date: {}", row.note.body));
}
