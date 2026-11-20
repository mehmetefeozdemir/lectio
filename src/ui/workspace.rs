//! The session workspace (ADR-015): slides on the left, the feed in the
//! centre, the audio stream on the right, and the input bar along the
//! bottom. The Layout menu hides panels (saved in the library's
//! preferences). Below `NARROW` one panel shows at a time, chosen by tabs.

use eframe::egui::{self, Margin, Role, Stroke};
use lectio_core::Session;

use super::theme::{self, palette, size, space};
use super::{Buffers, NARROW, View, capture, composer, icons, notes, playback, reading, settings_and_theme, slides, stream, widgets};
use crate::action::{Action, Panel, Screen};

/// Largest share of the window one side panel may take.
const SIDE_PANEL_MAX_SHARE: f32 = 0.3;

const PANELS: [(Panel, &str, &str); 3] =
    [(Panel::Slides, icons::SLIDES, "Slides"), (Panel::Feed, icons::EDIT, "Feed"), (Panel::Audio, icons::WAVEFORM, "Audio")];

/// Back arrow, the session title, and Layout / Paper view / theme / settings.
pub fn toolbar(ui: &mut egui::Ui, view: &View, session: &Session, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    ui.horizontal(|ui| {
        let back_hint = if view.busy_recording { "Stop the recording to go back" } else { "Back to all sessions" };
        let back = widgets::icon_button_sized(ui, icons::BACK_ARROW, back_hint, !view.busy_recording, size::ICON_LARGE);
        widgets::name(&back, Role::Button, "Back to sessions");
        if back.clicked() {
            actions.push(Action::GoTo(Screen::Home));
        }
        playback::header(ui, view, session, buffers, actions);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            settings_and_theme(ui, view, actions);
            layout_menu(ui, view, actions);
            let reading = view.material.reading;
            let text = widgets::icon_text(icons::READING, "Paper view", theme::regular(size::CAPTION), p.text);
            let button = ui.add(egui::Button::selectable(reading, text));
            widgets::name(&button, Role::Button, "Paper view");
            if button.on_hover_text("Notes and slides as a document, without the audio").clicked() {
                actions.push(Action::SetReading(!reading));
            }
        });
    });
}

fn layout_menu(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let text = widgets::icon_text(icons::LAYOUT, "Layout", theme::regular(size::CAPTION), p.text);
    let menu = ui.menu_button(text, |ui| {
        widgets::section_label(ui, "Show panels");
        for (panel, icon, name) in PANELS {
            let mut shown = is_shown(view, panel);
            let label = widgets::icon_text(icon, name, theme::regular(size::CAPTION), p.text);
            let check = ui.checkbox(&mut shown, label);
            widgets::name(&check, Role::CheckBox, &format!("Show {name} panel"));
            if check.changed() {
                actions.push(Action::SetPanel(panel, shown));
            }
        }
    });
    widgets::name(&menu.response, Role::Button, "Layout");
    menu.response.on_hover_text("Choose which panels to show");
}

fn is_shown(view: &View, panel: Panel) -> bool {
    match panel {
        Panel::Slides => view.prefs.show_slides,
        Panel::Feed => view.prefs.show_feed,
        Panel::Audio => view.prefs.show_audio,
    }
}

pub fn show(ui: &mut egui::Ui, view: &View, session: &Session, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    if view.material.reading {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| reading::show(ui, view, session, actions));
        return;
    }
    if view.fullscreen {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.paper)).show(ui, |ui| slides::fullscreen(ui, view, session, actions));
        return;
    }
    let bar = egui::Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).inner_margin(Margin::symmetric(space::L as i8, space::S as i8));
    egui::Panel::bottom("input-bar").frame(bar).show(ui, |ui| input_bar(ui, view, session, actions));

    if ui.ctx().content_rect().width() < NARROW {
        narrow(ui, view, session, buffers, actions);
    } else {
        wide(ui, view, session, buffers, actions);
    }
}

/// The input bar: the recorder or player, the note input, and the labels.
fn input_bar(ui: &mut egui::Ui, view: &View, session: &Session, actions: &mut Vec<Action>) {
    let recording = view.recorder_phase.is_some_and(|p| p != lectio_core::clock::RecorderPhase::Idle);
    if recording && view.prefs.captions && view.speech.captions_supported {
        captions(ui, view);
    }
    match view.recorder_phase {
        Some(phase) => capture::bar(ui, view, phase, actions),
        None => playback::bar(ui, view, session, actions),
    }
    ui.add_space(space::XS);
    composer::show(ui, view, actions);
}

/// The live caption strip: the last few seconds of speech as text (ADR-016).
fn captions(ui: &mut egui::Ui, view: &View) {
    let p = palette(ui);
    let text = if view.speech.caption.is_empty() { "Listening…" } else { view.speech.caption };
    let frame = egui::Frame::new()
        .fill(p.surface_alt)
        .corner_radius(egui::CornerRadius::same(theme::radius::CARD))
        .inner_margin(Margin::symmetric(space::M as i8, space::S as i8));
    let response = frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(widgets::icon(icons::CAPTIONS, size::ICON_LARGE, p.accent));
                let caption = egui::RichText::new(text).size(size::TITLE).color(p.text);
                ui.add(egui::Label::new(caption).truncate());
            });
        })
        .response;
    widgets::name(&response, Role::Label, "Live captions");
    ui.add_space(space::XS);
}

fn panel_frame(ui: &egui::Ui) -> egui::Frame {
    let p = palette(ui);
    egui::Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).inner_margin(Margin::same(space::M as i8))
}

fn wide(ui: &mut egui::Ui, view: &View, session: &Session, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let prefs = view.prefs;
    // Side panels may take at most this share of the window each, so they
    // can never squeeze the feed (PROBLEMS P-026).
    let max_side = ui.ctx().content_rect().width() * SIDE_PANEL_MAX_SHARE;
    // With the feed hidden, the slides take the middle.
    if prefs.show_slides && prefs.show_feed {
        egui::Panel::left("slides-panel")
            .frame(panel_frame(ui))
            .resizable(true)
            .default_size(380.0)
            .min_size(240.0)
            .max_size(max_side.max(240.0))
            .show(ui, |ui| slides::show(ui, view, session, buffers, actions));
    }
    if prefs.show_audio {
        egui::Panel::right("audio-panel")
            .frame(panel_frame(ui))
            .resizable(true)
            .default_size(250.0)
            .min_size(150.0)
            .max_size(max_side.max(150.0))
            .show(ui, |ui| stream::show(ui, view, session, actions));
    }
    let p = palette(ui);
    let frame = egui::Frame::new().fill(p.bg).inner_margin(Margin::symmetric(space::L as i8, space::M as i8));
    egui::CentralPanel::default().frame(frame).show(ui, |ui| {
        if prefs.show_feed {
            notes::feed(ui, view, session, buffers, actions);
        } else if prefs.show_slides {
            slides::show(ui, view, session, buffers, actions);
        } else if !prefs.show_audio {
            let message = "Every panel is hidden. Use Layout to bring them back.";
            if widgets::empty_state(ui, icons::LAYOUT, "Nothing to show", message, Some((icons::EDIT, "Show the feed"))) {
                actions.push(Action::SetPanel(Panel::Feed, true));
            }
        }
    });
}

/// One panel at a time, picked with tabs (Layout choices don't apply here).
fn narrow(ui: &mut egui::Ui, view: &View, session: &Session, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let frame = egui::Frame::new().fill(p.bg).inner_margin(Margin::symmetric(space::M as i8, space::XS as i8));
    egui::Panel::top("panel-tabs").frame(frame).show(ui, |ui| {
        ui.horizontal(|ui| {
            for (panel, icon, name) in PANELS {
                let selected = view.narrow_tab == panel;
                let color = if selected { p.accent } else { p.text_muted };
                let tab = ui.add(egui::Button::selectable(selected, widgets::icon_text(icon, name, theme::semibold(size::CAPTION), color)));
                widgets::name(&tab, Role::Tab, &format!("{name} tab"));
                if tab.clicked() {
                    actions.push(Action::ShowTab(panel));
                }
            }
        });
    });
    let frame = egui::Frame::new().fill(p.bg).inner_margin(Margin::symmetric(space::M as i8, space::S as i8));
    egui::CentralPanel::default().frame(frame).show(ui, |ui| match view.narrow_tab {
        Panel::Slides => slides::show(ui, view, session, buffers, actions),
        Panel::Feed => notes::feed(ui, view, session, buffers, actions),
        Panel::Audio => stream::show(ui, view, session, actions),
    });
}
