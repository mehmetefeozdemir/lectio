//! Store art drawn in code, like the icons (ADR-039): Google Play's feature
//! graphic (1024 × 500) and 512 × 512 icon. Only tests build this module;
//! `cargo test --bin lectio write_store_art -- --ignored` writes the files
//! into the Android store listing (fastlane `metadata/`).

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use tiny_skia::{GradientStop, LinearGradient, Paint, Pixmap, Point, Rect, SpreadMode, Transform};

use super::fonts::INTER_SEMIBOLD;
use super::icon::{BOTTOM, TOP, draw_notebook, ios_png};

pub const FEATURE_WIDTH: u32 = 1024;
pub const FEATURE_HEIGHT: u32 = 500;
pub const PLAY_ICON_PX: u32 = 512;
const NAME: &str = "Lectio";
const TAGLINE: &str = "Lecture notes linked to the recording";
const NAME_PX: f32 = 112.0;
const TAGLINE_PX: f32 = 34.0;
const TEXT_LEFT: f32 = 452.0;
/// The notebook's centre on the 1024 icon grid, and where and how large it goes.
const NOTEBOOK_CENTRE: (f32, f32) = (512.0, 524.0);
const NOTEBOOK_AT: (f32, f32) = (232.0, 250.0);
const NOTEBOOK_SCALE: f32 = 0.5;
const WHITE: [u8; 3] = [255, 255, 255];
const TAGLINE_COLOUR: [u8; 3] = [0xE0, 0xDD, 0xFF];

/// Writes `text` with its baseline at (`x`, `baseline`), blending `rgb` over
/// what is there by each pixel's coverage.
fn draw_text(pixmap: &mut Pixmap, text: &str, size: f32, x: f32, baseline: f32, rgb: [u8; 3]) -> Option<()> {
    let font = FontRef::try_from_slice(INTER_SEMIBOLD).ok()?;
    let scaled = font.as_scaled(PxScale::from(size));
    let (width, height) = (pixmap.width() as i32, pixmap.height() as i32);
    let mut caret = x;
    let mut previous = None;
    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(prev) = previous {
            caret += scaled.kern(prev, id);
        }
        let glyph = id.with_scale_and_position(PxScale::from(size), ab_glyph::point(caret, baseline));
        caret += scaled.h_advance(id);
        previous = Some(id);
        let Some(outline) = font.outline_glyph(glyph) else { continue };
        let bounds = outline.px_bounds();
        let pixels = pixmap.pixels_mut();
        outline.draw(|gx, gy, coverage| {
            let (px, py) = (bounds.min.x as i32 + gx as i32, bounds.min.y as i32 + gy as i32);
            if px < 0 || py < 0 || px >= width || py >= height {
                return;
            }
            let dst = &mut pixels[(py * width + px) as usize];
            let mix = |src: u8, under: u8| (f32::from(src) * coverage + f32::from(under) * (1.0 - coverage)).round() as u8;
            // The background is opaque, so premultiplied and straight colours match.
            if let Some(c) = tiny_skia::PremultipliedColorU8::from_rgba(mix(rgb[0], dst.red()), mix(rgb[1], dst.green()), mix(rgb[2], dst.blue()), 255) {
                *dst = c;
            }
        });
    }
    Some(())
}

/// The feature graphic: the icon's gradient, the notebook on the left, the
/// name and what it does on the right. Opaque (Play refuses transparency).
pub fn feature_graphic() -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(FEATURE_WIDTH, FEATURE_HEIGHT)?;
    let shader = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(0.0, FEATURE_HEIGHT as f32),
        vec![
            GradientStop::new(0.0, tiny_skia::Color::from_rgba8(TOP[0], TOP[1], TOP[2], 255)),
            GradientStop::new(1.0, tiny_skia::Color::from_rgba8(BOTTOM[0], BOTTOM[1], BOTTOM[2], 255)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )?;
    let background = Paint { shader, ..Paint::default() };
    pixmap.fill_rect(Rect::from_xywh(0.0, 0.0, FEATURE_WIDTH as f32, FEATURE_HEIGHT as f32)?, &background, Transform::identity(), None);
    // The notebook is drawn on the 1024 grid of a 1024-wide canvas, so grid
    // units are pixels here; move and shrink it into place.
    let t = Transform::from_translate(-NOTEBOOK_CENTRE.0, -NOTEBOOK_CENTRE.1)
        .post_scale(NOTEBOOK_SCALE, NOTEBOOK_SCALE)
        .post_translate(NOTEBOOK_AT.0, NOTEBOOK_AT.1);
    draw_notebook(&mut pixmap, false, t)?;
    draw_text(&mut pixmap, NAME, NAME_PX, TEXT_LEFT, 250.0, WHITE)?;
    draw_text(&mut pixmap, TAGLINE, TAGLINE_PX, TEXT_LEFT + 4.0, 310.0, TAGLINE_COLOUR)?;
    Some(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(pixmap: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let p = pixmap.pixel(x, y).unwrap().demultiply();
        [p.red(), p.green(), p.blue(), p.alpha()]
    }

    #[test]
    fn the_feature_graphic_is_opaque_with_the_notebook_and_the_name() {
        let pixmap = feature_graphic().unwrap();
        assert_eq!((pixmap.width(), pixmap.height()), (1024, 500));
        assert!(pixmap.pixels().iter().all(|p| p.alpha() == 255), "opaque everywhere");
        assert_eq!(at(&pixmap, 150, 250), [255, 255, 255, 255], "a notebook page");
        let white_text = (TEXT_LEFT as u32..TEXT_LEFT as u32 + 300)
            .flat_map(|x| (160..250).map(move |y| (x, y)))
            .filter(|&(x, y)| at(&pixmap, x, y)[..3] == [255, 255, 255])
            .count();
        assert!(white_text > 2_000, "the name is written ({white_text} white pixels)");
        assert!(at(&pixmap, 1000, 480)[2] > 150, "indigo in the corner");
    }

    /// Writes the Play listing's art next to the other listing files.
    #[test]
    #[ignore]
    fn write_store_art() {
        let folder = std::path::Path::new("mobile/android/fastlane/metadata/android/en-US/images");
        std::fs::create_dir_all(folder).unwrap();
        feature_graphic().unwrap().save_png(folder.join("featureGraphic.png")).unwrap();
        std::fs::write(folder.join("icon.png"), ios_png(PLAY_ICON_PX).unwrap()).unwrap();
    }
}
