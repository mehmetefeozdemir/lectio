//! The slide panel beside the notes: import a PDF, page through it, and
//! choose whether new notes link to the slide on screen.

use eframe::egui;
use lectio_core::Session;
use lectio_core::material::SlideDeck;

use super::View;
use crate::action::Action;
use crate::material::PageView;

pub fn show(ui: &mut egui::Ui, view: &View, session: &Session, actions: &mut Vec<Action>) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.heading("Slides");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let import = ui
                .add_enabled(view.can_import, egui::Button::new("Import PDF…"))
                .on_hover_text("Add the lecture's slides (a PDF) to this session");
            if import.clicked() {
                actions.push(Action::ImportSlides);
            }
        });
    });
    if view.material.is_importing() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Waiting for the file…");
        });
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let shown = view.material.viewer.deck.and_then(|d| session.decks.iter().find(|x| x.id == d));
        match shown {
            Some(deck) => deck_view(ui, view, session, deck, actions),
            None => deck_list(ui, session, actions),
        }
    });
}

fn deck_list(ui: &mut egui::Ui, session: &Session, actions: &mut Vec<Action>) {
    if session.decks.is_empty() {
        let hint = "Import the lecture's PDF to see its slides here. Notes you take can then link to the slide on screen.";
        ui.label(egui::RichText::new(hint).weak());
        return;
    }
    for deck in &session.decks {
        if ui.button(&deck.title).clicked() {
            actions.push(Action::ShowDeck(deck.id));
        }
    }
}

fn deck_view(ui: &mut egui::Ui, view: &View, session: &Session, deck: &SlideDeck, actions: &mut Vec<Action>) {
    let page = view.material.viewer.page;
    if session.decks.len() > 1 {
        egui::ComboBox::from_id_salt("deck-picker").selected_text(&deck.title).show_ui(ui, |ui| {
            for other in &session.decks {
                if ui.selectable_label(other.id == deck.id, &other.title).clicked() {
                    actions.push(Action::ShowDeck(other.id));
                }
            }
        });
    } else {
        ui.label(egui::RichText::new(&deck.title).strong());
    }

    ui.horizontal(|ui| {
        if ui.add_enabled(page > 1, egui::Button::new("Previous")).on_hover_text("Page Up").clicked() {
            actions.push(Action::TurnSlide(-1));
        }
        ui.label(format!("Slide {page} of {}", deck.page_count));
        let next = egui::Button::new("Next");
        if ui.add_enabled(page < deck.page_count, next).on_hover_text("Page Down").clicked() {
            actions.push(Action::TurnSlide(1));
        }
    });

    match view.material.page(deck.id, page) {
        PageView::Ready(texture) => {
            let image = egui::Image::from_texture(texture)
                .max_width(ui.available_width())
                .alt_text(format!("{}, slide {page} of {}", deck.title, deck.page_count));
            ui.add(image);
        }
        PageView::Loading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Showing the slide…");
            });
        }
        PageView::Failed(problem) => {
            ui.label(egui::RichText::new(problem).weak());
        }
    }

    let on_this_slide = |n: &&lectio_core::Note| n.slide.is_some_and(|l| l.deck == deck.id && l.page == page);
    let linked = session.notes.iter().filter(on_this_slide).count();
    if linked > 0 {
        ui.label(egui::RichText::new(format!("{linked} note(s) link to this slide.")).weak());
    }
    let mut link_new = view.material.viewer.link_new_notes;
    let toggle = ui
        .checkbox(&mut link_new, "Link new notes to this slide")
        .on_hover_text("Notes, photos and sketches you add now remember which slide was on screen");
    if toggle.changed() {
        actions.push(Action::SetLinkNewNotes(link_new));
    }
    ui.add_space(8.0);
    let remove = ui
        .small_button("Remove these slides")
        .on_hover_text("Your notes stay; only their links to these slides are removed");
    if remove.clicked() {
        actions.push(Action::RemoveDeck(deck.id));
    }
}
