//! Formulas (Phase 4b, ADR-023): a LaTeX subset is parsed into a tree
//! (`parse`) and laid out as boxes (`typeset`), the way TeX does it: every
//! piece has a width, a height above the baseline ("ascent") and a depth
//! below it ("descent"), and pieces are stacked or put side by side.
//!
//! The layout only needs glyph sizes, which the caller supplies through
//! `Metrics`; it returns glyphs, filled rectangles (fraction bars) and lines
//! (roots, accents) with positions. The app draws that list into an image
//! for the feed, the paper view and every export.

mod parse;
mod symbols;

use crate::model::new_note;
use crate::{LabelKind, Library, ModelError, NoteId, NoteKind, SessionId};

/// How a symbol spaces itself from its neighbours (TeX's atom classes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Ord,
    /// A big operator (∑, ∫); `limits` = scripts go above and below.
    Op { limits: bool },
    Bin,
    Rel,
    Open,
    Close,
    Punct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accent {
    Hat,
    Bar,
    Vec,
    Dot,
    Ddot,
    Tilde,
    Underline,
}

/// The parsed formula.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Row(Vec<Node>),
    Symbol { ch: char, class: Class },
    /// Upright words (`\text{…}`), in the text font.
    Text(String),
    /// `\sin`, `\lim`…: upright, spaced like an operator.
    Function { name: String, limits: bool },
    /// In ems; may be negative (`\!`).
    Space(f32),
    Scripts { base: Box<Node>, sub: Option<Box<Node>>, sup: Option<Box<Node>> },
    Frac { num: Box<Node>, den: Box<Node> },
    Binom { top: Box<Node>, bottom: Box<Node> },
    Sqrt { index: Option<Box<Node>>, body: Box<Node> },
    /// `\left( … \right)`: brackets as tall as the contents.
    Delimited { open: Option<char>, close: Option<char>, body: Box<Node> },
    Accent { kind: Accent, body: Box<Node> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MathError {
    pub message: String,
    /// Byte position in the source where the problem was found.
    pub at: usize,
}

impl std::fmt::Display for MathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Which font a glyph is drawn in: the math font, or the text font for words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MathFont {
    Math,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphExtent {
    pub advance: f32,
    pub ascent: f32,
    pub descent: f32,
}

/// Glyph sizes from a real font (the app) or a fake one (tests).
pub trait Metrics {
    fn glyph(&self, ch: char, font: MathFont, size: f32) -> GlyphExtent;
}

/// One thing to draw. Positions are in pixels from the top-left corner of
/// the formula; `y` grows downwards.
#[derive(Debug, Clone, PartialEq)]
pub enum MathItem {
    Glyph { ch: char, font: MathFont, size: f32, x: f32, baseline: f32 },
    /// A filled rectangle (a fraction bar).
    Rule { x: f32, y: f32, width: f32, height: f32 },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, width: f32 },
}

/// A laid-out formula: its size and what to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct MathBox {
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
    pub items: Vec<MathItem>,
}

impl MathBox {
    pub fn height(&self) -> f32 {
        self.ascent + self.descent
    }
}

pub fn parse(src: &str) -> Result<Node, MathError> {
    parse::Parser::new(src).formula()
}

/// Parses and lays out `src` at `size` pixels per em.
pub fn typeset(src: &str, size: f32, metrics: &dyn Metrics) -> Result<MathBox, MathError> {
    let tree = parse(src)?;
    let layout = Layout { metrics, root: size };
    let b = layout.node(&tree, size, false);
    let ascent = b.ascent;
    let items = b.items.into_iter().map(|item| moved(item, 0.0, ascent)).collect();
    Ok(MathBox { width: b.width.max(0.0), ascent, descent: b.descent, items })
}

/// Checks a formula for a note.
pub fn check(src: &str) -> Result<(), ModelError> {
    parse(src).map(|_| ()).map_err(|e| ModelError::InvalidFormula(e.message))
}

impl Library {
    /// Adds a formula note: `latex` must parse (ADR-023).
    pub fn with_math_note(
        &self,
        session: SessionId,
        offset_ms: u64,
        latex: &str,
        labels: &[LabelKind],
    ) -> Result<(Library, NoteId), ModelError> {
        let latex = latex.trim();
        check(latex)?;
        let mut note = new_note(offset_ms, latex, labels, None);
        note.kind = NoteKind::Math;
        self.insert_note(session, note)
    }
}

// ---------- layout ----------

/// Scripts are this much smaller than their base…
const SCRIPT_SCALE: f32 = 0.71;
/// …but never smaller than this share of the formula's size.
const MIN_SCALE: f32 = 0.5;
/// Fractions in running text are a little smaller.
const FRACTION_SCALE: f32 = 0.85;
/// Big operators (∑, ∫) at the top level.
const BIG_OP_SCALE: f32 = 1.4;
/// The math axis (the height of a minus sign), where fraction bars sit.
const AXIS: f32 = 0.25;

/// A box during layout: positions relative to its left end and baseline.
#[derive(Debug, Clone, Default)]
struct Bx {
    width: f32,
    ascent: f32,
    descent: f32,
    items: Vec<MathItem>,
}

fn moved(item: MathItem, dx: f32, dy: f32) -> MathItem {
    match item {
        MathItem::Glyph { ch, font, size, x, baseline } => MathItem::Glyph { ch, font, size, x: x + dx, baseline: baseline + dy },
        MathItem::Rule { x, y, width, height } => MathItem::Rule { x: x + dx, y: y + dy, width, height },
        MathItem::Line { x1, y1, x2, y2, width } => MathItem::Line { x1: x1 + dx, y1: y1 + dy, x2: x2 + dx, y2: y2 + dy, width },
    }
}

impl Bx {
    /// Places `other` with its baseline at `dy` below this one's, at `dx`.
    fn put(&mut self, other: Bx, dx: f32, dy: f32) {
        self.ascent = self.ascent.max(other.ascent - dy);
        self.descent = self.descent.max(other.descent + dy);
        self.width = self.width.max(dx + other.width);
        self.items.extend(other.items.into_iter().map(|i| moved(i, dx, dy)));
    }

    fn line(&mut self, from: [f32; 2], to: [f32; 2], width: f32) {
        self.items.push(MathItem::Line { x1: from[0], y1: from[1], x2: to[0], y2: to[1], width });
    }
}

struct Layout<'a> {
    metrics: &'a dyn Metrics,
    root: f32,
}

fn thickness(size: f32) -> f32 {
    (size * 0.05).max(0.8)
}

/// How a node spaces itself, for the row spacing rules.
fn class_of(node: &Node) -> Class {
    match node {
        Node::Symbol { class, .. } => *class,
        Node::Function { limits, .. } => Class::Op { limits: *limits },
        Node::Scripts { base, .. } => class_of(base),
        Node::Row(children) if children.len() == 1 => class_of(&children[0]),
        _ => Class::Ord,
    }
}

/// Space between two neighbours, in ems (simplified from TeX's table).
fn spacing(left: Class, right: Class) -> f32 {
    use Class::*;
    const THIN: f32 = 3.0 / 18.0;
    const MEDIUM: f32 = 4.0 / 18.0;
    const THICK: f32 = 5.0 / 18.0;
    match (left, right) {
        (Bin, _) | (_, Bin) => MEDIUM,
        (Rel, Rel) => 0.0,
        (Rel, _) | (_, Rel) => THICK,
        (Punct, _) => THIN,
        (Op { .. }, Ord | Open | Op { .. }) | (Ord | Close, Op { .. }) => THIN,
        _ => 0.0,
    }
}

/// TeX's rule: a "binary" sign with nothing to combine on its left (or
/// right) is just a sign, as in −x.
fn effective_classes(children: &[Node]) -> Vec<Class> {
    let mut classes: Vec<Class> = children.iter().map(class_of).collect();
    for i in 0..classes.len() {
        if classes[i] != Class::Bin {
            continue;
        }
        let before = i.checked_sub(1).map(|j| classes[j]);
        let after = classes.get(i + 1).copied();
        let lonely_left = matches!(before, None | Some(Class::Bin | Class::Op { .. } | Class::Rel | Class::Open | Class::Punct));
        let lonely_right = matches!(after, None | Some(Class::Rel | Class::Close | Class::Punct));
        if lonely_left || lonely_right {
            classes[i] = Class::Ord;
        }
    }
    classes
}

impl Layout<'_> {
    fn script_size(&self, size: f32) -> f32 {
        (size * SCRIPT_SCALE).max(self.root * MIN_SCALE)
    }

    fn glyph(&self, ch: char, font: MathFont, size: f32) -> Bx {
        let e = self.metrics.glyph(ch, font, size);
        Bx {
            width: e.advance,
            ascent: e.ascent,
            descent: e.descent,
            items: vec![MathItem::Glyph { ch, font, size, x: 0.0, baseline: 0.0 }],
        }
    }

    fn word(&self, text: &str, size: f32) -> Bx {
        let mut b = Bx::default();
        for ch in text.chars() {
            let g = self.glyph(ch, MathFont::Text, size);
            let x = b.width;
            if ch.is_whitespace() {
                b.width += g.width;
            } else {
                b.put(g, x, 0.0);
            }
        }
        b
    }

    fn node(&self, node: &Node, size: f32, script: bool) -> Bx {
        match node {
            Node::Row(children) => self.row(children, size, script),
            Node::Symbol { ch, class: Class::Op { .. } } if !script => {
                let mut b = self.glyph(*ch, MathFont::Math, size * BIG_OP_SCALE);
                // Centre big operators on the math axis.
                let dy = -AXIS * size - (b.descent - b.ascent) / 2.0;
                let mut out = Bx { width: b.width, ..Bx::default() };
                b.width = 0.0;
                out.put(b, 0.0, dy);
                out
            }
            Node::Symbol { ch, .. } => self.glyph(*ch, MathFont::Math, size),
            Node::Text(text) => self.word(text, size),
            Node::Function { name, .. } => self.word(name, size),
            Node::Space(em) => Bx { width: em * size, ..Bx::default() },
            Node::Scripts { base, sub, sup } => self.scripts(base, sub.as_deref(), sup.as_deref(), size, script),
            Node::Frac { num, den } => {
                let inner = if script { self.script_size(size) } else { (size * FRACTION_SCALE).max(self.root * MIN_SCALE) };
                self.stack(self.node(num, inner, script), self.node(den, inner, script), size, true)
            }
            Node::Binom { top, bottom } => {
                let inner = self.script_size(size);
                let stacked = self.stack(self.node(top, inner, true), self.node(bottom, inner, true), size, false);
                self.delimited(Some('('), Some(')'), stacked, size)
            }
            Node::Sqrt { index, body } => {
                let index = index.as_ref().map(|i| self.node(i, (size * 0.5).max(self.root * MIN_SCALE * 0.8), true));
                self.root_sign(index, self.node(body, size, script), size)
            }
            Node::Delimited { open, close, body } => self.delimited(*open, *close, self.node(body, size, script), size),
            Node::Accent { kind, body } => self.accent(*kind, self.node(body, size, script), size),
        }
    }

    fn row(&self, children: &[Node], size: f32, script: bool) -> Bx {
        let classes = effective_classes(children);
        let mut b = Bx::default();
        for (i, child) in children.iter().enumerate() {
            if i > 0 && !script {
                b.width += spacing(classes[i - 1], classes[i]) * size;
            }
            let x = b.width;
            let child_box = self.node(child, size, script);
            let width = child_box.width;
            b.put(child_box, x, 0.0);
            b.width = x + width;
        }
        b
    }

    fn scripts(&self, base: &Node, sub: Option<&Node>, sup: Option<&Node>, size: f32, script: bool) -> Bx {
        let base_box = self.node(base, size, script);
        let small = self.script_size(size);
        let sup_box = sup.map(|n| self.node(n, small, true));
        let sub_box = sub.map(|n| self.node(n, small, true));
        let limits = matches!(class_of(base), Class::Op { limits: true }) && !script;
        if limits {
            return self.limits(base_box, sub_box, sup_box, size);
        }
        let mut b = Bx::default();
        let base_width = base_box.width;
        let (base_ascent, base_descent) = (base_box.ascent, base_box.descent);
        b.put(base_box, 0.0, 0.0);
        let x = base_width + size * 0.04;
        let mut up = sup_box.as_ref().map_or(0.0, |s| (0.42 * size).max(base_ascent - 0.5 * s.ascent));
        let mut down = sub_box.as_ref().map_or(0.0, |s| (0.22 * size).max(base_descent + 0.1 * size).min(s.ascent + base_descent));
        if let (Some(p), Some(s)) = (&sup_box, &sub_box) {
            // Keep a gap between the two scripts.
            let gap = (down - s.ascent) - (-up + p.descent);
            let wanted = 0.12 * size;
            if gap < wanted {
                down += (wanted - gap) / 2.0;
                up += (wanted - gap) / 2.0;
            }
        }
        let mut right = x;
        if let Some(p) = sup_box {
            right = right.max(x + p.width);
            b.put(p, x, -up);
        }
        if let Some(s) = sub_box {
            right = right.max(x + s.width);
            b.put(s, x, down);
        }
        b.width = right;
        b
    }

    /// ∑ with its limits above and below, centred.
    fn limits(&self, base: Bx, sub: Option<Bx>, sup: Option<Bx>, size: f32) -> Bx {
        let gap = 0.15 * size;
        let width = [Some(base.width), sub.as_ref().map(|s| s.width), sup.as_ref().map(|s| s.width)]
            .into_iter()
            .flatten()
            .fold(0.0, f32::max);
        let mut b = Bx { width, ..Bx::default() };
        let (base_ascent, base_descent) = (base.ascent, base.descent);
        let centre = |w: f32| (width - w) / 2.0;
        let base_width = base.width;
        b.put(base, centre(base_width), 0.0);
        if let Some(p) = sup {
            let dy = -(base_ascent + gap + p.descent);
            b.put(p.clone(), centre(p.width), dy);
        }
        if let Some(s) = sub {
            let dy = base_descent + gap + s.ascent;
            b.put(s.clone(), centre(s.width), dy);
        }
        b.width = width;
        b
    }

    /// A fraction (with a bar) or a binomial's stack (without).
    fn stack(&self, num: Bx, den: Bx, size: f32, bar: bool) -> Bx {
        let t = thickness(size);
        let gap = 0.12 * size;
        let pad = 0.1 * size;
        let axis = AXIS * size;
        let width = num.width.max(den.width) + 2.0 * pad;
        let mut b = Bx { width, ..Bx::default() };
        let num_y = -(axis + t / 2.0 + gap + num.descent);
        let den_y = -axis + t / 2.0 + gap + den.ascent;
        let (nw, dw) = (num.width, den.width);
        b.put(num, (width - nw) / 2.0, num_y);
        b.put(den, (width - dw) / 2.0, den_y);
        if bar {
            b.items.push(MathItem::Rule { x: pad / 2.0, y: -axis - t / 2.0, width: width - pad, height: t });
        }
        b.width = width;
        b
    }

    /// √ drawn with lines: a short rise, a drop, a long rise, then the bar.
    fn root_sign(&self, index: Option<Bx>, body: Bx, size: f32) -> Bx {
        let t = thickness(size);
        let gap = 0.12 * size;
        let sign = 0.55 * size;
        let top = -(body.ascent + gap + t / 2.0);
        let bottom = body.descent + 0.05 * size;
        let knee = top + 0.6 * (bottom - top);
        let shift = index.as_ref().map_or(0.0, |i| (i.width - 0.45 * sign).max(0.0));
        let mut b = Bx::default();
        if let Some(i) = index {
            let dy = knee - 0.15 * size - i.descent;
            b.put(i, 0.0, dy);
        }
        let x0 = shift;
        b.line([x0, knee + 0.08 * size], [x0 + 0.25 * sign, knee], t);
        b.line([x0 + 0.25 * sign, knee], [x0 + 0.55 * sign, bottom], t * 1.4);
        b.line([x0 + 0.55 * sign, bottom], [x0 + sign, top], t);
        let body_x = x0 + sign + 0.08 * size;
        let end = body_x + body.width + 0.08 * size;
        b.line([x0 + sign, top], [end, top], t);
        b.put(body, body_x, 0.0);
        b.ascent = b.ascent.max(-top + t / 2.0);
        b.descent = b.descent.max(bottom + t);
        b.width = end;
        b
    }

    /// Brackets scaled to the contents' height, centred on them.
    fn delimited(&self, open: Option<char>, close: Option<char>, body: Bx, size: f32) -> Bx {
        let height = (body.ascent + body.descent).max(size * 0.8);
        let centre = (body.descent - body.ascent) / 2.0;
        let bracket = |ch: char| {
            let natural = self.metrics.glyph(ch, MathFont::Math, size);
            let scale = (height * 1.1 / (natural.ascent + natural.descent).max(0.01)).clamp(1.0, 4.0);
            let g = self.glyph(ch, MathFont::Math, size * scale);
            let dy = centre - (g.descent - g.ascent) / 2.0;
            (g, dy)
        };
        let mut b = Bx::default();
        let mut x = 0.0;
        if let Some(ch) = open {
            let (g, dy) = bracket(ch);
            let w = g.width;
            b.put(g, x, dy);
            x += w;
        }
        let w = body.width;
        b.put(body, x, 0.0);
        x += w;
        if let Some(ch) = close {
            let (g, dy) = bracket(ch);
            let w = g.width;
            b.put(g, x, dy);
            x += w;
        }
        b.width = x;
        b
    }

    fn accent(&self, kind: Accent, body: Bx, size: f32) -> Bx {
        let t = thickness(size);
        let gap = 0.08 * size;
        let h = 0.18 * size;
        let (w, top, bottom) = (body.width, -body.ascent, body.descent);
        let cx = w / 2.0;
        let span = w.min(0.6 * size).max(0.3 * size);
        let (left, right) = (cx - span / 2.0, cx + span / 2.0);
        let mid = top - gap - h / 2.0;
        let mut b = body;
        match kind {
            Accent::Bar => b.line([0.05 * size, top - gap], [w - 0.05 * size, top - gap], t),
            Accent::Underline => b.line([0.0, bottom + gap], [w, bottom + gap], t),
            Accent::Hat => {
                b.line([left, top - gap], [cx, top - gap - h], t);
                b.line([cx, top - gap - h], [right, top - gap], t);
            }
            Accent::Vec => {
                b.line([0.05 * size, mid], [w.max(0.4 * size), mid], t);
                let tip = w.max(0.4 * size);
                b.line([tip - h * 0.7, mid - h * 0.5], [tip, mid], t);
                b.line([tip - h * 0.7, mid + h * 0.5], [tip, mid], t);
            }
            Accent::Dot | Accent::Ddot => {
                let d = 0.11 * size;
                let centres: &[f32] = if kind == Accent::Dot { &[0.0] } else { &[-0.12, 0.12] };
                for c in centres {
                    b.items.push(MathItem::Rule { x: cx + c * size - d / 2.0, y: top - gap - d, width: d, height: d });
                }
            }
            Accent::Tilde => {
                let q = span / 3.0;
                b.line([left, mid + h / 4.0], [left + q, mid - h / 4.0], t);
                b.line([left + q, mid - h / 4.0], [left + 2.0 * q, mid + h / 4.0], t);
                b.line([left + 2.0 * q, mid + h / 4.0], [right, mid - h / 4.0], t);
            }
        }
        match kind {
            Accent::Underline => b.descent = b.descent.max(bottom + gap + t),
            _ => b.ascent = b.ascent.max(-(top - gap - h - t)),
        }
        b.width = b.width.max(w);
        b
    }
}
