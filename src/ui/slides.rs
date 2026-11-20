//! The slides panel: the deck with a page-number strip, a page-search box
//! (Phase 3), the page (zoomable, or full screen), page arrows, "Post this
//! slide", and the "link new notes" switch.

use eframe::egui::{self, CornerRadius, RichText, Stroke, Role};
use lectio_core::Session;
use lectio_core::material::SlideDeck;

use super::theme::{self, palette, radius, size, space};
use super::{Buffers, View, icons, widgets};
use lectio_core::search::search_deck;
use crate::action::Action;
use crate::material::PageView;

const ZOOM_STEP: f32 = 0.25;
const STRIP_BUTTON: f32 = 30.0;

pub fn show(ui: &mut egui::Ui, view: &View, session: &Session, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    tabs(ui, view, actions);
    ui.add_space(space::M);
    if view.material.is_importing() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new("Waiting for the file…").color(palette(ui).text_muted));
        });
    }
    let shown = view.material.viewer.deck.and_then(|d| session.decks.iter().find(|x| x.id == d));
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| match shown {
        Some(deck) => deck_view(ui, view, session, deck, buffers, actions),
        None if session.decks.is_empty() => {
            let action = view.can_import.then_some((icons::PDF, "Import PDF"));
            let message = "Import the lecture's PDF to see its slides beside your notes.";
            if widgets::empty_state(ui, icons::SLIDES, "No slides yet", message, action) {
                actions.push(Action::ImportSlides);
            }
        }
        None => {
            for deck in &session.decks {
                if widgets::subtle_button(ui, icons::PDF, &deck.title, true).clicked() {
                    actions.push(Action::ShowDeck(deck.id));
                }
            }
        }
    });
}

fn tabs(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let p = palette(ui);
    ui.horizontal(|ui| {
        let tab = ui.label(widgets::icon_text(icons::SLIDES, "Slides", theme::semibold(size::CAPTION), p.text));
        let r = tab.rect;
        ui.painter().line_segment([egui::pos2(r.left(), r.bottom() + 4.0), egui::pos2(r.right(), r.bottom() + 4.0)], Stroke::new(2.0, p.accent));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, icons::PLUS, "Import PDF", view.can_import).clicked() {
                actions.push(Action::ImportSlides);
            }
        });
    });
}

fn deck_view(ui: &mut egui::Ui, view: &View, session: &Session, deck: &SlideDeck, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let page = view.material.viewer.page;
    ui.horizontal(|ui| {
        if session.decks.len() > 1 {
            egui::ComboBox::from_id_salt("deck-picker").selected_text(&deck.title).show_ui(ui, |ui| {
                for other in &session.decks {
                    if ui.selectable_label(other.id == deck.id, &other.title).clicked() {
                        actions.push(Action::ShowDeck(other.id));
                    }
                }
            });
        } else {
            ui.label(widgets::icon_text(icons::PDF, &deck.title, theme::regular(size::BODY), p.text));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let menu = ui.menu_button(widgets::icon(icons::MORE, size::ICON, p.text_muted), |ui| {
                let remove = widgets::icon_text(icons::TRASH, "Remove these slides", theme::regular(size::CAPTION), p.danger_text);
                if ui.button(remove).on_hover_text("Notes stay; their links to these slides are removed").clicked() {
                    actions.push(Action::RemoveDeck(deck.id));
                }
            });
            widgets::name(&menu.response, Role::Button, "Slide deck actions");
        });
    });
    page_search(ui, deck, buffers, actions);
    ui.add_space(space::S);
    page_strip(ui, deck, page, actions);
    ui.add_space(space::S);
    page_image(ui, view, deck, page, buffers, actions);
    ui.add_space(space::S);
    controls(ui, view, session, deck, page, actions);
    let has_ink = !deck.ink_on(page).is_empty();
    super::ink::toolbar(ui, view.paper.ink, deck.id, page, has_ink, view.can_import, actions);
    ui.add_space(space::S);
    ui.horizontal(|ui| {
        let mut link_new = view.material.viewer.link_new_notes;
        if widgets::switch(ui, &mut link_new, "Link new notes to this slide").changed() {
            actions.push(Action::SetLinkNewNotes(link_new));
        }
        ui.label(RichText::new("Link new notes to this slide").size(size::CAPTION).color(p.text))
            .on_hover_text("Notes, photos and sketches you add now remember which slide was on screen");
    });
}

/// Finds pages by the text on them (extracted from the PDF, ADR-018).
fn page_search(ui: &mut egui::Ui, deck: &SlideDeck, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let ready = !deck.page_text.is_empty();
    let prompt = if ready { "Search these slides" } else { "Reading the slide text…" };
    let hint = widgets::icon_text(icons::SEARCH, prompt, theme::regular(size::CAPTION), p.text_muted);
    let field = egui::TextEdit::singleline(&mut buffers.slide_search).hint_text(hint).desired_width(f32::INFINITY);
    let field = ui.add_enabled(ready, field);
    widgets::name(&field, Role::SearchInput, "Search slides");
    if !ready || buffers.slide_search.trim().is_empty() {
        return;
    }
    let pages = search_deck(deck, &buffers.slide_search);
    ui.horizontal_wrapped(|ui| {
        if pages.is_empty() {
            ui.label(RichText::new("No slide has that text.").size(size::SMALL).color(p.text_muted));
            return;
        }
        ui.label(RichText::new("Found on").size(size::SMALL).color(p.text_muted));
        for page in pages {
            let chip = widgets::neutral_chip(ui, None, &page.to_string(), true, &format!("Show slide {page}"));
            if chip.clicked() {
                actions.push(Action::GoToSlide(lectio_core::material::SlideLink { deck: deck.id, page }));
            }
        }
    });
}

/// Page numbers in a row: click to jump to a page.
fn page_strip(ui: &mut egui::Ui, deck: &SlideDeck, page: u32, actions: &mut Vec<Action>) {
    let p = palette(ui);
    egui::ScrollArea::horizontal().id_salt("page-strip").max_height(STRIP_BUTTON + 8.0).show(ui, |ui| {
        ui.horizontal(|ui| {
            for number in 1..=deck.page_count {
                let selected = number == page;
                let color = if selected { p.on_accent } else { p.text };
                let text = RichText::new(number.to_string()).font(theme::mono(size::SMALL)).color(color);
                let fill = if selected { p.accent } else { p.surface_alt };
                let button = egui::Button::new(text).fill(fill).min_size(egui::vec2(STRIP_BUTTON, STRIP_BUTTON - 6.0));
                let response = ui.add(button);
                widgets::name(&response, Role::Button, &format!("Go to slide {number}"));
                if selected && response.gained_focus() {
                    response.scroll_to_me(None);
                }
                if response.clicked() {
                    actions.push(Action::GoToSlide(lectio_core::material::SlideLink { deck: deck.id, page: number }));
                }
            }
        });
    });
}

fn page_image(ui: &mut egui::Ui, view: &View, deck: &SlideDeck, page: u32, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let zoom = view.slide_zoom;
    let p = palette(ui);
    egui::Frame::new()
        .fill(p.paper)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::CARD))
        .show(ui, |ui| match view.material.page(deck.id, page) {
            PageView::Ready(texture) => {
                let alt = format!("{}, slide {page} of {}", deck.title, deck.page_count);
                let width = ui.available_width() * zoom;
                let image = egui::Image::from_texture(texture).fit_to_exact_size(egui::vec2(width, width * texture.aspect_ratio().recip()));
                let rect = ui.add(image.corner_radius(CornerRadius::same(radius::CARD)).alt_text(alt)).rect;
                super::ink::paint(ui.painter(), rect, deck.ink_on(page));
                if view.paper.ink.on && view.can_import {
                    super::ink::input(ui, rect, deck.id, page, view.paper.ink, buffers, actions);
                }
            }
            PageView::Loading => {
                ui.set_min_height(160.0);
                ui.centered_and_justified(|ui| ui.spinner());
            }
            PageView::Failed(problem) => {
                ui.label(RichText::new(problem).color(theme::LIGHT.text_muted));
            }
        });
}

fn controls(ui: &mut egui::Ui, view: &View, session: &Session, deck: &SlideDeck, page: u32, actions: &mut Vec<Action>) {
    let p = palette(ui);
    ui.horizontal_wrapped(|ui| {
        if widgets::icon_button(ui, icons::PREVIOUS, "Previous slide (Page Up)", page > 1).clicked() {
            actions.push(Action::TurnSlide(-1));
        }
        ui.label(RichText::new(format!("{page} / {}", deck.page_count)).font(theme::mono(size::CAPTION)).color(p.text_muted));
        if widgets::icon_button(ui, icons::NEXT, "Next slide (Page Down)", page < deck.page_count).clicked() {
            actions.push(Action::TurnSlide(1));
        }
        let zoom = view.slide_zoom;
        if widgets::icon_button(ui, icons::ZOOM_OUT, "Zoom out", zoom > 1.0).clicked() {
            actions.push(Action::SlideZoom(zoom - ZOOM_STEP));
        }
        if widgets::icon_button(ui, icons::ZOOM_IN, "Zoom in", zoom < 3.0).clicked() {
            actions.push(Action::SlideZoom(zoom + ZOOM_STEP));
        }
        if widgets::icon_button(ui, icons::FULLSCREEN, "Full screen (Esc to leave)", true).clicked() {
            actions.push(Action::SetFullscreen(true));
        }
        let enabled = view.can_take_notes;
        let text = widgets::icon_text(icons::PLUS, "Post this slide", theme::semibold(size::SMALL), p.accent);
        let post = ui.add_enabled(enabled, egui::Button::new(text).small());
        widgets::name(&post, Role::Button, "Post this slide");
        if post.on_hover_text("Add the slide on screen to the feed at the current time").clicked() {
            actions.push(Action::PostSlide { next: false });
        }
    });
    let on_slide = |n: &&lectio_core::Note| n.slide.is_some_and(|l| l.deck == deck.id && l.page == page);
    let linked = session.notes.iter().filter(on_slide).count();
    if linked > 0 {
        ui.label(RichText::new(format!("{linked} item(s) in the feed for this slide")).size(size::SMALL).color(p.text_muted));
    }
}

/// The shown slide filling the workspace, with page arrows. Esc leaves.
pub fn fullscreen(ui: &mut egui::Ui, view: &View, session: &Session, actions: &mut Vec<Action>) {
    let Some(deck) = view.material.viewer.deck.and_then(|d| session.decks.iter().find(|x| x.id == d)) else {
        actions.push(Action::SetFullscreen(false));
        return;
    };
    let page = view.material.viewer.page;
    ui.horizontal(|ui| {
        if widgets::subtle_button(ui, icons::CLOSE, "Leave full screen", true).clicked() {
            actions.push(Action::SetFullscreen(false));
        }
        if widgets::icon_button(ui, icons::PREVIOUS, "Previous slide (Page Up)", page > 1).clicked() {
            actions.push(Action::TurnSlide(-1));
        }
        ui.label(RichText::new(format!("{page} / {}", deck.page_count)).font(theme::mono(size::CAPTION)).color(theme::LIGHT.text_muted));
        if widgets::icon_button(ui, icons::NEXT, "Next slide (Page Down)", page < deck.page_count).clicked() {
            actions.push(Action::TurnSlide(1));
        }
    });
    ui.centered_and_justified(|ui| {
        if let PageView::Ready(texture) = view.material.page(deck.id, page) {
            let alt = format!("{}, slide {page} of {}", deck.title, deck.page_count);
            let rect = ui.add(egui::Image::from_texture(texture).shrink_to_fit().alt_text(alt)).rect;
            super::ink::paint(ui.painter(), rect, deck.ink_on(page));
        } else {
            ui.spinner();
        }
    });
}
