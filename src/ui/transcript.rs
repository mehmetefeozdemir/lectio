//! The Transcript tab of the right panel (ADR-016): model and language,
//! download and transcription progress, then the segments. Clicking a
//! segment plays from it; the playing segment is highlighted and kept in
//! view; the chat icon copies a segment into the feed as a note.

use eframe::egui::{self, CornerRadius, RichText, Role, Sense, Stroke};
use lectio_core::Session;
use lectio_core::prefs::SpeechModel;
use lectio_core::timeline::format_offset;
use lectio_core::transcript::{Transcript, segment_at};
use lectio_speech::models;
use lectio_speech::{LANGUAGES, language_name};

use super::theme::{self, palette, radius, size, space};
use super::{View, icons, widgets};
use crate::action::Action;

const MB: f64 = 1_048_576.0;

pub fn show(ui: &mut egui::Ui, view: &View, session: &Session, actions: &mut Vec<Action>) {
    let p = palette(ui);
    if view.speech.available {
        settings(ui, view, actions);
        progress(ui, view, session, actions);
    } else {
        let message = "Transcripts are made on this device. They can't be made in a read-only window (sample data).";
        ui.add(egui::Label::new(RichText::new(message).size(size::SMALL).color(p.text_muted)).wrap());
    }
    ui.add_space(space::S);
    let Some(transcript) = session.transcript.as_ref().filter(|t| !t.segments.is_empty()) else {
        let hint = if session.recording.is_some() {
            "No transcript yet. Choose a model and press Transcribe; everything stays on this device."
        } else {
            "Record the lecture first; the transcript is made from the recording."
        };
        ui.add(egui::Label::new(RichText::new(hint).size(size::SMALL).color(p.text_muted)).wrap());
        return;
    };
    let line = format!("{} · {} model", language_name(&transcript.language), transcript.model);
    ui.label(RichText::new(line).size(size::SMALL).color(p.text_muted));
    segments(ui, view, transcript, actions);
}

fn settings(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let chosen = view.prefs.speech_model;
    ui.horizontal_wrapped(|ui| {
        let combo = egui::ComboBox::from_id_salt("speech-model").selected_text(format!("Model: {}", chosen.name())).show_ui(
            ui,
            |ui| {
                for (index, model) in SpeechModel::ALL.into_iter().enumerate() {
                    let info = models::info(model);
                    let text = format!("{} · {:.0} MB · {}", model.name(), info.total_bytes() as f64 / MB, info.summary);
                    let why = view.speech.unavailable[index];
                    let item = ui.add_enabled(why.is_none(), egui::Button::selectable(model == chosen, text));
                    let item = match why {
                        Some(why) => item.on_disabled_hover_text(why),
                        None => item,
                    };
                    if item.clicked() {
                        actions.push(Action::SetSpeechModel(model));
                    }
                }
            },
        );
        widgets::name(&combo.response, Role::ComboBox, "Speech model");
        let language = view.prefs.language.as_deref().map_or("Detect language", language_name);
        let combo = egui::ComboBox::from_id_salt("speech-language").selected_text(language).height(320.0).show_ui(ui, |ui| {
            if ui.selectable_label(view.prefs.language.is_none(), "Detect language").clicked() {
                actions.push(Action::SetLanguage(None));
            }
            for (code, name) in LANGUAGES {
                let on = view.prefs.language.as_deref() == Some(*code);
                if ui.selectable_label(on, *name).clicked() {
                    actions.push(Action::SetLanguage(Some((*code).to_owned())));
                }
            }
        });
        widgets::name(&combo.response, Role::ComboBox, "Spoken language");
    });
    if !view.speech.downloaded && view.speech.download.is_none() {
        let size_mb = models::info(chosen).total_bytes() as f64 / MB;
        let text = format!("The {} model ({size_mb:.0} MB) downloads once, then works offline.", chosen.name());
        ui.add(egui::Label::new(RichText::new(text).size(size::SMALL).color(p.text_muted)).wrap());
    }
}

fn progress(ui: &mut egui::Ui, view: &View, session: &Session, actions: &mut Vec<Action>) {
    let p = palette(ui);
    if let Some((model, done, total)) = view.speech.download {
        let fraction = done as f32 / total.max(1) as f32;
        let text = format!("Downloading {} model: {:.0} of {:.0} MB", model.name(), done as f64 / MB, total as f64 / MB);
        let bar = ui.add(egui::ProgressBar::new(fraction).text(text).desired_height(18.0));
        widgets::name(&bar, Role::ProgressIndicator, "Model download");
        return;
    }
    match view.speech.job {
        Some((job, done, total)) if job == session.id => {
            let fraction = done as f32 / total.max(1) as f32;
            ui.horizontal(|ui| {
                let text = format!("Transcribing… {} of {}", format_offset(done), format_offset(total));
                let width = ui.available_width() - 32.0;
                let bar = ui.add(egui::ProgressBar::new(fraction).text(text).desired_height(18.0).desired_width(width));
                widgets::name(&bar, Role::ProgressIndicator, "Transcription progress");
                if widgets::icon_button(ui, icons::STOP_CIRCLE, "Stop transcribing (what is done is kept)", true).clicked() {
                    actions.push(Action::CancelTranscription);
                }
            });
        }
        Some(_) => {
            ui.label(RichText::new("Another session is being transcribed.").size(size::SMALL).color(p.text_muted));
        }
        None => idle_buttons(ui, view, session, actions),
    }
}

fn idle_buttons(ui: &mut egui::Ui, view: &View, session: &Session, actions: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        let unfinished = session.transcript.as_ref().is_some_and(|t| !t.complete && !t.segments.is_empty());
        let label = match (unfinished, session.transcript.is_some()) {
            (true, _) => "Continue transcribing",
            (false, true) => "Transcribe again",
            (false, false) => "Transcribe",
        };
        if widgets::primary_button(ui, icons::TRANSCRIPT, label, session.recording.is_some()).clicked() {
            actions.push(Action::Transcribe);
        }
        if !view.speech.downloaded && widgets::subtle_button(ui, icons::DOWNLOAD, "Download model", true).clicked() {
            actions.push(Action::DownloadModel);
        }
        if session.transcript.is_some() && widgets::icon_button(ui, icons::TRASH, "Delete transcript", true).clicked() {
            actions.push(Action::ClearTranscript);
        }
    });
}

fn segments(ui: &mut egui::Ui, view: &View, transcript: &Transcript, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let playback = view.duration_ms.is_some();
    let playing = playback.then(|| segment_at(&transcript.segments, view.offset_ms)).flatten();
    let follow_id = ui.scope_id().with("followed-segment");
    let followed = ui.ctx().data(|d| d.get_temp::<Option<usize>>(follow_id)).flatten();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(space::XS);
        for (index, segment) in transcript.segments.iter().enumerate() {
            let active = playing == Some(index);
            let fill = if active { p.accent_soft } else { egui::Color32::TRANSPARENT };
            let frame = egui::Frame::new()
                .fill(fill)
                .corner_radius(CornerRadius::same(radius::CONTROL))
                .inner_margin(egui::Margin::symmetric(space::XS as i8 + 2, 3));
            let sense = if playback { Sense::click() } else { Sense::hover() };
            let inner = frame.show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    let time_color = if active { p.accent } else { p.text_muted };
                    let time = RichText::new(format_offset(segment.start_ms)).font(theme::mono(size::SMALL)).color(time_color);
                    let time = ui.add(egui::Label::new(time).sense(sense));
                    let width = (ui.available_width() - 28.0).max(60.0);
                    let text = ui.scope(|ui| {
                        ui.set_max_width(width);
                        let words = RichText::new(&segment.text).size(size::CAPTION).color(p.text);
                        let (response, word) = widgets::word_label(ui, words, sense);
                        if let Some(word) = word {
                            actions.push(Action::LookUp { word, session: None, offset_ms: Some(segment.start_ms) });
                        }
                        response
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        let label = format!("Copy the words at {} into the feed", format_offset(segment.start_ms));
                        if widgets::icon_button(ui, icons::TO_FEED, &label, view.can_take_notes).clicked() {
                            actions.push(Action::SegmentToFeed(index));
                        }
                    });
                    time.union(text.inner)
                })
                .inner
            });
            let rect = inner.response.rect;
            let response = inner.inner;
            if active {
                let bar = egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height()));
                ui.painter().rect_filled(bar, CornerRadius::same(2), p.accent);
                if followed != Some(index) {
                    response.scroll_to_me(Some(egui::Align::Center));
                }
            }
            if response.hovered() && playback {
                let stroke = Stroke::new(1.0, p.border);
                ui.painter().rect_stroke(rect, CornerRadius::same(radius::CONTROL), stroke, egui::StrokeKind::Inside);
            }
            widgets::name(&response, Role::Button, &format!("Play from {}", format_offset(segment.start_ms)));
            if playback && response.clicked() {
                actions.push(Action::PlayFrom(segment.start_ms));
            }
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(follow_id, playing));
}
