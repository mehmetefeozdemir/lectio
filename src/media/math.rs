//! Draws formulas (ADR-023): `lectio_core::math` lays a formula out with the
//! glyph sizes measured here, and this module paints the result into an
//! RGBA image: glyphs from Noto Sans Math (Inter for `\text{…}` and any
//! character the math font lacks), bars and strokes with tiny-skia.
//!
//! The same image goes into the feed, the paper view and the exports.

use std::sync::OnceLock;

use ab_glyph::{Font, FontRef, GlyphId, PxScale, ScaleFont, point};
use lectio_core::math::{GlyphExtent, MathBox, MathFont, MathItem, Metrics, typeset};
use tiny_skia::{LineCap, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

use super::fonts;

/// Empty space around the drawing, in pixels.
const MARGIN: f32 = 2.0;
/// Formulas taller or wider than this are refused (a typo like 500 nested \sqrt).
const MAX_SIDE: f32 = 4_096.0;

struct Faces {
    math: FontRef<'static>,
    text: FontRef<'static>,
}

fn faces() -> &'static Faces {
    static FACES: OnceLock<Faces> = OnceLock::new();
    FACES.get_or_init(|| Faces {
        math: FontRef::try_from_slice(fonts::MATH).expect("the bundled math font is valid"),
        text: FontRef::try_from_slice(fonts::INTER_REGULAR).expect("the bundled text font is valid"),
    })
}

impl Faces {
    /// The font that draws `ch`: the requested one if it has the glyph,
    /// else the other one.
    fn pick(&self, ch: char, font: MathFont) -> (&FontRef<'static>, GlyphId) {
        let (first, second) = match font {
            MathFont::Math => (&self.math, &self.text),
            MathFont::Text => (&self.text, &self.math),
        };
        let id = first.glyph_id(ch);
        if id.0 != 0 {
            return (first, id);
        }
        (second, second.glyph_id(ch))
    }
}

/// `size` is pixels per em; ab_glyph scales by the font's full height.
fn scale(font: &FontRef<'static>, size: f32) -> PxScale {
    let units = font.units_per_em().unwrap_or(1_000.0);
    PxScale::from(size * font.height_unscaled() / units)
}

impl Metrics for Faces {
    fn glyph(&self, ch: char, font: MathFont, size: f32) -> GlyphExtent {
        let (face, id) = self.pick(ch, font);
        let px = scale(face, size);
        let advance = face.as_scaled(px).h_advance(id);
        let glyph = id.with_scale_and_position(px, point(0.0, 0.0));
        match face.outline_glyph(glyph) {
            Some(outlined) => {
                let bounds = outlined.px_bounds();
                GlyphExtent { advance, ascent: (-bounds.min.y).max(0.0), descent: bounds.max.y.max(0.0) }
            }
            // A space: width only.
            None => GlyphExtent { advance, ascent: 0.0, descent: 0.0 },
        }
    }
}

/// A drawn formula: premultiplied RGBA.
#[derive(Debug)]
pub struct FormulaImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Lays out `latex` at `size` pixels per em; errors are the parser's message.
pub fn layout(latex: &str, size: f32) -> Result<MathBox, String> {
    typeset(latex, size, faces()).map_err(|e| e.message)
}

/// Draws `latex` at `size` pixels per em in `color` on a transparent background.
pub fn render(latex: &str, size: f32, color: [u8; 3]) -> Result<FormulaImage, String> {
    let shape = layout(latex, size)?;
    let (w, h) = (shape.width + 2.0 * MARGIN, shape.height() + 2.0 * MARGIN);
    if w > MAX_SIDE || h > MAX_SIDE {
        return Err("This formula is too big to draw.".into());
    }
    let mut pixmap = Pixmap::new(w.ceil().max(1.0) as u32, h.ceil().max(1.0) as u32).ok_or("could not create the formula image")?;
    let mut paint = Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], 255);
    paint.anti_alias = true;
    for item in &shape.items {
        match *item {
            MathItem::Glyph { ch, font, size, x, baseline } => glyph(&mut pixmap, ch, font, size, x + MARGIN, baseline + MARGIN, color),
            MathItem::Rule { x, y, width, height } => {
                if let Some(rect) = Rect::from_xywh(x + MARGIN, y + MARGIN, width.max(0.5), height.max(0.5)) {
                    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
                }
            }
            MathItem::Line { x1, y1, x2, y2, width } => {
                let mut path = PathBuilder::new();
                path.move_to(x1 + MARGIN, y1 + MARGIN);
                path.line_to(x2 + MARGIN, y2 + MARGIN);
                if let Some(path) = path.finish() {
                    let stroke = Stroke { width, line_cap: LineCap::Round, ..Stroke::default() };
                    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
                }
            }
        }
    }
    Ok(FormulaImage { width: pixmap.width(), height: pixmap.height(), rgba: pixmap.take() })
}

/// Blends one glyph's coverage into the (premultiplied) pixmap.
fn glyph(pixmap: &mut Pixmap, ch: char, font: MathFont, size: f32, x: f32, baseline: f32, color: [u8; 3]) {
    let faces = faces();
    let (face, id) = faces.pick(ch, font);
    let Some(outlined) = face.outline_glyph(id.with_scale_and_position(scale(face, size), point(x, baseline))) else { return };
    let bounds = outlined.px_bounds();
    let (width, height) = (pixmap.width() as i64, pixmap.height() as i64);
    let data = pixmap.data_mut();
    outlined.draw(|gx, gy, coverage| {
        let (px, py) = (bounds.min.x as i64 + i64::from(gx), bounds.min.y as i64 + i64::from(gy));
        if px < 0 || py < 0 || px >= width || py >= height {
            return;
        }
        let i = ((py * width + px) * 4) as usize;
        let a = coverage.clamp(0.0, 1.0);
        for (c, value) in color.iter().enumerate() {
            data[i + c] = (f32::from(*value) * a + f32::from(data[i + c]) * (1.0 - a)).round() as u8;
        }
        data[i + 3] = (255.0 * a + f32::from(data[i + 3]) * (1.0 - a)).round() as u8;
    });
}

/// The formula as PNG (dark ink on transparent), for exports.
pub fn png(latex: &str, size: f32) -> Result<(Vec<u8>, u32, u32), String> {
    let image = render(latex, size, [0x1A, 0x1A, 0x1A])?;
    let pixmap = Pixmap::from_vec(image.rgba, tiny_skia::IntSize::from_wh(image.width, image.height).ok_or("empty formula")?)
        .ok_or("could not create the formula image")?;
    let bytes = pixmap.encode_png().map_err(|e| format!("Saving the formula failed: {e}"))?;
    Ok((bytes, image.width, image.height))
}

/// Characters in `latex` that neither bundled font can draw.
#[cfg(test)]
pub fn missing_glyphs(latex: &str) -> Result<Vec<char>, String> {
    let shape = layout(latex, 20.0)?;
    let faces = faces();
    let mut missing: Vec<char> = shape
        .items
        .iter()
        .filter_map(|item| match item {
            MathItem::Glyph { ch, font, .. } if faces.pick(*ch, *font).1 .0 == 0 => Some(*ch),
            _ => None,
        })
        .collect();
    missing.dedup();
    Ok(missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink(image: &FormulaImage) -> usize {
        image.rgba.chunks(4).filter(|p| p[3] > 128).count()
    }

    #[test]
    fn a_formula_becomes_an_image_with_ink() {
        let image = render(r"\frac{n(n+1)}{2}", 24.0, [0, 0, 0]).unwrap();
        assert!(image.width > 30 && image.height > 30);
        assert!(ink(&image) > 50);
    }

    #[test]
    fn a_wrong_formula_explains_itself() {
        assert!(render(r"\nope", 24.0, [0, 0, 0]).unwrap_err().contains(r"\nope"));
    }

    #[test]
    fn every_symbol_students_type_has_a_glyph_in_the_bundled_fonts() {
        let greek = r"\alpha\beta\gamma\delta\epsilon\varepsilon\zeta\eta\theta\vartheta\iota\kappa\lambda\mu\nu\xi\pi\varpi\rho\varrho\sigma\varsigma\tau\upsilon\phi\varphi\chi\psi\omega\Gamma\Delta\Theta\Lambda\Xi\Pi\Sigma\Upsilon\Phi\Psi\Omega";
        let symbols = r"\pm\mp\times\cdot\div\ast\circ\cup\cap\setminus\oplus\otimes\land\lor\le\ge\ne\approx\equiv\sim\simeq\cong\propto\in\notin\ni\subset\subseteq\supset\supseteq\to\leftarrow\leftrightarrow\Rightarrow\Leftarrow\Leftrightarrow\implies\iff\mapsto\mid\parallel\perp\ll\gg\infty\partial\nabla\forall\exists\nexists\emptyset\neg\angle\triangle\ldots\cdots\vdots\ddots\ell\hbar\Re\Im\aleph\top\bot\langle\rangle\lfloor\rfloor\lceil\rceil\int\iint\iiint\oint\sum\prod\coprod\bigcup\bigcap";
        let letters = r"abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789 \mathbf{aZ9} \mathbb{RNZQC} \text{Hi} \sin\lim + - = < > ( ) [ ] , ; ! ' |";
        for formula in [greek, symbols, letters] {
            assert_eq!(missing_glyphs(formula).unwrap(), Vec::<char>::new(), "{formula}");
        }
    }

    /// `cargo test --bin lectio write_formula_samples -- --ignored` writes
    /// sample pictures to `target/formulas/` to check the layout by eye.
    #[test]
    #[ignore]
    fn write_formula_samples() {
        let samples = [
            r"\frac{n(n+1)}{2}",
            r"\sum_{i=1}^{n} i^2 = \frac{n(n+1)(2n+1)}{6}",
            r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}",
            r"\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}",
            r"\lim_{n \to \infty} \left(1 + \frac{1}{n}\right)^n = e",
            r"\binom{n}{k} = \frac{n!}{k!(n-k)!}, \quad \hat{x}, \bar{y}, \vec{v}, \dot{a}, \tilde{z}",
            r"f: \mathbb{R} \to \mathbb{R}, \text{ for all } x \in A \cup B, \; \sqrt[3]{x_1^2 + x_2^2}",
        ];
        std::fs::create_dir_all("target/formulas").unwrap();
        for (i, latex) in samples.iter().enumerate() {
            let (bytes, _, _) = png(latex, 40.0).unwrap();
            std::fs::write(format!("target/formulas/{i}.png"), bytes).unwrap();
        }
    }

    #[test]
    fn exports_get_a_png() {
        let (bytes, w, h) = png("x^2", 32.0).unwrap();
        assert!(bytes.starts_with(b"\x89PNG") && w > 0 && h > 0);
    }
}
