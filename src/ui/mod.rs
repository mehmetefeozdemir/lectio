//! Views. Each view reads a `View` snapshot and pushes `Action`s; none of
//! them changes app state directly (ADR-006).

mod camera;
mod capture;
mod composer;
mod labels;
mod notes;
mod playback;
mod reading;
mod sidebar;
mod sketch;
mod slides;
pub mod theme;
mod timeline;

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use lectio_core::clock::{NoteDraft, RecorderCommand, RecorderPhase};
use lectio_core::{LabelKind, LabelSet, Library, SessionId};

use crate::action::Action;
use crate::material::MaterialState;
pub use sketch::SketchPad;

/// Width of the slide panel beside the notes.
const SLIDE_PANEL_WIDTH: f32 = 420.0;

/// Seek step for keyboard shortcuts and timeline arrow keys.
pub const SEEK_STEP_MS: i64 = 5_000;

/// Read-only snapshot of what the views need for one frame.
pub struct View<'a> {
    pub library: &'a Library,
    pub selected: Option<SessionId>,
    /// `Some` when the open session has no recording yet (capture mode).
    pub recorder_phase: Option<RecorderPhase>,
    pub offset_ms: u64,
    /// `Some` when the open session has a recording (playback mode).
    pub duration_ms: Option<u64>,
    pub playing: bool,
    pub can_play: bool,
    pub playback_problem: Option<&'a str>,
    pub draft: &'a NoteDraft,
    pub can_take_notes: bool,
    pub busy_recording: bool,
    pub sample_mode: bool,
    /// Audio lost while recording (P-006). Shown only when above zero.
    pub audio_dropped_ms: u64,
    pub audio_overruns: u64,
    /// Phase 2: slide viewer, textures, camera, reading view.
    pub material: &'a MaterialState,
    pub camera_available: bool,
    /// Files can be imported (false for sample data / read-only windows).
    pub can_import: bool,
}

/// What the user is in the middle of editing that isn't part of the model:
/// typed text, and the sketch being drawn.
#[derive(Default)]
pub struct Buffers {
    pub new_course: String,
    pub label_names: Option<Vec<(LabelKind, String)>>,
    pub sketch: Option<SketchPad>,
}

impl Buffers {
    pub fn open_label_editor(&mut self, labels: &LabelSet) {
        self.label_names = Some(labels.iter().map(|l| (l.kind, l.name.clone())).collect());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerKind {
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Banner {
    pub kind: BannerKind,
    pub text: String,
}

impl Banner {
    pub fn warning(text: String) -> Self {
        Self { kind: BannerKind::Warning, text }
    }
    pub fn error(text: String) -> Self {
        Self { kind: BannerKind::Error, text }
    }
}

pub fn draw(
    ui: &mut egui::Ui,
    view: &View,
    buffers: &mut Buffers,
    banner: Option<&Banner>,
    actions: &mut Vec<Action>,
) {
    egui::Panel::top("header").show(ui, |ui| header(ui, view, banner, actions));
    egui::Panel::left("courses")
        .resizable(true)
        .default_size(250.0)
        .min_size(190.0)
        .show(ui, |ui| sidebar::show(ui, view, buffers, actions));
    egui::CentralPanel::default_margins().show(ui, |ui| {
        let session = view.selected.and_then(|id| view.library.session(id));
        let Some(session) = session else { return welcome(ui, view) };
        if view.material.reading {
            return reading::show(ui, view, session, actions);
        }
        egui::Panel::right("slides")
            .resizable(true)
            .default_size(SLIDE_PANEL_WIDTH)
            .min_size(260.0)
            .show(ui, |ui| slides::show(ui, view, session, actions));
        egui::CentralPanel::default_margins().show(ui, |ui| match view.recorder_phase {
            Some(phase) => capture::show(ui, view, session, phase, actions),
            None => playback::show(ui, view, session, actions),
        });
    });
    labels::editor_window(ui.ctx(), buffers, actions);
    sketch::window(ui.ctx(), buffers, actions);
    camera::window(ui.ctx(), view, actions);
}

fn header(ui: &mut egui::Ui, view: &View, banner: Option<&Banner>, actions: &mut Vec<Action>) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Lectio").heading().strong());
        ui.label(egui::RichText::new("lecture notes on the audio timeline").weak());
        if view.sample_mode {
            let badge = egui::RichText::new(" Sample data ")
                .color(theme::chip_text())
                .background_color(theme::SAMPLE_BADGE);
            ui.label(badge);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let rename = ui.button("Rename labels…").on_hover_text("Change the names of the four labels");
            if rename.clicked() {
                actions.push(Action::OpenLabelEditor);
            }
            if view.selected.is_some() {
                let reading = view.material.reading;
                let toggle = ui
                    .add(egui::Button::selectable(reading, "Reading view"))
                    .on_hover_text("Notes only, without the recorder or slides, for calm review");
                if toggle.clicked() {
                    actions.push(Action::SetReading(!reading));
                }
            }
        });
    });
    if let Some(banner) = banner {
        banner_row(ui, banner, actions);
    }
    ui.add_space(4.0);
}

fn banner_row(ui: &mut egui::Ui, banner: &Banner, actions: &mut Vec<Action>) {
    let fill = match banner.kind {
        BannerKind::Warning => theme::WARNING_FILL,
        BannerKind::Error => theme::ERROR_FILL,
    };
    egui::Frame::new().fill(fill).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
        ui.horizontal(|ui| {
            let text = egui::RichText::new(&banner.text).color(theme::chip_text());
            ui.add(egui::Label::new(text).wrap());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Dismiss").clicked() {
                    actions.push(Action::DismissBanner);
                }
            });
        });
    });
}

fn welcome(ui: &mut egui::Ui, view: &View) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.heading("Welcome to Lectio");
        ui.add_space(8.0);
        let hint = if view.library.courses().is_empty() {
            "Add your first course on the left (for example CSCA67), then add a session for today's lecture."
        } else {
            "Pick a session on the left, or add a new one under a course."
        };
        ui.label(hint);
        ui.add_space(16.0);
        let where_saved = if cfg!(target_arch = "wasm32") { "in this browser" } else { "on this computer" };
        ui.label(egui::RichText::new(format!("Your notes stay {where_saved}. No account needed.")).weak());
    });
}

/// Keyboard shortcuts that work anywhere in the window (Cmd on macOS, Ctrl elsewhere).
pub fn shortcuts(ctx: &egui::Context, view: &View) -> Vec<Action> {
    let pressed = |modifiers: Modifiers, key: Key| {
        ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(modifiers, key)))
    };
    let mut actions = Vec::new();
    // Check the longer shortcut first: Cmd+Shift+R must not also count as Cmd+R.
    if pressed(Modifiers::COMMAND | Modifiers::SHIFT, Key::R) && view.busy_recording {
        actions.push(Action::Recorder(RecorderCommand::Stop));
    }
    if pressed(Modifiers::COMMAND, Key::R) {
        actions.extend(transport_action(view));
    }
    for (key, kind) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4].into_iter().zip(LabelKind::ALL) {
        if pressed(Modifiers::COMMAND, key) && view.can_take_notes {
            actions.push(Action::LabelMoment(kind));
        }
    }
    if view.duration_ms.is_some() {
        if pressed(Modifiers::COMMAND, Key::OpenBracket) {
            actions.push(Action::SeekBy(-SEEK_STEP_MS));
        }
        if pressed(Modifiers::COMMAND, Key::CloseBracket) {
            actions.push(Action::SeekBy(SEEK_STEP_MS));
        }
    }
    // Page Up / Page Down turn slides, unless the user is typing.
    if view.material.viewer.deck.is_some() && !ctx.egui_wants_keyboard_input() {
        if pressed(Modifiers::NONE, Key::PageUp) {
            actions.push(Action::TurnSlide(-1));
        }
        if pressed(Modifiers::NONE, Key::PageDown) {
            actions.push(Action::TurnSlide(1));
        }
    }
    actions
}

/// What the main transport key does right now: start/pause/resume
/// recording, or play/pause the audio.
fn transport_action(view: &View) -> Option<Action> {
    match view.recorder_phase {
        Some(RecorderPhase::Idle) => Some(Action::Recorder(RecorderCommand::Start)),
        Some(RecorderPhase::Recording) => Some(Action::Recorder(RecorderCommand::Pause)),
        Some(RecorderPhase::Paused) => Some(Action::Recorder(RecorderCommand::Resume)),
        Some(RecorderPhase::Stopped) => None,
        None if view.can_play => Some(Action::TogglePlay),
        None => None,
    }
}

/// "⌘R" on macOS, "Ctrl+R" elsewhere, for button hints.
pub fn shortcut_hint(ctx: &egui::Context, modifiers: Modifiers, key: Key) -> String {
    ctx.format_shortcut(&KeyboardShortcut::new(modifiers, key))
}
