//! Markdown export of a session for the reading view's "Copy all" and
//! "Export" (ADR-015): headings, slide items, notes with time and labels.

use crate::material::AttachmentKind;
use crate::timeline::format_offset;
use crate::{Library, ModelError, Note, NoteKind, Session, SessionId};

pub fn session_markdown(library: &Library, session: SessionId) -> Result<String, ModelError> {
    let s = library.session(session).ok_or(ModelError::SessionNotFound)?;
    let course = library.course_of(session).map(|c| c.name.as_str()).unwrap_or("");
    let length = s.recording.as_ref().map(|r| format!(" · {}", format_offset(r.duration_ms))).unwrap_or_default();
    let mut out = format!("# {}\n\n*{course}{length}*\n\n", s.title);
    for note in &s.notes {
        out.push_str(&note_markdown(library, s, note));
    }
    Ok(out)
}

fn note_markdown(library: &Library, session: &Session, note: &Note) -> String {
    let time = format_offset(note.offset_ms);
    if note.is_heading() {
        return format!("\n## {}\n\n", note.body);
    }
    if note.kind == NoteKind::Slide {
        let page = note.slide.map(|l| l.page).unwrap_or(0);
        let deck = note.slide.and_then(|l| session.decks.iter().find(|d| d.id == l.deck)).map(|d| d.title.as_str());
        return format!("\n**{}** ({}, page {page}) · {time}\n\n", note.body, deck.unwrap_or("slides"));
    }
    let what = match note.attachment.as_ref().map(|a| a.kind) {
        Some(AttachmentKind::Photo) => "[photo] ",
        Some(AttachmentKind::Sketch) => "[sketch] ",
        None => "",
    };
    let body = if note.body.is_empty() && what.is_empty() { "(marked moment)" } else { note.body.as_str() };
    let names: Vec<&str> = note.labels.iter().map(|&k| library.labels().name(k)).collect();
    let labels = if names.is_empty() { String::new() } else { format!(" *({})*", names.join(", ")) };
    format!("- {time} {what}{body}{labels}\n")
}
