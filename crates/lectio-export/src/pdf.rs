//! The paper view as an A4 PDF with krilla (pure Rust; fonts are embedded
//! and subset, so the text stays selectable and searchable).
//!
//! krilla draws one line of text at a time, so this module does the page
//! layout itself, in two steps: first lay out every block into positioned
//! text runs and pictures, breaking lines with the font's glyph widths and
//! starting a new page when the next piece doesn't fit; then draw the pages.

use krilla::Document;
use krilla::color::luma;
use krilla::geom::{Point, Size, Transform};
use krilla::image::Image;
use krilla::metadata::Metadata;
use krilla::page::PageSettings;
use krilla::paint::Fill;
use krilla::text::{Font, TextDirection};
use lectio_core::paper::{Block, PaperOptions, note_text, photo_word};
use lectio_core::timeline::format_offset;

use crate::{Picture, Pictures, picture_of};

/// The text fonts (TrueType bytes): regular and bold (Inter in the app).
pub struct Fonts {
    pub regular: Vec<u8>,
    pub bold: Vec<u8>,
}

// A4 in points (1/72 inch), with 2 cm margins.
const PAGE_W: f32 = 595.28;
const PAGE_H: f32 = 841.89;
const MARGIN: f32 = 56.7;
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN;
const BOTTOM: f32 = PAGE_H - MARGIN;
const BODY: f32 = 10.5;
const LINE: f32 = 1.45;
/// CSS pixels are 0.75 pt.
const PT_PER_PX: f32 = 0.75;
const MAX_PICTURE_H: f32 = 420.0;
const GRAY: u8 = 92;

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Regular,
    Bold,
    Gray,
}

enum Op {
    Text { x: f32, y: f32, size: f32, style: Style, text: String },
    Picture { x: f32, y: f32, w: f32, h: f32, index: usize },
}

struct Layout<'a> {
    regular: ttf_parser::Face<'a>,
    bold: ttf_parser::Face<'a>,
    pages: Vec<Vec<Op>>,
    y: f32,
    pictures: Vec<&'a Picture>,
}

impl<'a> Layout<'a> {
    fn width(&self, text: &str, size: f32, style: Style) -> f32 {
        let face = if style == Style::Bold { &self.bold } else { &self.regular };
        let units: u32 = text
            .chars()
            .map(|c| face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).map_or(500, u32::from))
            .sum();
        units as f32 * size / f32::from(face.units_per_em())
    }

    fn page(&mut self) -> &mut Vec<Op> {
        if self.pages.is_empty() {
            self.pages.push(Vec::new());
        }
        self.pages.last_mut().expect("a page was just added")
    }

    /// Starts a new page if `height` more doesn't fit on this one.
    fn room(&mut self, height: f32) {
        if self.y + height > BOTTOM && self.y > MARGIN {
            self.pages.push(Vec::new());
            self.y = MARGIN;
        }
    }

    /// Wraps runs of text (each with a style) to the content width.
    fn paragraph(&mut self, runs: &[(String, Style)], size: f32, after: f32) {
        let space = self.width(" ", size, Style::Regular);
        let mut lines: Vec<Vec<(f32, Style, String)>> = vec![Vec::new()];
        let mut x = 0.0;
        for (text, style) in runs {
            for word in text.split_whitespace() {
                let w = self.width(word, size, *style);
                if x > 0.0 && x + w > CONTENT_W {
                    lines.push(Vec::new());
                    x = 0.0;
                }
                if let Some(line) = lines.last_mut() {
                    line.push((x, *style, word.to_owned()));
                }
                x += w + space;
            }
        }
        let height = size * LINE;
        for line in lines.into_iter().filter(|l| !l.is_empty()) {
            self.room(height);
            let baseline = self.y + size;
            let ops: Vec<Op> = line
                .into_iter()
                .map(|(x, style, text)| Op::Text { x: MARGIN + x, y: baseline, size, style, text })
                .collect();
            self.page().extend(ops);
            self.y += height;
        }
        self.y += after;
    }

    /// A picture at its size (scaled to fit), optionally after a time.
    fn picture(&mut self, picture: &'a Picture, before: Option<String>) {
        let mut w = picture.width as f32 * PT_PER_PX;
        let mut h = picture.height as f32 * PT_PER_PX;
        let lead = before.as_ref().map_or(0.0, |t| self.width(t, BODY, Style::Gray) + 6.0);
        let scale = ((CONTENT_W - lead) / w).min(MAX_PICTURE_H / h).min(1.0);
        w *= scale;
        h *= scale;
        self.room(h);
        let y = self.y;
        if let Some(text) = before {
            self.page().push(Op::Text { x: MARGIN, y: y + h / 2.0 + BODY * 0.35, size: BODY, style: Style::Gray, text });
        }
        self.pictures.push(picture);
        let index = self.pictures.len() - 1;
        self.page().push(Op::Picture { x: MARGIN + lead, y, w, h, index });
        self.y += h + 6.0;
    }

    fn block(&mut self, block: &Block, options: &PaperOptions, pictures: &'a Pictures) {
        let time = |ms: u64| if options.times { format!("{}  ", format_offset(ms)) } else { String::new() };
        let tail = |labels: &[String]| if labels.is_empty() { String::new() } else { format!("({})", labels.join(", ")) };
        let picture = picture_of(block, pictures).map(|(p, _)| p);
        match block {
            Block::Title { title, subtitle } => {
                self.paragraph(&[(title.clone(), Style::Bold)], 22.0, 2.0);
                self.paragraph(&[(subtitle.clone(), Style::Gray)], BODY, 14.0);
            }
            Block::Heading { text, .. } => {
                self.y += 8.0;
                self.room(15.0 * LINE + BODY * LINE * 2.0); // keep a heading with what follows
                self.paragraph(&[(text.clone(), Style::Bold)], 15.0, 4.0);
            }
            Block::Note { offset_ms, text, labels } => {
                let runs = [(time(*offset_ms), Style::Gray), (note_text(text).to_owned(), Style::Regular), (tail(labels), Style::Gray)];
                self.paragraph(&runs, BODY, 3.0);
            }
            Block::Formula { offset_ms, latex, labels } => match picture {
                Some(p) => self.picture(p, Some(time(*offset_ms)).filter(|t| !t.is_empty())),
                None => self.paragraph(&[(time(*offset_ms), Style::Gray), (latex.clone(), Style::Regular), (tail(labels), Style::Gray)], BODY, 3.0),
            },
            Block::Slide { page, deck_title, caption, offset_ms, .. } => {
                self.y += 4.0;
                if let Some(p) = picture {
                    self.picture(p, None);
                }
                let when = match offset_ms {
                    Some(ms) if options.times => format!(" · {}", format_offset(*ms)),
                    _ => String::new(),
                };
                self.paragraph(&[(format!("{caption} · {deck_title}, page {page}{when}"), Style::Gray)], BODY * 0.9, 8.0);
            }
            Block::Image { kind, offset_ms, caption, .. } => {
                self.paragraph(&[(time(*offset_ms), Style::Gray), (format!("{} {caption}", photo_word(*kind)), Style::Regular)], BODY, 2.0);
                if let Some(p) = picture {
                    self.picture(p, None);
                }
            }
            Block::Transcript { lines } => {
                self.y += 8.0;
                self.paragraph(&[("Transcript".to_owned(), Style::Bold)], 15.0, 4.0);
                for (ms, text) in lines {
                    self.paragraph(&[(time(*ms), Style::Gray), (text.clone(), Style::Regular)], BODY, 3.0);
                }
            }
        }
    }
}

fn face(bytes: &[u8]) -> Result<ttf_parser::Face<'_>, String> {
    ttf_parser::Face::parse(bytes, 0).map_err(|_| "The PDF font could not be read.".to_owned())
}

fn font(bytes: &[u8]) -> Result<Font, String> {
    Font::new(bytes.to_vec().into(), 0).ok_or_else(|| "The PDF font could not be read.".to_owned())
}

pub fn pdf(blocks: &[Block], options: &PaperOptions, pictures: &Pictures, fonts: &Fonts) -> Result<Vec<u8>, String> {
    let regular_face = face(&fonts.regular)?;
    // A missing bold font falls back to the regular one.
    let (bold_bytes, bold_face) = match face(&fonts.bold) {
        Ok(f) => (&fonts.bold, f),
        Err(_) => (&fonts.regular, face(&fonts.regular)?),
    };
    let mut layout = Layout { regular: regular_face, bold: bold_face, pages: Vec::new(), y: MARGIN, pictures: Vec::new() };
    for block in blocks {
        layout.block(block, options, pictures);
    }
    if layout.pages.is_empty() {
        layout.pages.push(Vec::new());
    }
    let (regular, bold) = (font(&fonts.regular)?, font(bold_bytes)?);
    let title = blocks.iter().find_map(|b| if let Block::Title { title, .. } = b { Some(title.clone()) } else { None });
    let mut document = Document::new();
    document.set_metadata(Metadata::new().title(title.clone().unwrap_or_else(|| "Lectio notes".into())).language("en".into()));
    let page_count = layout.pages.len();
    for (number, ops) in layout.pages.iter().enumerate() {
        let settings = PageSettings::from_wh(PAGE_W, PAGE_H).ok_or("bad page size")?;
        let mut page = document.start_page_with(settings);
        let mut surface = page.surface();
        for op in ops {
            match op {
                Op::Text { x, y, size, style, text } => {
                    let paint = if *style == Style::Gray { luma::Color::new(GRAY) } else { luma::Color::black() };
                    surface.set_fill(Some(Fill { paint: paint.into(), ..Fill::default() }));
                    let font = if *style == Style::Bold { bold.clone() } else { regular.clone() };
                    surface.draw_text(Point::from_xy(*x, *y), font, *size, text, false, TextDirection::Auto);
                }
                Op::Picture { x, y, w, h, index } => {
                    let png = layout.pictures[*index].png.clone();
                    let image = Image::from_png(png.into(), true).map_err(|e| format!("A picture could not be added to the PDF ({e})."))?;
                    let size = Size::from_wh(*w, *h).ok_or("bad picture size")?;
                    surface.push_transform(&Transform::from_translate(*x, *y));
                    surface.draw_image(image, size);
                    surface.pop();
                }
            }
        }
        // Footer: "Title · 2 / 5".
        let footer = format!("{} · {} / {page_count}", title.as_deref().unwrap_or("Lectio"), number + 1);
        surface.set_fill(Some(Fill { paint: luma::Color::new(GRAY).into(), ..Fill::default() }));
        surface.draw_text(Point::from_xy(MARGIN, PAGE_H - MARGIN / 2.0), regular.clone(), 8.0, &footer, false, TextDirection::Auto);
        surface.finish();
        page.finish();
    }
    document.finish().map_err(|e| format!("The PDF could not be written ({e:?})."))
}
