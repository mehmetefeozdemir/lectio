//! Paper view (Phase 4b, ADR-025): the session as a document in a centred
//! column. It draws the same blocks every export writes
//! (`lectio_core::paper`): headings, notes with their time, formulas,
//! photos and sketches, and each slide (with its drawing) once, just before
//! the first note about it. Copy gives Markdown; Print and Export make a
//! PDF, a Word document or Markdown.

use eframe::egui::{self, RichText, Role};
use lectio_core::Session;
use lectio_core::material::Attachment;
use lectio_core::paper::{Block, document, note_text, photo_word, time_prefix};
use lectio_core::timeline::format_offset;

use super::theme::{self, palette, size, space};
use super::{View, icons, notes, widgets};
use crate::action::Action;
use crate::material::PageView;
use crate::platform::DocFormat;

const COLUMN_WIDTH: f32 = 720.0;
const READING_SIZE: f32 = 17.0;
const LINE_HEIGHT: f32 = 27.0;
/// Slides in the paper view: wide enough to read, narrow enough to keep the notes in view.
const SLIDE_WIDTH: f32 = 560.0;

pub fn show(ui: &mut egui::Ui, view: &View, session: &Session, actions: &mut Vec<Action>) {
    let options = view.paper.options;
    let blocks = document(view.library, session.id, &options).unwrap_or_default();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let width = ui.available_width().min(COLUMN_WIDTH);
        let margin = ((ui.available_width() - width) / 2.0).max(space::L);
        ui.horizontal(|ui| {
            ui.add_space(margin);
            ui.vertical(|ui| {
                ui.set_width(width);
                ui.add_space(space::L);
                toolbar(ui, view, actions);
                ui.add_space(space::L);
                if blocks.len() <= 1 {
                    title_only(ui, &blocks);
                    ui.label(RichText::new("No notes in this session yet.").color(palette(ui).text_muted));
                }
                for block in &blocks {
                    block_view(ui, view, session, block, &options);
                }
                ui.add_space(space::XXL);
            });
        });
    });
}

fn title_only(ui: &mut egui::Ui, blocks: &[Block]) {
    if blocks.is_empty() {
        ui.label(RichText::new("Paper view").font(theme::semibold(size::HEADING)));
    }
}

/// Back · Copy · Print · Export ▾ · options, and an export's progress.
fn toolbar(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let busy = view.paper.export.is_some();
    ui.horizontal_wrapped(|ui| {
        if widgets::subtle_button(ui, icons::CLOSE, "Back to the session", true).clicked() {
            actions.push(Action::SetReading(false));
        }
        if widgets::subtle_button(ui, icons::COPY, "Copy all", true).clicked() {
            actions.push(Action::CopyAll);
        }
        if widgets::subtle_button(ui, icons::PRINT, "Print", view.paper.can_print && !busy).clicked() {
            actions.push(Action::Print);
        }
        let menu = ui.add_enabled_ui(view.paper.can_print && !busy, |ui| {
            ui.menu_button(widgets::icon_text(icons::EXPORT, "Export", theme::regular(size::CAPTION), p.text), |ui| {
                let formats = [
                    (DocFormat::Pdf, icons::FILE_PDF, "PDF document"),
                    (DocFormat::Docx, icons::FILE_DOC, "Word document (.docx)"),
                    (DocFormat::Markdown, icons::FILE_TEXT, "Markdown text"),
                ];
                for (format, icon, label) in formats {
                    let item = ui.button(widgets::icon_text(icon, label, theme::regular(size::CAPTION), p.text));
                    widgets::name(&item, Role::Button, &format!("Export as {label}"));
                    if item.clicked() {
                        actions.push(Action::Export(format));
                    }
                }
            })
        });
        widgets::name(&menu.inner.response, Role::Button, "Export");
    });
    ui.horizontal_wrapped(|ui| {
        let mut times = view.paper.options.times;
        if widgets::switch(ui, &mut times, "Show times").changed() {
            actions.push(Action::SetPaperTimes(times));
        }
        ui.label(RichText::new("Show times").size(size::CAPTION).color(p.text));
        ui.add_space(space::M);
        let mut transcript = view.paper.options.transcript;
        if widgets::switch(ui, &mut transcript, "Include the transcript").changed() {
            actions.push(Action::SetPaperTranscript(transcript));
        }
        ui.label(RichText::new("Include the transcript").size(size::CAPTION).color(p.text));
    });
    if let Some((format, done, total)) = view.paper.export {
        let text = format!("Preparing the {} file: picture {} of {total}…", format.name(), done.min(total));
        let bar = ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).text(text));
        widgets::name(&bar, Role::ProgressIndicator, "Export progress");
    }
}

fn time_and_labels(ui: &mut egui::Ui, offset_ms: u64, labels: &[String], options: &lectio_core::paper::PaperOptions) {
    let p = palette(ui);
    let time = time_prefix(offset_ms, options);
    if !time.is_empty() {
        ui.label(RichText::new(time.trim()).font(theme::mono(size::SMALL)).color(p.text_muted));
    }
    if !labels.is_empty() {
        ui.label(RichText::new(format!("({})", labels.join(", "))).size(size::SMALL).color(p.text_muted));
    }
}

fn block_view(ui: &mut egui::Ui, view: &View, session: &Session, block: &Block, options: &lectio_core::paper::PaperOptions) {
    let p = palette(ui);
    match block {
        Block::Title { title, subtitle } => {
            let heading = ui.label(RichText::new(title).font(theme::semibold(size::HEADING)).color(p.text));
            widgets::name(&heading, Role::Heading, title);
            ui.label(RichText::new(subtitle).size(size::CAPTION).color(p.text_muted));
            ui.add_space(space::XL);
        }
        Block::Heading { text, .. } => {
            ui.add_space(space::M);
            let heading = ui.label(RichText::new(text).font(theme::semibold(size::TITLE)).color(p.text));
            widgets::name(&heading, Role::Heading, text);
            ui.add_space(space::S);
        }
        Block::Note { offset_ms, text, labels } => {
            ui.horizontal_wrapped(|ui| time_and_labels(ui, *offset_ms, labels, options));
            let body = RichText::new(note_text(text)).size(READING_SIZE).line_height(Some(LINE_HEIGHT));
            let body = if text.is_empty() { body.italics().color(p.text_muted) } else { body.color(p.text) };
            ui.add(egui::Label::new(body).wrap());
            ui.add_space(space::M);
        }
        Block::Formula { offset_ms, latex, labels } => {
            ui.horizontal_wrapped(|ui| time_and_labels(ui, *offset_ms, labels, options));
            super::formula::show(ui, latex, READING_SIZE * 1.3, egui::Sense::hover());
            ui.add_space(space::M);
        }
        Block::Slide { deck, page, deck_title, caption, offset_ms } => {
            ui.add_space(space::S);
            if let PageView::Ready(texture) = view.material.page(*deck, *page) {
                let alt = format!("{caption} ({deck_title}, page {page})");
                let width = ui.available_width().min(SLIDE_WIDTH);
                let rect = ui.add(egui::Image::from_texture(texture).max_width(width).alt_text(alt)).rect;
                if let Some(d) = session.decks.iter().find(|d| d.id == *deck) {
                    super::ink::paint(ui.painter(), rect, d.ink_on(*page));
                }
            } else {
                ui.spinner();
            }
            let when = match offset_ms {
                Some(ms) if options.times => format!(" · {}", format_offset(*ms)),
                _ => String::new(),
            };
            let line = format!("{caption} · {deck_title}, page {page}{when}");
            ui.label(widgets::icon_text(icons::SLIDES, &line, theme::regular(size::SMALL), p.text_muted));
            ui.add_space(space::L);
        }
        Block::Image { blob, kind, width, height, offset_ms, caption } => {
            ui.horizontal_wrapped(|ui| {
                time_and_labels(ui, *offset_ms, &[], options);
                ui.label(RichText::new(format!("{} {caption}", photo_word(*kind))).size(READING_SIZE).color(p.text));
            });
            let attachment = Attachment { kind: *kind, blob: blob.clone(), width: *width, height: *height };
            notes::attachment_image(ui, view, &attachment, ui.available_width());
            ui.add_space(space::L);
        }
        Block::Transcript { lines } => {
            ui.add_space(space::L);
            let heading = ui.label(RichText::new("Transcript").font(theme::semibold(size::TITLE)).color(p.text));
            widgets::name(&heading, Role::Heading, "Transcript");
            for (ms, text) in lines {
                ui.horizontal_wrapped(|ui| {
                    time_and_labels(ui, *ms, &[], options);
                    ui.label(RichText::new(text).size(READING_SIZE).color(p.text));
                });
            }
        }
    }
}
