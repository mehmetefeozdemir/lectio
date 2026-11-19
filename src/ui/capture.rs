//! The recorder row of the input bar (capture mode): a round record/pause
//! button (red while recording), the elapsed time in mono, the audio source
//! choice before recording, a live level meter, and stop.

use eframe::egui::{self, CornerRadius, Key, Modifiers, RichText, Sense, Stroke, Role};
use lectio_core::clock::{RecorderCommand, RecorderPhase};
use lectio_core::timeline::format_offset;

use super::theme::{self, palette, size, space};
use super::{View, icons, shortcut_hint, widgets};
use crate::action::Action;

const BIG_BUTTON: f32 = 40.0;
const METER_BAR: f32 = 3.0;
/// After this many seconds of silence while recording, say so.
pub const SILENCE_HINT_S: f64 = 10.0;
pub const SILENCE_HINT: &str =
    "Nothing has been picked up for 10 seconds. Check that the microphone is connected and not muted.";

pub fn bar(ui: &mut egui::Ui, view: &View, phase: RecorderPhase, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        record_button(ui, phase, actions);
        ui.add_space(space::XS);
        let p = palette(ui);
        let status = match phase {
            RecorderPhase::Idle => "Ready to record",
            RecorderPhase::Recording => "Recording",
            RecorderPhase::Paused => "Paused",
            RecorderPhase::Stopped => "Saving…",
        };
        let color = if phase == RecorderPhase::Recording { p.record } else { p.text_muted };
        ui.label(RichText::new(format_offset(view.offset_ms)).font(theme::mono(size::TITLE)).color(p.text))
            .on_hover_text("Time recorded so far. Pauses don't count.");
        ui.label(RichText::new(status).font(theme::semibold(size::CAPTION)).color(color));
        if phase == RecorderPhase::Idle {
            source_picker(ui);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let can_stop = matches!(phase, RecorderPhase::Recording | RecorderPhase::Paused);
            let hint = shortcut_hint(ui.ctx(), Modifiers::COMMAND | Modifiers::SHIFT, Key::R);
            let label = format!("Stop and save ({hint})");
            let stop = widgets::icon_button_sized(ui, icons::STOP, &label, can_stop, size::ICON_LARGE);
            widgets::name(&stop, Role::Button, "Stop and save");
            if stop.clicked() {
                actions.push(Action::Recorder(RecorderCommand::Stop));
            }
            level_meter(ui, &view.levels, phase);
        });
    });
    audio_hints(ui, view, phase);
}

/// Where the audio comes from. Only the microphone works for now; the
/// other two are shown so the user knows they are planned (Phase 3).
fn source_picker(ui: &mut egui::Ui) {
    let p = palette(ui);
    let selected = widgets::icon_text(icons::MICROPHONE, "Microphone", theme::regular(size::CAPTION), p.text);
    let combo = egui::ComboBox::from_id_salt("audio-source").selected_text(selected).show_ui(ui, |ui| {
        let _ = ui.selectable_label(true, "Microphone");
        for source in ["Computer audio", "Both"] {
            ui.add_enabled(false, egui::Button::selectable(false, source)).on_disabled_hover_text("Coming soon");
        }
    });
    widgets::name(&combo.response, Role::ComboBox, "Audio source");
    combo.response.on_hover_text("Audio source");
}

fn record_button(ui: &mut egui::Ui, phase: RecorderPhase, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let (label, icon, command) = match phase {
        RecorderPhase::Idle => ("Start recording", icons::RECORD, Some(RecorderCommand::Start)),
        RecorderPhase::Recording => ("Pause recording", icons::PAUSE, Some(RecorderCommand::Pause)),
        RecorderPhase::Paused => ("Resume recording", icons::RECORD, Some(RecorderCommand::Resume)),
        RecorderPhase::Stopped => ("Saving", icons::RECORD, None),
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(BIG_BUTTON, BIG_BUTTON), Sense::click());
    let filled = phase == RecorderPhase::Recording;
    let painter = ui.painter();
    let ring = if response.hovered() { 2.0 } else { 1.5 };
    if filled {
        painter.circle_filled(rect.center(), BIG_BUTTON / 2.0, p.record);
    } else {
        painter.circle(rect.center(), BIG_BUTTON / 2.0 - 1.0, p.surface, Stroke::new(ring, p.record));
    }
    let icon_color = if filled { p.on_record } else { p.record };
    widgets::paint_centered(painter, rect, icon, theme::icon_font(22.0), icon_color);
    if response.has_focus() {
        painter.circle_stroke(rect.center(), BIG_BUTTON / 2.0 + 3.0, ui.visuals().selection.stroke);
    }
    let hint = shortcut_hint(ui.ctx(), Modifiers::COMMAND, Key::R);
    widgets::name(&response, Role::Button, label);
    if response.on_hover_text(format!("{label} ({hint})")).clicked() {
        actions.extend(command.map(Action::Recorder));
    }
}

/// Recent loudness as thin bars: a quick "is the mic hearing anything?" check.
fn level_meter(ui: &mut egui::Ui, levels: &[f32], phase: RecorderPhase) {
    let p = palette(ui);
    let width = (ui.available_width() - space::M).clamp(0.0, 200.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 32.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let step = METER_BAR + 2.0;
    let count = (rect.width() / step) as usize;
    let recent = &levels[levels.len().saturating_sub(count)..];
    for i in 0..count {
        let level = recent.get(i + recent.len() - recent.len().min(count)).copied().unwrap_or(0.0);
        let height = (level.sqrt() * rect.height()).max(2.0);
        let x = rect.left() + i as f32 * step;
        let bar = egui::Rect::from_center_size(egui::pos2(x + METER_BAR / 2.0, rect.center().y), egui::vec2(METER_BAR, height));
        let color = if phase == RecorderPhase::Recording && level > 0.0 { p.record.gamma_multiply(0.75) } else { p.track };
        painter.rect_filled(bar, CornerRadius::same(1), color);
    }
    response.widget_info(|| egui::WidgetInfo::labeled(Role::Meter, true, "Microphone level"));
}

fn audio_hints(ui: &mut egui::Ui, view: &View, phase: RecorderPhase) {
    let p = palette(ui);
    if phase == RecorderPhase::Idle {
        let note = if cfg!(target_arch = "wasm32") {
            "Audio is stored in this browser when you stop recording."
        } else {
            "Audio is saved on this computer as you record."
        };
        ui.label(RichText::new(note).size(size::SMALL).color(p.text_muted));
    }
    if phase == RecorderPhase::Recording && view.quiet_for_s >= SILENCE_HINT_S {
        ui.label(RichText::new(SILENCE_HINT).size(size::SMALL).color(p.warning_text).background_color(p.warning_fill));
    }
    if view.audio_dropped_ms > 0 {
        let lost = if view.audio_dropped_ms < 1_000 {
            format!("{} ms", view.audio_dropped_ms)
        } else {
            format!("{:.1} s", view.audio_dropped_ms as f64 / 1_000.0)
        };
        let text = format!("{lost} of audio could not be saved because the computer was too busy.");
        ui.label(RichText::new(text).color(p.warning_text).background_color(p.warning_fill));
    }
    if view.audio_overruns > 0 {
        let text = format!("The system reported {} audio glitch(es) during this recording.", view.audio_overruns);
        ui.label(RichText::new(text).size(size::SMALL).color(p.text_muted));
    }
}
