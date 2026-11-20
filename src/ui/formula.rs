//! Formulas on screen (ADR-023): `media::math` draws a formula into an
//! image once per (formula, size, colour, screen scale); the texture is
//! kept in egui's memory and reused every frame.

use std::collections::HashMap;

use eframe::egui::{self, ColorImage, Id, RichText, Role, TextureHandle, TextureOptions};

use super::theme::{palette, size};
use super::widgets;
use crate::media::math;

/// Formulas kept on the graphics card; the cache is emptied when it grows past this.
const MAX_CACHED: usize = 400;

type Key = (String, u32, [u8; 3], u32);

#[derive(Clone, Default)]
struct Cache(HashMap<Key, Result<TextureHandle, String>>);

/// The formula's texture and its size in points, or why it can't be drawn.
fn texture(ctx: &egui::Context, latex: &str, size_pt: f32, color: egui::Color32) -> Result<(TextureHandle, egui::Vec2), String> {
    let ppp = ctx.pixels_per_point();
    let rgb = [color.r(), color.g(), color.b()];
    let key: Key = (latex.to_owned(), size_pt.to_bits(), rgb, ppp.to_bits());
    let id = Id::unique("formula-textures");
    let cached = ctx.data(|d| d.get_temp::<Cache>(id)).and_then(|c| c.0.get(&key).cloned());
    let made = match cached {
        Some(found) => found,
        None => {
            let made = math::render(latex, size_pt * ppp, rgb).map(|image| {
                let pixels = ColorImage::from_rgba_premultiplied([image.width as usize, image.height as usize], &image.rgba);
                ctx.load_texture(format!("formula:{latex}"), pixels, TextureOptions::LINEAR)
            });
            ctx.data_mut(|d| {
                let cache = d.get_temp_mut_or_default::<Cache>(id);
                if cache.0.len() >= MAX_CACHED {
                    cache.0.clear();
                }
                cache.0.insert(key, made.clone());
            });
            made
        }
    };
    made.map(|t| {
        let size = t.size_vec2() / ppp;
        (t, size)
    })
}

/// Shows a formula (or, if it can't be read, its source and the reason).
/// The accessible name is "Formula: <LaTeX>", which screen readers read out.
pub fn show(ui: &mut egui::Ui, latex: &str, size_pt: f32, sense: egui::Sense) -> egui::Response {
    let p = palette(ui);
    match texture(ui.ctx(), latex, size_pt, p.text) {
        Ok((texture, size)) => {
            let scale = (ui.available_width() / size.x).min(1.0);
            let image = egui::Image::new((texture.id(), size * scale)).sense(sense).alt_text(format!("Formula: {latex}"));
            let response = ui.add(image);
            widgets::name(&response, Role::Image, &format!("Formula: {latex}"));
            response
        }
        Err(why) => {
            let text = RichText::new(format!("{latex}  ({why})")).size(size::SMALL).color(p.danger_text);
            ui.add(egui::Label::new(text).wrap().sense(sense))
        }
    }
}

/// The live preview above the input while typing a formula.
pub fn preview(ui: &mut egui::Ui, latex: &str) {
    let p = palette(ui);
    if latex.trim().is_empty() {
        let hint = r"Type LaTeX, like \frac{a}{b}, x^2, \sqrt{x}, \alpha, \sum_{i=1}^{n}. Enter saves the formula.";
        ui.label(RichText::new(hint).size(size::SMALL).color(p.text_muted));
        return;
    }
    match math::layout(latex, 16.0) {
        Ok(_) => {
            show(ui, latex.trim(), size::BODY * 1.4, egui::Sense::hover());
        }
        Err(why) => {
            let label = ui.label(RichText::new(why).size(size::SMALL).color(p.danger_text));
            widgets::name(&label, Role::Label, "Formula problem");
        }
    }
}
