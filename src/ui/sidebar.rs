//! Left panel: courses, their sessions, and the "add course" box.

use eframe::egui;

use super::{Buffers, View};
use crate::action::Action;

pub fn show(ui: &mut egui::Ui, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    ui.add_space(6.0);
    ui.heading("Courses");
    if view.busy_recording {
        ui.label(egui::RichText::new("Stop the recording to switch sessions.").weak().small());
    }
    ui.add_space(4.0);

    egui::ScrollArea::vertical().auto_shrink([false, true]).max_height(ui.available_height() - 90.0).show(
        ui,
        |ui| {
            if view.library.courses().is_empty() {
                ui.label(egui::RichText::new("No courses yet.").weak());
            }
            for course in view.library.courses() {
                course_block(ui, view, course, actions);
            }
        },
    );

    ui.separator();
    add_course_box(ui, buffers, actions);
}

fn course_block(ui: &mut egui::Ui, view: &View, course: &lectio_core::Course, actions: &mut Vec<Action>) {
    let contains_selected = view.selected.is_some_and(|s| course.sessions.iter().any(|x| x.id == s));
    egui::CollapsingHeader::new(egui::RichText::new(&course.name).strong())
        .id_salt(course.id)
        .default_open(true)
        .open(contains_selected.then_some(true))
        .show(ui, |ui| {
            for session in &course.sessions {
                let is_selected = view.selected == Some(session.id);
                let status = if session.recording.is_some() { "recorded" } else { "not recorded" };
                let response = ui
                    .add_enabled(!view.busy_recording, egui::Button::selectable(is_selected, &session.title))
                    .on_hover_text(format!("{} notes, {status}", session.notes.len()));
                if response.clicked() {
                    actions.push(Action::SelectSession(session.id));
                }
            }
            if ui.button("+ New session").on_hover_text("Add a session for one lecture").clicked() {
                actions.push(Action::AddSession(course.id));
            }
        });
}

fn add_course_box(ui: &mut egui::Ui, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    ui.label("New course");
    ui.horizontal(|ui| {
        let field = ui.add(
            egui::TextEdit::singleline(&mut buffers.new_course)
                .hint_text("e.g. CSCA67")
                .desired_width(ui.available_width() - 60.0),
        );
        let submitted = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button("Add").clicked() || submitted {
            actions.push(Action::AddCourse(buffers.new_course.clone()));
        }
    });
}
