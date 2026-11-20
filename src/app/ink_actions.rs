//! Phase 4b actions: drawing on slides (ADR-024). The slide panel sends a
//! finished stroke (pen-up) or an eraser touch; the rules and limits live in
//! `lectio_core::ink`.

use lectio_core::ink::{InkColor, InkStroke, InkTool};

use super::*;
use crate::action::InkPen;

/// Stored stroke widths, as a fraction of the page width.
const PEN_WIDTH: f32 = 0.003;
const HIGHLIGHTER_WIDTH: f32 = 0.004;
/// The eraser's reach, as a fraction of the page.
pub const ERASER_RADIUS: f32 = 0.015;

/// The drawing tools' state (not saved; drawing starts off).
#[derive(Debug, Clone, Copy, Default)]
pub struct InkState {
    pub on: bool,
    pub pen: InkPen,
    pub color: InkColor,
}

impl LectioApp {
    /// Handles ink actions; returns any other action unchanged.
    pub(super) fn apply_ink(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::SetInking(on) => self.ink.on = on,
            Action::SetInkTool(pen) => {
                self.ink.pen = pen;
                // Yellow suits the highlighter; switch back from it for the pen.
                match (pen, self.ink.color) {
                    (InkPen::Highlighter, InkColor::Black) => self.ink.color = InkColor::Yellow,
                    (InkPen::Pen, InkColor::Yellow) => self.ink.color = InkColor::Black,
                    _ => {}
                }
            }
            Action::SetInkColor(color) => self.ink.color = color,
            Action::AddInkStroke { deck, page, points } => {
                let (tool, width) = match self.ink.pen {
                    InkPen::Highlighter => (InkTool::Highlighter, HIGHLIGHTER_WIDTH),
                    _ => (InkTool::Pen, PEN_WIDTH),
                };
                let stroke = InkStroke { tool, color: self.ink.color, width, points };
                self.change_session(|lib, s| lib.with_ink_stroke(s, deck, page, stroke));
            }
            Action::EraseInk { deck, page, at } => {
                self.change_session(|lib, s| lib.with_ink_erased(s, deck, page, at, ERASER_RADIUS).map(|(lib, _)| lib));
            }
            Action::UndoInk { deck, page } => self.change_session(|lib, s| lib.without_last_stroke(s, deck, page)),
            Action::ClearInk { deck, page } => self.change_session(|lib, s| lib.without_page_ink(s, deck, page)),
            other => return Some(other),
        }
        None
    }
}
