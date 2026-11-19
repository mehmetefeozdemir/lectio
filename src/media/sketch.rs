//! Freehand sketches (ADR-013): strokes drawn in the sketch window are
//! rasterized into a PNG with `tiny-skia`, so a sketch is stored and shown
//! exactly like a photo.

use tiny_skia::{Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// The sketch canvas, in pixels. Landscape, like a whiteboard.
pub const SKETCH_WIDTH: u32 = 960;
pub const SKETCH_HEIGHT: u32 = 600;
const PEN_WIDTH: f32 = 3.5;
const INK: [u8; 3] = [0x1A, 0x1A, 0x1A];

/// One pen-down → pen-up line, in canvas pixels.
pub type StrokePoints = Vec<[f32; 2]>;

pub fn is_blank(strokes: &[StrokePoints]) -> bool {
    strokes.iter().all(|s| s.is_empty())
}

/// Draws dark ink strokes on a white canvas and returns PNG bytes.
pub fn rasterize_png(strokes: &[StrokePoints]) -> Result<Vec<u8>, String> {
    if is_blank(strokes) {
        return Err("The sketch is empty.".into());
    }
    let mut pixmap = Pixmap::new(SKETCH_WIDTH, SKETCH_HEIGHT).ok_or("could not create the sketch canvas")?;
    pixmap.fill(Color::WHITE);
    let mut paint = Paint::default();
    paint.set_color_rgba8(INK[0], INK[1], INK[2], 255);
    paint.anti_alias = true;
    let pen = Stroke { width: PEN_WIDTH, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };

    for points in strokes.iter().filter(|s| !s.is_empty()) {
        if let [[x, y]] = points.as_slice() {
            // A single tap is a dot.
            if let Some(dot) = PathBuilder::from_circle(*x, *y, PEN_WIDTH / 2.0) {
                pixmap.fill_path(&dot, &paint, FillRule::Winding, Transform::identity(), None);
            }
            continue;
        }
        let mut path = PathBuilder::new();
        path.move_to(points[0][0], points[0][1]);
        for [x, y] in &points[1..] {
            path.line_to(*x, *y);
        }
        if let Some(path) = path.finish() {
            pixmap.stroke_path(&path, &paint, &pen, Transform::identity(), None);
        }
    }
    pixmap.encode_png().map_err(|e| format!("Saving the sketch failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(png: &[u8], x: u32, y: u32) -> [u8; 4] {
        let image = image::load_from_memory(png).unwrap().to_rgba8();
        image.get_pixel(x, y).0
    }

    #[test]
    fn a_stroke_becomes_dark_ink_on_a_white_png_of_the_canvas_size() {
        let strokes = vec![vec![[100.0, 100.0], [300.0, 100.0]]];

        let png = rasterize_png(&strokes).unwrap();

        assert!(png.starts_with(b"\x89PNG"));
        let image = image::load_from_memory(&png).unwrap();
        assert_eq!((image.width(), image.height()), (SKETCH_WIDTH, SKETCH_HEIGHT));
        assert!(pixel(&png, 200, 100)[0] < 80, "on the line: dark");
        assert_eq!(pixel(&png, 10, 10), [255, 255, 255, 255], "background: white");
    }

    #[test]
    fn a_single_tap_leaves_a_dot() {
        let png = rasterize_png(&[vec![[50.0, 60.0]]]).unwrap();
        assert!(pixel(&png, 50, 60)[0] < 128);
    }

    #[test]
    fn an_empty_sketch_is_not_saved() {
        assert!(rasterize_png(&[]).is_err());
        assert!(rasterize_png(&[vec![], vec![]]).is_err());
        assert!(is_blank(&[vec![]]));
    }
}
