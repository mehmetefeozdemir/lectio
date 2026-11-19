//! Views. Each view reads a `View` snapshot and pushes `Action`s; none of
//! them changes app state directly (ADR-007). Look and layout follow the
//! design system in `theme.rs` (ADR-014); screens and panels follow ADR-015.
//!
//! Screens: Home (all sessions), Collections, Tasks, Search, Trash, with a
//! navigation column on the left; and the session workspace (slides, feed,
//! audio stream, input bar), reached by opening a session.

mod camera;
mod capture;
mod composer;
mod format;
mod home;
pub mod icons;
mod labels;
mod nav;
mod notes;
mod playback;
mod reading;
mod sketch;
mod slides;
mod stream;
pub mod theme;
mod timeline;
pub mod widgets;
mod workspace;

use eframe::egui::{self, Align2, CornerRadius, Id, Key, KeyboardShortcut, Margin, Modifiers, RichText, ThemePreference};
use lectio_core::clock::{NoteDraft, RecorderCommand, RecorderPhase};
use lectio_core::prefs::Preferences;
use lectio_core::{LabelKind, LabelSet, Library, NoteId, SessionId};

use crate::action::{Action, Panel, Screen};
use crate::material::MaterialState;
pub use sketch::SketchPad;
use theme::{palette, radius, size, space};

/// Cmd+1..5 apply the labels in `LabelKind::ALL` order (Heading is Cmd+5).
pub const LABEL_KEYS: [Key; 5] = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5];
/// "Post next slide".
pub const POST_SLIDE: (Modifiers, Key) = (Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::N);
/// Seek step for keyboard shortcuts and timeline arrow keys.
pub const SEEK_STEP_MS: i64 = 5_000;
/// Below this width (px) the navigation shows icons only and the workspace
/// shows one panel at a time, as tabs.
pub const NARROW: f32 = 900.0;

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
    /// Audio lost while recording. Shown only when above zero.
    pub audio_dropped_ms: u64,
    pub audio_overruns: u64,
    pub material: &'a MaterialState,
    pub camera_available: bool,
    /// Files can be imported (false for sample data / read-only windows).
    pub can_import: bool,
    pub undo_message: Option<&'static str>,
    /// Recent recording levels (0..=1), newest last, for the level meter.
    pub levels: Vec<f32>,
    pub sidebar: Option<bool>,
    pub theme: ThemePreference,
    pub screen: Screen,
    pub narrow_tab: Panel,
    /// Slide zoom (1.0 = fit width) and the full-screen slide view.
    pub slide_zoom: f32,
    pub fullscreen: bool,
    /// Seconds the microphone has heard nothing while recording.
    pub quiet_for_s: f64,
    /// Saved layout and theme (part of the library, ADR-015).
    pub prefs: &'a Preferences,
}

/// What the user is in the middle of editing that isn't part of the model.
#[derive(Default)]
pub struct Buffers {
    pub new_course: String,
    pub adding_course: bool,
    pub label_names: Option<Vec<(LabelKind, String)>>,
    pub sketch: Option<SketchPad>,
    /// Inline title editing: which session, and the text so far.
    pub title_edit: Option<(SessionId, String)>,
    /// Inline editing of a slide item's title in the feed.
    pub note_edit: Option<(NoteId, String)>,
    /// "New session" was pressed and the course is being chosen.
    pub picking_course: bool,
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

pub fn draw(ui: &mut egui::Ui, view: &View, buffers: &mut Buffers, banner: Option<&Banner>, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let width = ui.ctx().content_rect().width();
    let session = view.selected.and_then(|id| view.library.session(id)).filter(|_| view.screen == Screen::Workspace);

    let bar = egui::Frame::new().fill(p.bg).inner_margin(Margin::symmetric(space::M as i8, space::S as i8));
    egui::Panel::top("toolbar").frame(bar).show(ui, |ui| match session {
        Some(session) => workspace::toolbar(ui, view, session, buffers, actions),
        None => toolbar(ui, view, actions),
    });
    if let Some(banner) = banner {
        let frame = egui::Frame::new().fill(p.bg).inner_margin(Margin::symmetric(space::M as i8, 0));
        egui::Panel::top("banner").frame(frame).show(ui, |ui| banner_row(ui, banner, actions));
    }
    match session {
        Some(session) => workspace::show(ui, view, session, buffers, actions),
        None => {
            let compact = !view.sidebar.unwrap_or(width >= NARROW);
            let frame = egui::Frame::new().fill(p.surface_alt).inner_margin(Margin::same(space::S as i8));
            egui::Panel::left("nav").frame(frame).resizable(false).exact_size(if compact { 56.0 } else { 200.0 }).show(ui, |ui| {
                nav::show(ui, view, compact, actions);
            });
            let frame = egui::Frame::new().fill(p.bg).inner_margin(Margin::symmetric(space::XL as i8, space::L as i8));
            egui::CentralPanel::default().frame(frame).show(ui, |ui| home::show(ui, view, buffers, actions));
        }
    }
    labels::editor_window(ui.ctx(), buffers, actions);
    sketch::window(ui.ctx(), buffers, actions);
    camera::window(ui.ctx(), view, actions);
    undo_toast(ui.ctx(), view, actions);
}

/// The toolbar on the list screens: navigation toggle, name, theme, settings.
fn toolbar(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let expanded = view.sidebar.unwrap_or(ui.ctx().content_rect().width() >= NARROW);
    ui.horizontal(|ui| {
        let label = if expanded { "Collapse navigation" } else { "Expand navigation" };
        if widgets::icon_button_sized(ui, icons::SIDEBAR, label, true, size::ICON_LARGE).clicked() {
            actions.push(Action::SetSidebar(!expanded));
        }
        ui.label(RichText::new("Lectio").font(theme::semibold(size::BODY)).color(p.text));
        if view.sample_mode {
            widgets::neutral_chip(ui, None, "Sample data", false, "Sample data");
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| settings_and_theme(ui, view, actions));
    });
}

/// Settings menu and the theme switch (right side of every toolbar).
pub fn settings_and_theme(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let p = palette(ui);
    ui.menu_button(widgets::icon(icons::SETTINGS, size::ICON_LARGE, p.text_muted), |ui| {
        if ui.button(widgets::icon_text(icons::TAG, "Rename labels…", theme::regular(size::CAPTION), p.text)).clicked() {
            actions.push(Action::OpenLabelEditor);
        }
    })
    .response
    .on_hover_text("Settings");
    let icon = match view.theme {
        ThemePreference::System => icons::SYSTEM,
        ThemePreference::Light => icons::SUN,
        ThemePreference::Dark => icons::MOON,
    };
    let next = theme::next_theme(view.theme);
    let label = format!("Theme: {} (switch to {})", theme::theme_name(view.theme), theme::theme_name(next));
    if widgets::icon_button_sized(ui, icon, &label, true, size::ICON_LARGE).clicked() {
        actions.push(Action::SetTheme(next));
    }
}

fn banner_row(ui: &mut egui::Ui, banner: &Banner, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let (fill, text) = match banner.kind {
        BannerKind::Warning => (p.warning_fill, p.warning_text),
        BannerKind::Error => (p.error_fill, p.error_text),
    };
    egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(radius::CARD)).inner_margin(space::S).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(RichText::new(&banner.text).color(text)).wrap());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, icons::CLOSE, "Dismiss message", true).clicked() {
                    actions.push(Action::DismissBanner);
                }
            });
        });
    });
    ui.add_space(space::S);
}

fn undo_toast(ctx: &egui::Context, view: &View, actions: &mut Vec<Action>) {
    let Some(message) = view.undo_message else { return };
    egui::Area::new(Id::unique("undo-toast")).anchor(Align2::CENTER_BOTTOM, [0.0, -space::XL]).show(ctx, |ui| {
        let p = palette(ui);
        egui::Frame::popup(ui.style()).fill(p.surface).inner_margin(Margin::symmetric(space::M as i8, space::S as i8)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(message).color(p.text));
                let undo = ui.add(egui::Button::new(widgets::icon_text(icons::UNDO, "Undo", theme::semibold(size::CAPTION), p.accent)).frame(false));
                widgets::name(&undo, egui::Role::Button, "Undo");
                if undo.clicked() {
                    actions.push(Action::Undo);
                }
                if widgets::icon_button(ui, icons::CLOSE, "Dismiss", true).clicked() {
                    actions.push(Action::DismissUndo);
                }
            });
        });
    });
}

/// Keyboard shortcuts that work anywhere in the window (Cmd on macOS, Ctrl elsewhere).
pub fn shortcuts(ctx: &egui::Context, view: &View) -> Vec<Action> {
    let pressed = |modifiers: Modifiers, key: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(modifiers, key)));
    let mut actions = Vec::new();
    // Check the longer shortcut first: Cmd+Shift+R must not also count as Cmd+R.
    if pressed(Modifiers::COMMAND | Modifiers::SHIFT, Key::R) && view.busy_recording {
        actions.push(Action::Recorder(RecorderCommand::Stop));
    }
    if pressed(Modifiers::COMMAND, Key::R) {
        actions.extend(transport_action(view));
    }
    for (key, kind) in LABEL_KEYS.into_iter().zip(LabelKind::ALL) {
        if pressed(Modifiers::COMMAND, key) && view.can_take_notes {
            actions.push(Action::LabelMoment(kind));
        }
    }
    if pressed(POST_SLIDE.0, POST_SLIDE.1) && view.can_take_notes && view.material.viewer.deck.is_some() {
        actions.push(Action::PostSlide { next: true });
    }
    if view.fullscreen && pressed(Modifiers::NONE, Key::Escape) {
        actions.push(Action::SetFullscreen(false));
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
pub fn transport_action(view: &View) -> Option<Action> {
    match view.recorder_phase {
        Some(RecorderPhase::Idle) => Some(Action::Recorder(RecorderCommand::Start)),
        Some(RecorderPhase::Recording) => Some(Action::Recorder(RecorderCommand::Pause)),
        Some(RecorderPhase::Paused) => Some(Action::Recorder(RecorderCommand::Resume)),
        Some(RecorderPhase::Stopped) => None,
        None if view.can_play => Some(Action::TogglePlay),
        None => None,
    }
}

/// "⌘R" on macOS, "Ctrl+R" elsewhere, for tooltips.
pub fn shortcut_hint(ctx: &egui::Context, modifiers: Modifiers, key: Key) -> String {
    ctx.format_shortcut(&KeyboardShortcut::new(modifiers, key))
}
