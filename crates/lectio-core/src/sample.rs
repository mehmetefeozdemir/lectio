//! The sample lecture: a populated library with a recording, labelled notes
//! linked to slides, slides posted to the feed, ink, formulas, a transcript
//! and a collection. The desktop shows it for `--screenshot` and the web
//! `#sample` page; the mobile apps load it for the store screenshots. One copy,
//! so every platform's pictures show the same lecture.

use crate::ink::{InkColor, InkStroke, InkTool};
use crate::material::{BlobKey, MediaKind, SlideLink};
use crate::transcript::{Segment, Transcript};
use crate::{DeckId, LabelKind, Library, ModelError, Recording, SessionId};

const MINUTE_MS: u64 = 60_000;
const DAY_MS: u64 = 86_400_000;
/// 2026-11-17 (UTC), the sample's main lecture day, as days since 1970-01-01.
const SAMPLE_DAY: i64 = 20_774;
/// How long the sample lecture's recording is.
pub const SAMPLE_RECORDING_MS: u64 = 50 * MINUTE_MS;

/// The two-slide deck the sample lecture uses (also used by tests).
pub const SAMPLE_SLIDES: &[u8] = include_bytes!("../../../assets/sample-slides.pdf");

/// 10:00 UTC on the sample day plus `offset` days.
fn lecture_day(offset: i64) -> u64 {
    (SAMPLE_DAY + offset) as u64 * DAY_MS + 10 * 60 * MINUTE_MS
}

pub struct Sample {
    pub library: Library,
    /// "Graph traversal", the lecture with everything in it.
    pub session: SessionId,
    pub deck: DeckId,
    /// Where the deck's PDF ([`SAMPLE_SLIDES`]) must be stored.
    pub slides: BlobKey,
    /// The recording's file name (`recordings/<name>`); the caller decides
    /// whether to write audio for it.
    pub recording_file: String,
}

/// Two courses, five lectures (one in the trash), a collection, and the
/// "Graph traversal" lecture.
pub fn sample_library() -> Result<Sample, ModelError> {
    let (lib, algorithms) = Library::default().with_course("CSCA67")?;
    let (lib, _) = lib.with_session(algorithms, "Lecture 1", lecture_day(-14))?;
    let (lib, _) = lib.with_session(algorithms, "Lecture 2", lecture_day(-7))?;
    let (lib, session) = lib.with_session(algorithms, "Graph traversal", lecture_day(0))?;
    let (lib, calculus) = lib.with_course("MATA31")?;
    let (lib, _) = lib.with_session(calculus, "Limits", lecture_day(1))?;
    let (lib, old_session) = lib.with_session(calculus, "Office hours (test run)", lecture_day(-3))?;
    let lib = lib.with_session_trashed(old_session, lecture_day(2))?;

    let recording_file = format!("{session}.wav");
    let lib = lib.with_recording(
        session,
        Recording { file_name: recording_file.clone(), duration_ms: SAMPLE_RECORDING_MS, sample_rate: 16_000, cleaned_file: None },
    )?;
    let slides = BlobKey::new_random(MediaKind::Pdf);
    let (lib, deck) = lib.with_deck(session, "Week 3 - Graphs.pdf", slides.clone(), 2)?;

    // (offset, text, labels, slide page)
    let notes: [(u64, &str, &[LabelKind], Option<u32>); 9] = [
        (2 * MINUTE_MS + 10_000, "Graphs: a set of vertices and a set of edges", &[LabelKind::Definition], Some(1)),
        (6 * MINUTE_MS, "Adjacency list vs. matrix: space trade-off", &[], Some(1)),
        (11 * MINUTE_MS + 40_000, "", &[LabelKind::Important], None),
        (16 * MINUTE_MS + 50_000, "Queue: first in, first out", &[LabelKind::Definition], Some(2)),
        (17 * MINUTE_MS + 5_000, "BFS visits vertices in order of distance from the start", &[LabelKind::Review], Some(2)),
        (24 * MINUTE_MS + 30_000, "Proof sketch on the board; copy it from the slides later", &[LabelKind::Task], None),
        (33 * MINUTE_MS, "DFS uses a stack (or recursion)", &[], None),
        (38 * MINUTE_MS + 15_000, "Cycle: a path that starts and ends at the same vertex", &[LabelKind::Definition], None),
        (41 * MINUTE_MS + 20_000, "Exam hint: cycle detection", &[LabelKind::Important, LabelKind::Review], None),
    ];
    let lib = notes.iter().try_fold(lib, |lib, (offset, body, labels, page)| {
        let (lib, note) = lib.with_note(session, *offset, body, labels)?;
        let link = page.map(|page| SlideLink { deck, page });
        lib.with_slide_link(session, note, link)
    })?;

    // Section headings and slides posted to the feed (ADR-015).
    let (lib, _) = lib.with_note(session, 30_000, "Graph basics", &[LabelKind::Heading])?;
    let (lib, _) = lib.with_slide_item(session, MINUTE_MS, SlideLink { deck, page: 1 })?;
    let (lib, _) = lib.with_note(session, 14 * MINUTE_MS + 30_000, "Breadth-first search", &[LabelKind::Heading])?;
    let (lib, bfs_slide) = lib.with_slide_item(session, 15 * MINUTE_MS, SlideLink { deck, page: 2 })?;
    let lib = lib.with_note_text(session, bfs_slide, "BFS step by step")?;
    let lib = lib.with_transcript(session, sample_transcript())?;

    // Phase 4b: formulas, a drawing on slide 2, and a collection across courses.
    let (lib, _) = lib.with_math_note(session, 20 * MINUTE_MS, r"\text{BFS time} = O(|V| + |E|)", &[LabelKind::Important])?;
    let (lib, _) = lib.with_math_note(session, 36 * MINUTE_MS, r"\sum_{v \in V} \deg(v) = 2|E|", &[])?;
    let lib = sample_ink().into_iter().try_fold(lib, |lib, stroke| lib.with_ink_stroke(session, deck, 2, stroke))?;
    let limits = lib.courses().iter().flat_map(|c| &c.sessions).find(|s| s.title == "Limits").map(|s| s.id);
    let (lib, midterm) = lib.with_collection("Midterm 1")?;
    let lib = lib.with_session_in_collection(midterm, session)?;
    let lib = match limits {
        Some(limits) => lib.with_session_in_collection(midterm, limits)?,
        None => lib,
    };
    Ok(Sample { library: lib, session, deck, slides, recording_file })
}

/// Transcript segments around the playhead (17:40), so the Transcript tab has
/// something to show.
fn sample_transcript() -> Transcript {
    let lines: [(u64, u64, &str); 8] = [
        (16 * MINUTE_MS + 30_000, 16 * MINUTE_MS + 36_000, "So let's look at breadth-first search."),
        (16 * MINUTE_MS + 36_000, 16 * MINUTE_MS + 44_000, "We start at one vertex and visit all of its neighbours first."),
        (16 * MINUTE_MS + 44_000, 16 * MINUTE_MS + 53_000, "To remember who comes next, we keep a queue."),
        (16 * MINUTE_MS + 53_000, 17 * MINUTE_MS + 4_000, "Vertices leave the queue in the order they were discovered."),
        (17 * MINUTE_MS + 4_000, 17 * MINUTE_MS + 15_000, "That means we visit them in order of distance from the start."),
        (17 * MINUTE_MS + 15_000, 17 * MINUTE_MS + 29_000, "This is why BFS finds shortest paths when every edge counts the same."),
        (17 * MINUTE_MS + 29_000, 17 * MINUTE_MS + 47_000, "Let me draw the tree it builds on the board."),
        (17 * MINUTE_MS + 47_000, 17 * MINUTE_MS + 58_000, "Each level of the tree is one more step away from the root."),
    ];
    let segments = lines.iter().map(|&(start_ms, end_ms, text)| Segment { start_ms, end_ms, text: text.to_owned() }).collect();
    Transcript { language: "en".into(), model: "base".into(), segments, complete: true }
}

/// A highlight under slide 2's title and a red loop around its diagram.
fn sample_ink() -> Vec<InkStroke> {
    let circle: Vec<[f32; 2]> = (0..=32)
        .map(|i| {
            let a = i as f32 / 32.0 * std::f32::consts::TAU;
            [0.62 + 0.17 * a.cos(), 0.69 + 0.24 * a.sin()]
        })
        .collect();
    vec![
        InkStroke { tool: InkTool::Highlighter, color: InkColor::Yellow, width: 0.004, points: vec![[0.12, 0.33], [0.62, 0.33]] },
        InkStroke { tool: InkTool::Pen, color: InkColor::Red, width: 0.003, points: circle },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_lecture_has_everything_the_screenshots_show() {
        let sample = sample_library().unwrap();
        let session = sample.library.session(sample.session).unwrap();
        assert_eq!(session.title, "Graph traversal");
        assert_eq!(session.recording.as_ref().unwrap().duration_ms, SAMPLE_RECORDING_MS);
        assert_eq!(sample.recording_file, format!("{}.wav", sample.session));
        assert!(session.notes.len() >= 12, "notes, headings, slides and formulas");
        let definitions = session.notes.iter().filter(|n| n.labels.contains(&LabelKind::Definition)).count();
        assert_eq!(definitions, 3, "enough for a short quiz");
        assert_eq!(session.transcript.as_ref().unwrap().segments.len(), 8);
        assert_eq!(sample.library.courses().len(), 2);
        assert_eq!(sample.library.collections().len(), 1);
    }

    #[test]
    fn the_sample_slides_are_a_two_page_pdf() {
        assert!(SAMPLE_SLIDES.starts_with(b"%PDF"));
    }
}
