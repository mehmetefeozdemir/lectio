//! Lectio's app icon, drawn in code with tiny-skia (no image file to keep in
//! sync): an indigo rounded square with an open notebook, lines of notes on
//! the left page and a sound wave on the right. The desktop window uses it,
//! and `assets/icon/lectio-icon.png` (for the installers) is written from it.

use tiny_skia::{FillRule, GradientStop, LineCap, LinearGradient, Paint, PathBuilder, Pixmap, Point, Rect, SpreadMode, Stroke, Transform};

const TOP: [u8; 3] = [0x4F, 0x46, 0xE5];
const BOTTOM: [u8; 3] = [0x6D, 0x5B, 0xD0];
const PAGE: [u8; 3] = [0xFF, 0xFF, 0xFF];
const LINES: [u8; 3] = [0xB9, 0xB4, 0xF0];
const WAVE: [u8; 3] = [0x6D, 0x5B, 0xD0];
const WAVE_STEPS: u32 = 120;
const WAVE_CYCLES: f32 = 2.5;

fn paint(rgb: [u8; 3], alpha: u8) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(rgb[0], rgb[1], rgb[2], alpha);
    paint.anti_alias = true;
    paint
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let mut p = PathBuilder::new();
    p.move_to(x + r, y);
    p.line_to(x + w - r, y);
    p.quad_to(x + w, y, x + w, y + r);
    p.line_to(x + w, y + h - r);
    p.quad_to(x + w, y + h, x + w - r, y + h);
    p.line_to(x + r, y + h);
    p.quad_to(x, y + h, x, y + h - r);
    p.line_to(x, y + r);
    p.quad_to(x, y, x + r, y);
    p.close();
    p.finish()
}

/// The icon as straight (not premultiplied) RGBA, `size` × `size` pixels.
pub fn rgba(size: u32) -> Option<Vec<u8>> {
    let mut pixmap = Pixmap::new(size, size)?;
    let s = size as f32;
    let u = |v: f32| v * s / 1024.0;
    // Background: rounded square with a top-to-bottom gradient.
    let shader = LinearGradient::new(
        Point::from_xy(0.0, u(64.0)),
        Point::from_xy(0.0, u(960.0)),
        vec![
            GradientStop::new(0.0, tiny_skia::Color::from_rgba8(TOP[0], TOP[1], TOP[2], 255)),
            GradientStop::new(1.0, tiny_skia::Color::from_rgba8(BOTTOM[0], BOTTOM[1], BOTTOM[2], 255)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )?;
    let background = Paint { anti_alias: true, shader, ..Paint::default() };
    let square = rounded_rect(u(64.0), u(64.0), u(896.0), u(896.0), u(200.0))?;
    pixmap.fill_path(&square, &background, FillRule::Winding, Transform::identity(), None);
    // Shadow, then the two pages of the open notebook.
    let shadow = rounded_rect(u(214.0), u(316.0), u(608.0), u(440.0), u(36.0))?;
    pixmap.fill_path(&shadow, &paint([0, 0, 0], 50), FillRule::Winding, Transform::identity(), None);
    for x in [u(200.0), u(518.0)] {
        let page = rounded_rect(x, u(292.0), u(306.0), u(440.0), u(32.0))?;
        pixmap.fill_path(&page, &paint(PAGE, 255), FillRule::Winding, Transform::identity(), None);
    }
    // Left page: three lines of notes.
    let line = Stroke { width: u(26.0), line_cap: LineCap::Round, ..Stroke::default() };
    for (i, length) in [196.0, 160.0, 186.0].into_iter().enumerate() {
        let y = u(400.0 + 92.0 * i as f32);
        let mut p = PathBuilder::new();
        p.move_to(u(254.0), y);
        p.line_to(u(254.0 + length), y);
        if let Some(path) = p.finish() {
            pixmap.stroke_path(&path, &paint(LINES, 255), &line, Transform::identity(), None);
        }
    }
    // Right page: a sound wave.
    let mut wave = PathBuilder::new();
    let (x0, x1, mid) = (568.0, 774.0, 512.0);
    for step in 0..=WAVE_STEPS {
        let t = step as f32 / WAVE_STEPS as f32;
        let x = x0 + (x1 - x0) * t;
        let envelope = (std::f32::consts::PI * t).sin();
        let y = mid - 110.0 * envelope * (t * WAVE_CYCLES * std::f32::consts::TAU).sin();
        if step == 0 { wave.move_to(u(x), u(y)) } else { wave.line_to(u(x), u(y)) }
    }
    if let Some(path) = wave.finish() {
        let stroke = Stroke { width: u(22.0), line_cap: LineCap::Round, ..Stroke::default() };
        pixmap.stroke_path(&path, &paint(WAVE, 255), &stroke, Transform::identity(), None);
    }
    // The fold between the pages.
    if let Some(fold) = Rect::from_xywh(u(506.0), u(292.0), u(12.0), u(440.0)) {
        pixmap.fill_rect(fold, &paint([0xE4, 0xE2, 0xF6], 255), Transform::identity(), None);
    }
    Some(pixmap.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }).collect())
}

/// The icon as PNG (for the installers; written by the test below).
#[cfg(test)]
pub fn png(size: u32) -> Result<Vec<u8>, String> {
    let pixels = rgba(size).ok_or("could not draw the icon")?;
    let image = image::RgbaImage::from_raw(size, size, pixels).ok_or("bad icon size")?;
    let mut out = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_is_square_with_transparent_corners_and_a_white_page() {
        let size = 256;
        let pixels = rgba(size).unwrap();
        let at = |x: u32, y: u32| {
            let i = ((y * size + x) * 4) as usize;
            [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
        };
        assert_eq!(pixels.len(), (size * size * 4) as usize);
        assert_eq!(at(2, 2)[3], 0, "outside the rounded square");
        assert_eq!(at(70, 140), [255, 255, 255, 255], "the left page");
        assert!(at(128, 30)[2] > 180, "indigo background");
    }

    /// `cargo test --bin lectio write_app_icon -- --ignored` writes the PNG
    /// the installers use (`tools/package/*`).
    #[test]
    #[ignore]
    fn write_app_icon() {
        std::fs::create_dir_all("assets/icon").unwrap();
        std::fs::write("assets/icon/lectio-icon.png", png(1024).unwrap()).unwrap();
    }
}
