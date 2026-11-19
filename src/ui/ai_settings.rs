//! Settings › AI study tools and dictionary (ADR-021, ADR-022): which
//! provider writes outlines, quizzes and flashcards (or none, which hides
//! those features), and the offline/online dictionary.

use eframe::egui::{self, RichText, Role};
use lectio_core::dictionary::ATTRIBUTION;
use lectio_core::prefs::{AiProvider, LocalModel};
use lectio_llm::models;

use super::theme::{palette, size, space};
use super::{Buffers, View, icons, widgets};
use crate::action::Action;

const MB: f64 = 1_048_576.0;

pub fn window(ctx: &egui::Context, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    if !view.study.settings_open {
        return;
    }
    let mut open = true;
    egui::Window::new("AI study tools and dictionary")
        .collapsible(false)
        .resizable(false)
        .default_width(460.0)
        .open(&mut open)
        .show(ctx, |ui| {
            if !view.study.ai_available {
                let note = "These tools can't be set up in a read-only window (sample data).";
                ui.label(RichText::new(note).color(palette(ui).text_muted));
                return;
            }
            providers(ui, view, buffers, actions);
            ui.add_space(space::M);
            ui.separator();
            dictionary(ui, view, actions);
            ui.add_space(space::M);
            if widgets::subtle_button(ui, icons::CLOSE, "Close", true).clicked() {
                actions.push(Action::SetAiSettingsOpen(false));
            }
        });
    if !open {
        actions.push(Action::SetAiSettingsOpen(false));
    }
}

fn note(ui: &mut egui::Ui, text: &str) {
    let color = palette(ui).text_muted;
    ui.add(egui::Label::new(RichText::new(text).size(size::SMALL).color(color)).wrap());
}

fn providers(ui: &mut egui::Ui, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let current = &view.prefs.ai;
    widgets::section_label(ui, "Outline, quizzes and flashcards");
    note(ui, "Every item a model writes links to the moment or slide it came from. With no provider these tools stay hidden.");
    ui.add_space(space::XS);
    let choices = [
        ("Off", AiProvider::None),
        ("On this device", AiProvider::Local { model: LocalModel::Small }),
        ("Ollama on this computer", AiProvider::Ollama { model: String::new() }),
        ("An OpenAI-compatible service", AiProvider::OpenAi { base_url: buffers.ai_base_url.clone(), model: buffers.ai_model.clone() }),
    ];
    for (label, provider) in choices {
        let selected = std::mem::discriminant(current) == std::mem::discriminant(&provider);
        let radio = ui.radio(selected, label);
        widgets::name(&radio, Role::RadioButton, label);
        if radio.clicked() && !selected {
            actions.push(Action::SetAiProvider(provider));
        }
    }
    ui.add_space(space::S);
    match current {
        AiProvider::None => {}
        AiProvider::Local { model } => local(ui, view, *model, actions),
        AiProvider::Ollama { model } => ollama(ui, view, model, actions),
        AiProvider::OpenAi { base_url, model } => endpoint(ui, view, base_url, model, buffers, actions),
    }
}

fn local(ui: &mut egui::Ui, view: &View, chosen: LocalModel, actions: &mut Vec<Action>) {
    for (index, model) in LocalModel::ALL.into_iter().enumerate() {
        let why = view.study.local_unavailable[index];
        let radio = ui.add_enabled(why.is_none(), egui::RadioButton::new(model == chosen, models::info(model).name));
        let radio = match why {
            Some(why) => radio.on_disabled_hover_text(why),
            None => radio,
        };
        if radio.clicked() {
            actions.push(Action::SetAiProvider(AiProvider::Local { model }));
        }
    }
    let index = LocalModel::ALL.iter().position(|m| *m == chosen).unwrap_or(0);
    if let Some((done, total)) = view.study.download {
        let text = format!("Downloading: {:.0} of {:.0} MB", done as f64 / MB, total as f64 / MB);
        let bar = ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).text(text));
        widgets::name(&bar, Role::ProgressIndicator, "Study model download");
    } else if view.study.ready {
        note(ui, "Loaded and ready. Works offline.");
        if let Some((tokens, seconds)) = view.study.last_timing {
            let speed = tokens as f64 / seconds.max(0.001);
            note(ui, &format!("Last answer: {tokens} tokens (prompt and reply) in {seconds:.1} s, {speed:.0} tokens/s."));
        }
    } else if view.study.local_downloaded[index] {
        note(ui, "Downloaded. Works offline.");
    } else {
        let size_mb = models::info(chosen).total_bytes() as f64 / MB;
        note(ui, &format!("Downloads once ({size_mb:.0} MB), then works offline."));
        if widgets::subtle_button(ui, icons::DOWNLOAD, "Download the study model", true).clicked() {
            actions.push(Action::PrepareAi);
        }
    }
}

fn ollama(ui: &mut egui::Ui, view: &View, chosen: &str, actions: &mut Vec<Action>) {
    match view.study.ollama {
        None => note(ui, "Checking for Ollama…"),
        Some(None) => note(ui, "Ollama isn't running on this computer. Start it, then check again."),
        Some(Some(names)) if names.is_empty() => note(ui, "Ollama has no models yet (for example: ollama pull qwen2.5:1.5b)."),
        Some(Some(names)) => {
            let shown = if chosen.is_empty() { "Choose a model" } else { chosen };
            let combo = egui::ComboBox::from_id_salt("ollama-model").selected_text(shown).show_ui(ui, |ui| {
                for name in names {
                    if ui.selectable_label(name == chosen, name).clicked() {
                        actions.push(Action::SetAiProvider(AiProvider::Ollama { model: name.clone() }));
                    }
                }
            });
            widgets::name(&combo.response, Role::ComboBox, "Ollama model");
        }
    }
    if widgets::subtle_button(ui, icons::REVIEW, "Check again", true).clicked() {
        actions.push(Action::ProbeOllama);
    }
}

fn endpoint(ui: &mut egui::Ui, view: &View, base_url: &str, model: &str, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    if buffers.ai_base_url.is_empty() && !base_url.is_empty() {
        buffers.ai_base_url = base_url.to_owned();
    }
    if buffers.ai_model.is_empty() && !model.is_empty() {
        buffers.ai_model = model.to_owned();
    }
    egui::Grid::new("endpoint").num_columns(2).spacing([space::S, space::S]).show(ui, |ui| {
        ui.label("Address");
        let field = ui.add(egui::TextEdit::singleline(&mut buffers.ai_base_url).hint_text("https://…/v1").desired_width(280.0));
        widgets::name(&field, Role::TextInput, "Endpoint address");
        ui.end_row();
        ui.label("Model");
        let field = ui.add(egui::TextEdit::singleline(&mut buffers.ai_model).hint_text("model name").desired_width(280.0));
        widgets::name(&field, Role::TextInput, "Endpoint model");
        ui.end_row();
        ui.label("Key");
        let hint = if view.study.has_key { "saved (type to replace)" } else { "paste the key" };
        let field = egui::TextEdit::singleline(&mut buffers.ai_key).password(true).hint_text(hint).desired_width(280.0);
        let field = ui.add(field);
        widgets::name(&field, Role::PasswordInput, "Endpoint key");
        ui.end_row();
    });
    ui.horizontal(|ui| {
        if widgets::primary_button(ui, icons::TASK, "Save", true).clicked() {
            let base_url = buffers.ai_base_url.trim().to_owned();
            actions.push(Action::SetAiProvider(AiProvider::OpenAi { base_url, model: buffers.ai_model.trim().to_owned() }));
            if !buffers.ai_key.is_empty() {
                actions.push(Action::SaveAiKey(std::mem::take(&mut buffers.ai_key)));
            }
        }
        if view.study.has_key && widgets::subtle_button(ui, icons::TRASH, "Forget the key", true).clicked() {
            actions.push(Action::SaveAiKey(String::new()));
        }
    });
    note(ui, "The key is kept on this device apart from your notes, and only sent to this address.");
}

fn dictionary(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let p = palette(ui);
    widgets::section_label(ui, "Dictionary");
    if !view.study.dictionary_available {
        return;
    }
    if let Some((done, total)) = view.study.dict_download {
        let text = format!("Downloading the dictionary: {:.0} of {:.0} MB", done as f64 / MB, total as f64 / MB);
        let bar = ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).text(text));
        widgets::name(&bar, Role::ProgressIndicator, "Dictionary download");
    } else if view.study.dict_indexing {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Preparing the dictionary…");
        });
    } else if view.study.dictionary_ready {
        note(ui, &format!("Offline dictionary ready: {ATTRIBUTION}."));
    } else {
        note(ui, "Double-click a word in a note or the transcript to look it up. The offline dictionary downloads once (47 MB).");
        if widgets::subtle_button(ui, icons::DOWNLOAD, "Download the dictionary", true).clicked() {
            actions.push(Action::DownloadDictionary);
        }
    }
    let mut online = view.prefs.online_dictionary;
    ui.horizontal(|ui| {
        if widgets::switch(ui, &mut online, "Look words up online too").changed() {
            actions.push(Action::SetOnlineDictionary(online));
        }
        ui.label(RichText::new("Look words up online too (dictionaryapi.dev)").size(size::CAPTION).color(p.text));
    });
}
