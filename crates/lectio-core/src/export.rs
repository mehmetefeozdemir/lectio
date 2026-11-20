//! Markdown export of a session for "Copy all" and "Export Markdown"
//! (ADR-015). Since Phase 4b it is written from the paper view's document
//! model (`paper`), so every export shows the same blocks.

use crate::paper::{PaperOptions, document, markdown};
use crate::{Library, ModelError, SessionId};

pub fn session_markdown(library: &Library, session: SessionId) -> Result<String, ModelError> {
    let options = PaperOptions::default();
    Ok(markdown(&document(library, session, &options)?, &options))
}
