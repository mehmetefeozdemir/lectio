//! The Collections page (Phase 4b, ADR-026): the user's own collections of
//! sessions across courses (create, rename, delete; sessions are added and
//! removed from each session's "⋯" menu), then the courses as before.

use eframe::egui::{self, Key, RichText, Role};
use lectio_core::collections::Collection;
use lectio_core::{CollectionId, Session, SessionId};

use super::home::{new_course_field, page_title, session_card};
use super::theme::{self, palette, size, space};
use super::{Buffers, View, icons, widgets};
use crate::action::Action;

pub fn page(ui: &mut egui::Ui, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let detail = format!("{} collection(s) · {} course(s)", view.library.collections().len(), view.library.courses().len());
    page_title(ui, "Collections", &detail, |ui| {
        if widgets::subtle_button(ui, icons::PLUS, "New course", true).clicked() {
            buffers.adding_course = true;
        }
        if widgets::subtle_button(ui, icons::COLLECTION, "New collection", true).clicked() {
            buffers.adding_collection = Some(None);
        }
    });
    if buffers.adding_course {
        new_course_field(ui, buffers, actions);
        ui.add_space(space::M);
    }
    if buffers.adding_collection == Some(None) {
        new_collection_field(ui, buffers, actions);
        ui.add_space(space::M);
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // A session can be listed under a collection and under its course: each
        // section gets its own id scope, so the two cards don't share widget ids.
        for collection in view.library.collections() {
            ui.push_id(collection.id, |ui| collection_section(ui, view, collection, buffers, actions));
        }
        if view.library.collections().is_empty() {
            let p = palette(ui);
            let hint = "Collections group sessions from any course, like \"Midterm 1\". Make one here, or from a session's ⋯ menu.";
            ui.add(egui::Label::new(RichText::new(hint).color(p.text_muted)).wrap());
            ui.add_space(space::L);
        }
        courses(ui, view, buffers, actions);
    });
}

/// "Collection name" field; Enter creates it (with the session, if any).
pub fn new_collection_field(ui: &mut egui::Ui, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let with = buffers.adding_collection.flatten();
    let field = ui.add(
        egui::TextEdit::singleline(&mut buffers.new_collection).hint_text("Collection name, e.g. Midterm 1").desired_width(f32::INFINITY),
    );
    widgets::name(&field, Role::TextInput, "Collection name");
    if !field.has_focus() && !field.lost_focus() {
        field.request_focus();
    }
    if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
        actions.push(Action::NewCollection { name: buffers.new_collection.clone(), with });
    }
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        buffers.adding_collection = None;
        buffers.new_collection.clear();
    }
}

fn collection_section(ui: &mut egui::Ui, view: &View, collection: &Collection, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let renaming = buffers.renaming_collection.as_ref().is_some_and(|(id, _)| *id == collection.id);
    ui.horizontal(|ui| {
        if renaming {
            rename_field(ui, collection.id, buffers, actions);
        } else {
            let title = ui.label(widgets::icon_text(icons::COLLECTION, &collection.name, theme::semibold(size::TITLE), p.text));
            widgets::name(&title, Role::Heading, &format!("Collection: {}", collection.name));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let delete = format!("Delete the collection {}", collection.name);
            if widgets::icon_button(ui, icons::TRASH, &delete, true).clicked() {
                actions.push(Action::DeleteCollection(collection.id));
            }
            let rename = format!("Rename the collection {}", collection.name);
            if !renaming && widgets::icon_button(ui, icons::EDIT, &rename, true).clicked() {
                buffers.renaming_collection = Some((collection.id, collection.name.clone()));
            }
        });
    });
    ui.add_space(space::S);
    let sessions = view.library.collection_sessions(collection.id);
    if sessions.is_empty() {
        ui.label(RichText::new("No sessions yet. Add one from a session's ⋯ menu.").color(p.text_muted));
    }
    for id in sessions {
        if let (Some(course), Some(session)) = (view.library.course_of(id), view.library.session(id)) {
            session_card(ui, view, course, session, buffers, actions);
        }
    }
    ui.add_space(space::L);
}

fn rename_field(ui: &mut egui::Ui, id: CollectionId, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let Some((_, text)) = &mut buffers.renaming_collection else { return };
    let field = ui.add(egui::TextEdit::singleline(text).desired_width(280.0));
    widgets::name(&field, Role::TextInput, "New collection name");
    if !field.has_focus() && !field.lost_focus() {
        field.request_focus();
    }
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        buffers.renaming_collection = None;
    } else if field.lost_focus() {
        actions.push(Action::RenameCollection(id, text.clone()));
        buffers.renaming_collection = None;
    }
}

fn courses(ui: &mut egui::Ui, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let live = view.library.live_sessions();
    for course in view.library.courses() {
        ui.push_id(course.id, |ui| course_section(ui, view, course, &live, buffers, actions));
    }
}

fn course_section(
    ui: &mut egui::Ui,
    view: &View,
    course: &lectio_core::Course,
    live: &[(&lectio_core::Course, &Session)],
    buffers: &mut Buffers,
    actions: &mut Vec<Action>,
) {
    let p = palette(ui);
    ui.horizontal(|ui| {
        let title = ui.label(widgets::icon_text(icons::COLLECTIONS, &course.name, theme::semibold(size::TITLE), p.text));
        widgets::name(&title, Role::Heading, &format!("Course: {}", course.name));
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
        session_card(ui, view, course, session, buffers, actions);
    }
    ui.add_space(space::L);
}

/// The collection part of a session's "⋯" menu: one tick per collection,
/// and "New collection with this session…".
pub fn menu_items(ui: &mut egui::Ui, view: &View, session: &Session, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    widgets::section_label(ui, "Collections");
    for collection in view.library.collections() {
        let mut inside = collection.sessions.contains(&session.id);
        let text = widgets::icon_text(icons::COLLECTION, &collection.name, theme::regular(size::CAPTION), p.text);
        let check = ui.checkbox(&mut inside, text);
        widgets::name(&check, Role::CheckBox, &format!("In collection {}", collection.name));
        if check.changed() {
            actions.push(toggle(collection.id, session.id, inside));
        }
    }
    let new = widgets::icon_text(icons::PLUS, "New collection with this session…", theme::regular(size::CAPTION), p.text);
    if ui.button(new).clicked() {
        buffers.adding_collection = Some(Some(session.id));
        buffers.new_collection.clear();
    }
}

fn toggle(collection: CollectionId, session: SessionId, add: bool) -> Action {
    if add { Action::AddToCollection { collection, session } } else { Action::RemoveFromCollection { collection, session } }
}
