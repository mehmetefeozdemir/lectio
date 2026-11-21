//! Slide ink in pictures (ADR-024): draws a page's strokes onto the rendered
//! page with tiny-skia, for exports on every platform. On screen each app
//! draws the same strokes with its own UI toolkit, using these colours,
//! widths and order.

use lectio_core::ink::{InkStroke, InkTool};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Stroke, Transform};

use crate::raster::Rgba;

/// Pen strokes are opaque; highlighter strokes let the slide show through.
pub const PEN_ALPHA: u8 = 255;
pub const HIGHLIGHT_ALPHA: u8 = 96;
/// The highlighter is drawn wider than the width stored for it.
pub const HIGHLIGHT_SCALE: f32 = 4.0;

/// Width as drawn, as a fraction of the page width (the highlighter widened).
/// The phone apps scale this by the page's on-screen width themselves.
pub fn relative_width(stroke: &InkStroke) -> f32 {
    let scale = if stroke.tool == InkTool::Highlighter { HIGHLIGHT_SCALE } else { 1.0 };
    stroke.width * scale
}

/// Width on a page `page_width` pixels wide, at least one pixel.
pub fn stroke_width(stroke: &InkStroke, page_width: f32) -> f32 {
    (relative_width(stroke) * page_width).max(1.0)
}

pub fn alpha(stroke: &InkStroke) -> u8 {
    if stroke.tool == InkTool::Highlighter { HIGHLIGHT_ALPHA } else { PEN_ALPHA }
}

/// Highlighters first, so pen strokes stay on top of them.
pub fn in_drawing_order(strokes: &[InkStroke]) -> impl Iterator<Item = &InkStroke> {
    let highlighters = strokes.iter().filter(|s| s.tool == InkTool::Highlighter);
    highlighters.chain(strokes.iter().filter(|s| s.tool == InkTool::Pen))
}

/// Draws the strokes over the page and returns the result as PNG.
pub fn page_png(page: &Rgba, strokes: &[InkStroke]) -> Result<Vec<u8>, String> {
    let mut pixmap = page.to_pixmap()?;
    let (pw, ph) = (page.width as f32, page.height as f32);
    for stroke in in_drawing_order(strokes) {
        let [r, g, b] = stroke.color.rgb();
        let mut paint = Paint::default();
        paint.set_color_rgba8(r, g, b, alpha(stroke));
        paint.anti_alias = true;
        let width = stroke_width(stroke, pw);
        let points: Vec<(f32, f32)> = stroke.points.iter().map(|[x, y]| (x * pw, y * ph)).collect();
        if let [(x, y)] = points.as_slice() {
            if let Some(dot) = PathBuilder::from_circle(*x, *y, width / 2.0) {
                pixmap.fill_path(&dot, &paint, FillRule::Winding, Transform::identity(), None);
            }
            continue;
        }
        let mut path = PathBuilder::new();
        if let Some((x, y)) = points.first() {
            path.move_to(*x, *y);
        }
        for (x, y) in points.iter().skip(1) {
            path.line_to(*x, *y);
        }
        if let Some(path) = path.finish() {
            let pen = Stroke { width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
            pixmap.stroke_path(&path, &paint, &pen, Transform::identity(), None);
        }
    }
    pixmap.encode_png().map_err(|e| format!("Saving the slide failed: {e}"))
}

#[cfg(test)]
mod tests {
    use lectio_core::ink::InkColor;

    use super::*;

    fn white(w: u32, h: u32) -> Rgba {
        Rgba { width: w, height: h, pixels: vec![255; (w * h * 4) as usize] }
    }

    fn pixel(png: &[u8], x: u32, y: u32) -> [u8; 4] {
        image::load_from_memory(png).unwrap().to_rgba8().get_pixel(x, y).0
    }

    #[test]
    fn pen_ink_is_drawn_over_the_page_where_the_stroke_is() {
        let stroke = InkStroke { tool: InkTool::Pen, color: InkColor::Red, width: 0.02, points: vec![[0.1, 0.5], [0.9, 0.5]] };

        let png = page_png(&white(200, 100), &[stroke]).unwrap();

        let on = pixel(&png, 100, 50);
        assert!(on[0] > 180 && on[1] < 80, "red on the line: {on:?}");
        assert_eq!(pixel(&png, 100, 10), [255, 255, 255, 255], "white elsewhere");
    }

    #[test]
    fn a_highlighter_lets_the_page_show_through_and_pens_stay_on_top() {
        let mark = InkStroke { tool: InkTool::Highlighter, color: InkColor::Yellow, width: 0.02, points: vec![[0.1, 0.5], [0.9, 0.5]] };
        let pen = InkStroke { tool: InkTool::Pen, color: InkColor::Black, width: 0.04, points: vec![[0.5, 0.1], [0.5, 0.9]] };

        let png = page_png(&white(100, 100), &[pen.clone(), mark.clone()]).unwrap();

        let highlighted = pixel(&png, 20, 50);
        assert!(highlighted[2] < 230 && highlighted[0] > 200, "pale yellow, not opaque: {highlighted:?}");
        assert!(pixel(&png, 50, 50)[0] < 60, "the pen crosses on top");
        let order: Vec<InkTool> = in_drawing_order(&[pen, mark]).map(|s| s.tool).collect();
        assert_eq!(order, vec![InkTool::Highlighter, InkTool::Pen]);
    }
}
