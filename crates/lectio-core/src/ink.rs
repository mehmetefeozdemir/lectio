//! Slide ink (Phase 4b, ADR-024): pen and highlighter strokes drawn on a
//! slide page, stored as vectors in page fractions (0 = left/top, 1 =
//! right/bottom), so they fit the page at any size: the slides panel, the
//! paper view and exported PDFs.

use serde::{Deserialize, Serialize};

use crate::{DeckId, Library, ModelError, SessionId};

/// Most points in one stroke (a long, slow scribble is about 1 000).
pub const MAX_POINTS: usize = 2_000;
/// Most strokes on one page.
pub const MAX_STROKES: usize = 500;
/// Points are kept to 1/10 000 of the page: finer than a pixel on a 4K
/// screen, and short in `library.json`.
const PRECISION: f32 = 10_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InkTool {
    #[default]
    Pen,
    /// Wide and see-through, drawn under the pen strokes.
    Highlighter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InkColor {
    #[default]
    Black,
    Red,
    Blue,
    Green,
    Yellow,
}

impl InkColor {
    pub const ALL: [InkColor; 5] = [Self::Black, Self::Red, Self::Blue, Self::Green, Self::Yellow];

    /// sRGB, the same on screen and in exports.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Self::Black => [0x1A, 0x1A, 0x1A],
            Self::Red => [0xD0, 0x2B, 0x2B],
            Self::Blue => [0x1F, 0x5F, 0xD6],
            Self::Green => [0x1E, 0x8A, 0x4A],
            Self::Yellow => [0xF2, 0xC2, 0x00],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Black => "Black",
            Self::Red => "Red",
            Self::Blue => "Blue",
            Self::Green => "Green",
            Self::Yellow => "Yellow",
        }
    }
}

/// One pen-down → pen-up line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InkStroke {
    pub tool: InkTool,
    pub color: InkColor,
    /// Line width as a fraction of the page width.
    pub width: f32,
    pub points: Vec<[f32; 2]>,
}

/// The strokes on one page (pages without strokes aren't stored).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageInk {
    /// 1-based, like `SlideLink::page`.
    pub page: u32,
    pub strokes: Vec<InkStroke>,
}

impl crate::material::SlideDeck {
    /// The strokes on `page`, oldest first (empty if none).
    pub fn ink_on(&self, page: u32) -> &[InkStroke] {
        self.ink.iter().find(|p| p.page == page).map_or(&[], |p| p.strokes.as_slice())
    }
}

fn round(value: f32) -> f32 {
    (value * PRECISION).round() / PRECISION
}

fn checked(stroke: InkStroke) -> Result<InkStroke, ModelError> {
    if stroke.points.is_empty() {
        return Err(ModelError::InvalidInk("A stroke needs at least one point.".into()));
    }
    if stroke.points.len() > MAX_POINTS {
        return Err(ModelError::InvalidInk(format!("A stroke can have at most {MAX_POINTS} points.")));
    }
    let inside = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
    if !stroke.points.iter().flatten().all(|v| inside(*v)) || !(stroke.width.is_finite() && stroke.width > 0.0 && stroke.width <= 0.1) {
        return Err(ModelError::InvalidInk("A stroke must stay on the page.".into()));
    }
    let points = stroke.points.iter().map(|[x, y]| [round(*x), round(*y)]).collect();
    Ok(InkStroke { points, width: round(stroke.width).max(1.0 / PRECISION), ..stroke })
}

/// Squared distance from `p` to the segment `a`–`b`.
fn distance_squared(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0) };
    let (x, y) = (a[0] + t * dx - p[0], a[1] + t * dy - p[1]);
    x * x + y * y
}

/// Does the stroke pass within `radius` of `at` (all in page fractions)?
pub fn stroke_hits(stroke: &InkStroke, at: [f32; 2], radius: f32) -> bool {
    let reach = radius + stroke.width / 2.0;
    let near = |a: [f32; 2], b: [f32; 2]| distance_squared(at, a, b) <= reach * reach;
    match stroke.points.as_slice() {
        [] => false,
        [only] => near(*only, *only),
        points => points.windows(2).any(|w| near(w[0], w[1])),
    }
}

impl Library {
    /// Adds a stroke to a page (checked, rounded).
    pub fn with_ink_stroke(&self, session: SessionId, deck: DeckId, page: u32, stroke: InkStroke) -> Result<Library, ModelError> {
        let stroke = checked(stroke)?;
        self.map_page_ink(session, deck, page, |strokes| {
            if strokes.len() >= MAX_STROKES {
                return Err(ModelError::InvalidInk(format!("A page can have at most {MAX_STROKES} strokes. Erase some first.")));
            }
            strokes.push(stroke);
            Ok(())
        })
    }

    /// Removes every stroke within `radius` of `at`; returns how many.
    pub fn with_ink_erased(
        &self,
        session: SessionId,
        deck: DeckId,
        page: u32,
        at: [f32; 2],
        radius: f32,
    ) -> Result<(Library, usize), ModelError> {
        let mut removed = 0;
        let next = self.map_page_ink(session, deck, page, |strokes| {
            let before = strokes.len();
            strokes.retain(|s| !stroke_hits(s, at, radius));
            removed = before - strokes.len();
            Ok(())
        })?;
        Ok((next, removed))
    }

    /// Undo: removes the newest stroke on the page (no change if none).
    pub fn without_last_stroke(&self, session: SessionId, deck: DeckId, page: u32) -> Result<Library, ModelError> {
        self.map_page_ink(session, deck, page, |strokes| {
            strokes.pop();
            Ok(())
        })
    }

    /// Clears the page.
    pub fn without_page_ink(&self, session: SessionId, deck: DeckId, page: u32) -> Result<Library, ModelError> {
        self.map_page_ink(session, deck, page, |strokes| {
            strokes.clear();
            Ok(())
        })
    }

    /// Changes one page's strokes; pages left empty are dropped.
    fn map_page_ink(
        &self,
        session: SessionId,
        deck: DeckId,
        page: u32,
        change: impl FnOnce(&mut Vec<InkStroke>) -> Result<(), ModelError>,
    ) -> Result<Library, ModelError> {
        self.map_session(session, |s| {
            let index = s.decks.iter().position(|d| d.id == deck).ok_or(ModelError::DeckNotFound)?;
            let page_count = s.decks[index].page_count;
            if page == 0 || page > page_count {
                return Err(ModelError::PageOutOfRange { page, page_count });
            }
            let mut changed = s.clone();
            let target = &mut changed.decks[index];
            let mut strokes = target.ink_on(page).to_vec();
            change(&mut strokes)?;
            target.ink.retain(|p| p.page != page);
            if !strokes.is_empty() {
                target.ink.push(PageInk { page, strokes });
                target.ink.sort_by_key(|p| p.page);
            }
            Ok(changed)
        })
    }
}
