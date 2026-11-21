//! Lectio's app icon, drawn in code with tiny-skia (no image file to keep in
//! sync): an indigo rounded square with an open notebook, lines of notes on
//! the left page and a sound wave on the right. The desktop window uses it,
//! and `assets/icon/lectio-icon.png` (for the installers) is written from it.

use tiny_skia::{FillRule, GradientStop, LineCap, LinearGradient, Paint, PathBuilder, Pixmap, Point, Rect, SpreadMode, Stroke, Transform};

pub(crate) const TOP: [u8; 3] = [0x4F, 0x46, 0xE5];
pub(crate) const BOTTOM: [u8; 3] = [0x6D, 0x5B, 0xD0];
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

/// Desktop icons carry their own rounded square with a transparent margin;
/// iPhone and iPad icons are full squares that the system rounds itself;
/// Android icons are layers (a background, the notebook on its own, and a
/// one-colour copy for themed icons) that the launcher masks and moves.
/// All but `Rounded` are drawn only by the (ignored) test that writes the
/// icon files.
#[derive(Clone, Copy, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
enum Shape {
    Rounded,
    FullBleed,
    AndroidForeground,
    AndroidMonochrome,
}

/// Android's always-visible circle is 66 of the layer's 108 dp; the
/// notebook's corners reach 382 units from the middle of the 1024 grid.
const ANDROID_ZOOM: f32 = 0.30 * 1024.0 / 382.0;
/// An adaptive icon layer: 108 dp at the highest density (4×).
#[cfg(test)]
const ANDROID_LAYER_PX: u32 = 432;

/// The icon as straight (not premultiplied) RGBA, `size` × `size` pixels.
pub fn rgba(size: u32) -> Option<Vec<u8>> {
    draw(size, Shape::Rounded)
}

/// The square the artwork is drawn in, on the 1024 grid.
const SQUARE_START: f32 = 64.0;
const SQUARE_SIZE: f32 = 896.0;

/// Where the 1024 grid lands for each shape: the full-bleed icon scales the
/// square to fill the canvas, the Android layers shrink the notebook into
/// the safe circle.
fn placement(shape: Shape, size: f32) -> Transform {
    let u = |v: f32| v * size / 1024.0;
    match shape {
        Shape::Rounded => Transform::identity(),
        Shape::FullBleed => {
            let zoom = 1024.0 / SQUARE_SIZE;
            Transform::from_translate(-u(SQUARE_START), -u(SQUARE_START)).post_scale(zoom, zoom)
        }
        Shape::AndroidForeground | Shape::AndroidMonochrome => Transform::from_translate(-u(512.0), -u(512.0))
            .post_scale(ANDROID_ZOOM, ANDROID_ZOOM)
            .post_translate(u(512.0), u(512.0)),
    }
}

fn draw(size: u32, shape: Shape) -> Option<Vec<u8>> {
    let mut pixmap = Pixmap::new(size, size)?;
    let t = placement(shape, size as f32);
    if matches!(shape, Shape::Rounded | Shape::FullBleed) {
        draw_background(&mut pixmap, shape, t)?;
    }
    draw_notebook(&mut pixmap, shape == Shape::AndroidMonochrome, t)?;
    Some(pixmap.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }).collect())
}

/// The indigo square with a top-to-bottom gradient.
fn draw_background(pixmap: &mut Pixmap, shape: Shape, t: Transform) -> Option<()> {
    let s = pixmap.width() as f32;
    let u = |v: f32| v * s / 1024.0;
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
    let corner = if shape == Shape::Rounded { u(200.0) } else { 0.0 };
    let square = rounded_rect(u(SQUARE_START), u(SQUARE_START), u(SQUARE_SIZE), u(SQUARE_SIZE), corner)?;
    pixmap.fill_path(&square, &background, FillRule::Winding, t, None);
    Some(())
}

/// The open notebook: notes on the left page, a sound wave on the right.
/// `monochrome` draws the pages in white and cuts the marks out of them
/// (themed Android icons use only the alpha of this layer).
pub(crate) fn draw_notebook(pixmap: &mut Pixmap, monochrome: bool, t: Transform) -> Option<()> {
    let s = pixmap.width() as f32;
    let u = |v: f32| v * s / 1024.0;
    let mark = |rgb: [u8; 3]| {
        let mut p = paint(rgb, 255);
        if monochrome {
            p.blend_mode = tiny_skia::BlendMode::Clear;
        }
        p
    };
    // Shadow (not in the one-colour layer), then the two pages.
    if !monochrome {
        let shadow = rounded_rect(u(214.0), u(316.0), u(608.0), u(440.0), u(36.0))?;
        pixmap.fill_path(&shadow, &paint([0, 0, 0], 50), FillRule::Winding, t, None);
    }
    for x in [u(200.0), u(518.0)] {
        let page = rounded_rect(x, u(292.0), u(306.0), u(440.0), u(32.0))?;
        pixmap.fill_path(&page, &paint(PAGE, 255), FillRule::Winding, t, None);
    }
    // Left page: three lines of notes.
    let line = Stroke { width: u(26.0), line_cap: LineCap::Round, ..Stroke::default() };
    for (i, length) in [196.0, 160.0, 186.0].into_iter().enumerate() {
        let y = u(400.0 + 92.0 * i as f32);
        let mut p = PathBuilder::new();
        p.move_to(u(254.0), y);
        p.line_to(u(254.0 + length), y);
        if let Some(path) = p.finish() {
            pixmap.stroke_path(&path, &mark(LINES), &line, t, None);
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
        pixmap.stroke_path(&path, &mark(WAVE), &stroke, t, None);
    }
    // The fold between the pages.
    if let Some(fold) = Rect::from_xywh(u(506.0), u(292.0), u(12.0), u(440.0)) {
        pixmap.fill_rect(fold, &mark([0xE4, 0xE2, 0xF6]), t, None);
    }
    Some(())
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

/// The iPhone/iPad icon: full-bleed and opaque RGB (the App Store refuses
/// icons with an alpha channel).
#[cfg(test)]
pub fn ios_png(size: u32) -> Result<Vec<u8>, String> {
    let pixels = draw(size, Shape::FullBleed).ok_or("could not draw the icon")?;
    let rgb: Vec<u8> = pixels.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let image = image::RgbImage::from_raw(size, size, rgb).ok_or("bad icon size")?;
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

    #[test]
    fn the_mobile_icon_fills_the_square_with_no_transparency() {
        let size = 256;
        let pixels = draw(size, Shape::FullBleed).unwrap();
        assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255), "opaque everywhere");
        let corner = &pixels[0..4];
        assert!(corner[2] > 180 && corner[0] < 120, "indigo reaches the corner: {corner:?}");
    }

    /// Android draws adaptive icons in many masks (circle, squircle, ...);
    /// only the middle 66 of the 108 dp layer is always visible.
    #[test]
    fn the_android_foreground_keeps_the_notebook_inside_the_safe_circle() {
        let size = 432;
        let pixels = draw(size, Shape::AndroidForeground).unwrap();
        let safe_radius = size as f32 * 33.0 / 108.0;
        let centre = size as f32 / 2.0;
        let mut drawn = 0;
        for (i, p) in pixels.as_chunks::<4>().0.iter().enumerate() {
            if p[3] == 0 {
                continue;
            }
            drawn += 1;
            let (x, y) = ((i as u32 % size) as f32 + 0.5, (i as u32 / size) as f32 + 0.5);
            assert!(((x - centre).powi(2) + (y - centre).powi(2)).sqrt() <= safe_radius, "pixel ({x}, {y}) is outside the safe zone");
        }
        assert!(drawn > 10_000, "the notebook is drawn");
        assert_eq!(pixels[3], 0, "transparent corner: the background is its own layer");
    }

    #[test]
    fn the_android_monochrome_layer_is_white_with_the_lines_cut_out() {
        let size = 1024;
        let pixels = draw(size, Shape::AndroidMonochrome).unwrap();
        let at = |x: u32, y: u32| {
            let i = ((y * size + x) * 4) as usize;
            [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
        };
        assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 0 || p[..3] == [255, 255, 255]), "only white, at any opacity");
        assert_eq!(at(266, 667)[3], 255, "a page");
        assert_eq!(at(342, 422)[3], 0, "the first line of notes is a hole");
    }

    /// `cargo test --bin lectio write_app_icon -- --ignored` writes the PNGs
    /// the installers (`tools/package/*`) and the iOS app use.
    #[test]
    #[ignore]
    fn write_app_icon() {
        std::fs::create_dir_all("assets/icon").unwrap();
        std::fs::write("assets/icon/lectio-icon.png", png(1024).unwrap()).unwrap();
        std::fs::write("assets/icon/lectio-icon-ios.png", ios_png(1024).unwrap()).unwrap();
        // 108 dp at xxxhdpi (4×) = 432 px; the background layer is a gradient
        // drawable in the Android project.
        for (shape, name) in [(Shape::AndroidForeground, "foreground"), (Shape::AndroidMonochrome, "monochrome")] {
            let pixels = draw(ANDROID_LAYER_PX, shape).unwrap();
            let image = image::RgbaImage::from_raw(ANDROID_LAYER_PX, ANDROID_LAYER_PX, pixels).unwrap();
            image.save(format!("assets/icon/lectio-icon-android-{name}.png")).unwrap();
        }
    }
}
