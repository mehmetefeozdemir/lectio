//! Lectio's design tokens (ADR-014): type scale, spacing, radii, colours for
//! light and dark themes, bundled fonts, and the egui `Style` built from them.
//! Views read these values instead of hard-coding numbers or colours.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Shadow, Stroke, TextStyle, Theme,
    ThemePreference, Visuals,
};
use lectio_core::LabelKind;

/// Type scale (px). Two weights: regular and semibold.
pub mod size {
    pub const SMALL: f32 = 12.0;
    pub const CAPTION: f32 = 13.0;
    pub const BODY: f32 = 15.0;
    pub const TITLE: f32 = 18.0;
    pub const HEADING: f32 = 24.0;
    pub const ICON: f32 = 16.0;
    pub const ICON_LARGE: f32 = 18.0;
}

/// Spacing scale (px).
pub mod space {
    pub const XS: f32 = 4.0;
    pub const S: f32 = 8.0;
    pub const M: f32 = 12.0;
    pub const L: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
}

/// Corner radii (px): controls/chips and cards/panels.
pub mod radius {
    pub const CONTROL: u8 = 6;
    pub const CARD: u8 = 10;
}

const SEMIBOLD_FAMILY: &str = "semibold";
const ICON_FAMILY: &str = "icons";
const FONT_INTER: &str = "Inter";
const FONT_INTER_SEMIBOLD: &str = "Inter-SemiBold";
const FONT_MONO: &str = "JetBrainsMono";
const FONT_ICONS: &str = "Phosphor";

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD_FAMILY.into()))
}

/// Phosphor icons only (see `ui/icons.rs`).
pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ICON_FAMILY.into()))
}

pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

/// Monospace is for timestamps only.
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// Every colour the views use, per theme.
pub struct Palette {
    /// Main canvas behind everything.
    pub bg: Color32,
    /// Cards, inputs, the notes feed.
    pub surface: Color32,
    /// Sidebar and quiet fills (subtle buttons, pills).
    pub surface_alt: Color32,
    pub hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub accent_soft: Color32,
    pub record: Color32,
    pub on_record: Color32,
    pub danger_text: Color32,
    pub warning_fill: Color32,
    pub warning_text: Color32,
    pub error_fill: Color32,
    pub error_text: Color32,
    /// Timeline track and level-meter background.
    pub track: Color32,
    /// Plain (unlabelled) timeline markers.
    pub marker: Color32,
    /// The paper of sketches and slides (always light).
    pub paper: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const LIGHT: Palette = Palette {
    bg: rgb(0xF6F6F8),
    surface: rgb(0xFFFFFF),
    surface_alt: rgb(0xEEEEF2),
    hover: rgb(0xE6E6EC),
    border: rgb(0xDEDEE5),
    text: rgb(0x1C1C22),
    text_muted: rgb(0x5C5C6A),
    accent: rgb(0x3D4BC4),
    on_accent: rgb(0xFFFFFF),
    accent_soft: rgb(0xE7E9FA),
    record: rgb(0xC9302C),
    on_record: rgb(0xFFFFFF),
    danger_text: rgb(0xB02A25),
    warning_fill: rgb(0xFBF0D5),
    warning_text: rgb(0x5F4500),
    error_fill: rgb(0xFBE3E1),
    error_text: rgb(0x7E1F1A),
    track: rgb(0xE2E2E8),
    marker: rgb(0x7A7A88),
    paper: rgb(0xFFFFFF),
};

pub const DARK: Palette = Palette {
    bg: rgb(0x131317),
    surface: rgb(0x1B1B21),
    surface_alt: rgb(0x23232A),
    hover: rgb(0x2C2C35),
    border: rgb(0x30303A),
    text: rgb(0xEDEDF2),
    text_muted: rgb(0xA3A3B2),
    accent: rgb(0x8E99F5),
    on_accent: rgb(0x14142A),
    accent_soft: rgb(0x262A48),
    record: rgb(0xF0645B),
    on_record: rgb(0x1A0A09),
    danger_text: rgb(0xF2928A),
    warning_fill: rgb(0x3A3120),
    warning_text: rgb(0xF0D48E),
    error_fill: rgb(0x40211F),
    error_text: rgb(0xF6B4AD),
    track: rgb(0x34343E),
    marker: rgb(0x9A9AAA),
    paper: rgb(0xFFFFFF),
};

pub fn palette_for(dark: bool) -> &'static Palette {
    if dark { &DARK } else { &LIGHT }
}

pub fn palette(ui: &egui::Ui) -> &'static Palette {
    palette_for(ui.visuals().dark_mode)
}

/// Desaturated label colours: a soft fill with readable text, and a
/// stronger tone for timeline markers.
#[derive(Clone, Copy)]
pub struct LabelColors {
    pub fill: Color32,
    pub text: Color32,
    pub marker: Color32,
}

pub fn label_colors(kind: LabelKind, dark: bool) -> LabelColors {
    use LabelKind as K;
    let (fill, text, marker) = match (kind, dark) {
        (K::Important, false) => (0xFBE3E0, 0x84271D, 0xC4574B),
        (K::Review, false) => (0xF7EDD2, 0x634904, 0xB0861F),
        (K::Task, false) => (0xDDF0E3, 0x1B5631, 0x3E9459),
        (K::Definition, false) => (0xE2E8F8, 0x22407D, 0x4C6FC0),
        (K::Heading, false) => (0xE8E8EE, 0x2A2A35, 0x6C6C7E),
        (K::Important, true) => (0x3B2422, 0xF3BBB3, 0xE07C70),
        (K::Review, true) => (0x3A3222, 0xECD192, 0xD4AE4E),
        (K::Task, true) => (0x1F3427, 0xAADFBA, 0x67BE83),
        (K::Definition, true) => (0x212B42, 0xB3C6F2, 0x7F9DE6),
        (K::Heading, true) => (0x2A2A33, 0xE3E3EA, 0xA6A6B6),
    };
    LabelColors { fill: rgb(fill), text: rgb(text), marker: rgb(marker) }
}

/// Registers fonts and the light and dark styles, and follows the OS theme.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    for (theme, dark) in [(Theme::Light, false), (Theme::Dark, true)] {
        let mut style = (*ctx.style_of(theme)).clone();
        apply(&mut style, dark);
        ctx.set_style_of(theme, style);
    }
    ctx.set_theme(ThemePreference::System);
}

/// System → Light → Dark → System, for the toolbar's theme button.
pub fn next_theme(current: ThemePreference) -> ThemePreference {
    match current {
        ThemePreference::System => ThemePreference::Light,
        ThemePreference::Light => ThemePreference::Dark,
        ThemePreference::Dark => ThemePreference::System,
    }
}

pub fn theme_name(preference: ThemePreference) -> &'static str {
    match preference {
        ThemePreference::System => "System",
        ThemePreference::Light => "Light",
        ThemePreference::Dark => "Dark",
    }
}

fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let data = |bytes: &'static [u8]| Arc::new(FontData::from_static(bytes));
    fonts.font_data.insert(FONT_INTER.into(), data(include_bytes!("../../assets/fonts/Inter-Regular.ttf")));
    fonts.font_data.insert(FONT_INTER_SEMIBOLD.into(), data(include_bytes!("../../assets/fonts/Inter-SemiBold.ttf")));
    fonts.font_data.insert(FONT_MONO.into(), data(include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf")));
    fonts.font_data.insert(FONT_ICONS.into(), data(include_bytes!("../../assets/fonts/Phosphor.ttf")));
    // Icons get their own family. They can't share a fallback list with
    // Inter: Inter has glyphs at some of the private-use codepoints Phosphor
    // uses, and Phosphor has glyphs for some Latin letters, so whichever font
    // came first would hide the other's characters. Mixed "icon + text"
    // strings are built from two parts with `widgets::icon_text`.
    let defaults = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let with = |text: &str| [vec![text.to_owned()], defaults.clone()].concat();
    fonts.families.insert(FontFamily::Proportional, with(FONT_INTER));
    fonts.families.insert(FontFamily::Name(SEMIBOLD_FAMILY.into()), with(FONT_INTER_SEMIBOLD));
    fonts.families.insert(FontFamily::Monospace, with(FONT_MONO));
    fonts.families.insert(FontFamily::Name(ICON_FAMILY.into()), vec![FONT_ICONS.to_owned()]);
    fonts
}

fn apply(style: &mut egui::Style, dark: bool) {
    let p = palette_for(dark);
    style.text_styles = [
        (TextStyle::Heading, semibold(size::HEADING)),
        (TextStyle::Body, regular(size::BODY)),
        (TextStyle::Button, regular(size::CAPTION)),
        (TextStyle::Small, regular(size::SMALL)),
        (TextStyle::Monospace, mono(size::CAPTION)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(space::S, space::S);
    style.spacing.button_padding = egui::vec2(space::S + 2.0, space::XS + 1.0);
    style.spacing.interact_size.y = 28.0;
    style.spacing.menu_margin = egui::Margin::same(space::S as i8);
    style.spacing.window_margin = egui::Margin::same(space::L as i8);
    style.visuals = visuals(p, dark);
}

fn visuals(p: &Palette, dark: bool) -> Visuals {
    let mut v = if dark { Visuals::dark() } else { Visuals::light() };
    let control = CornerRadius::same(radius::CONTROL);
    let card = CornerRadius::same(radius::CARD);
    let border = Stroke::new(1.0, p.border);
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = border;
    v.window_corner_radius = card;
    v.menu_corner_radius = card;
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow { offset: [0, 4], blur: 18, spread: 0, color: Color32::from_black_alpha(if dark { 90 } else { 28 }) };
    v.extreme_bg_color = p.surface;
    v.text_edit_bg_color = Some(p.surface);
    v.faint_bg_color = p.surface_alt;
    v.code_bg_color = p.surface_alt;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning_text;
    v.error_fg_color = p.danger_text;
    v.override_text_color = None;
    v.weak_text_color = Some(p.text_muted);
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(2.0, p.accent);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.indent_has_left_vline = false;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = border;
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = control;
    for (state, fill, stroke) in [
        (&mut w.inactive, p.surface_alt, Stroke::NONE),
        (&mut w.hovered, p.hover, Stroke::new(1.0, p.border)),
        (&mut w.active, p.accent_soft, Stroke::new(1.0, p.accent)),
        (&mut w.open, p.hover, Stroke::new(1.0, p.border)),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = stroke;
        state.fg_stroke = Stroke::new(1.0, p.text);
        state.corner_radius = control;
        state.expansion = 0.0;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

// ---------- contrast (WCAG 2.x) ----------
    
    fn channel(c: u8) -> f32 {
        let c = c as f32 / 255.0;
        if c <= 0.039_28 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }
    
    fn luminance(c: Color32) -> f32 {
        0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
    }
    
    /// WCAG contrast ratio between two colours (1.0 … 21.0).
    pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    /// AA for normal text, and for graphical objects (markers, icons).
    const AA_TEXT: f32 = 4.5;
    const AA_GRAPHICS: f32 = 3.0;

    #[test]
    fn contrast_maths_matches_known_values() {
        assert!((contrast_ratio(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 0.01);
    }

    #[test]
    fn text_is_readable_on_every_background_in_both_themes() {
        for (name, p) in [("light", &LIGHT), ("dark", &DARK)] {
            let pairs = [
                ("text/bg", p.text, p.bg),
                ("text/surface", p.text, p.surface),
                ("text/surface_alt", p.text, p.surface_alt),
                ("text/hover", p.text, p.hover),
                ("muted/bg", p.text_muted, p.bg),
                ("muted/surface", p.text_muted, p.surface),
                ("muted/surface_alt", p.text_muted, p.surface_alt),
                ("on_accent/accent", p.on_accent, p.accent),
                ("accent/surface", p.accent, p.surface),
                ("text/accent_soft", p.text, p.accent_soft),
                ("on_record/record", p.on_record, p.record),
                ("danger/surface", p.danger_text, p.surface),
                ("warning", p.warning_text, p.warning_fill),
                ("error", p.error_text, p.error_fill),
            ];
            for (pair, fg, bg) in pairs {
                let ratio = contrast_ratio(fg, bg);
                assert!(ratio >= AA_TEXT, "{name} {pair}: {ratio:.2} < {AA_TEXT}");
            }
        }
    }

    #[test]
    fn label_chips_and_markers_meet_wcag_aa_in_both_themes() {
        for dark in [false, true] {
            let p = palette_for(dark);
            for kind in LabelKind::ALL {
                let c = label_colors(kind, dark);
                let text = contrast_ratio(c.text, c.fill);
                assert!(text >= AA_TEXT, "{kind:?} chip (dark={dark}): {text:.2}");
                for bg in [p.surface, p.bg] {
                    let marker = contrast_ratio(c.marker, bg);
                    assert!(marker >= AA_GRAPHICS, "{kind:?} marker (dark={dark}): {marker:.2}");
                }
            }
            let plain = contrast_ratio(p.marker, p.surface);
            assert!(plain >= AA_GRAPHICS, "plain marker (dark={dark}): {plain:.2}");
        }
    }

    #[test]
    fn theme_button_cycles_through_all_three_choices() {
        let start = ThemePreference::System;
        let cycle = next_theme(next_theme(next_theme(start)));
        assert_eq!(cycle, start);
        assert_eq!(next_theme(start), ThemePreference::Light);
    }
}
