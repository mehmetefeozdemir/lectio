//! The audio stream: a recording drawn as short sections next to the feed
//! (ADR-015). Each section can be tinted with the label of a note in its time
//! range, and clicking one plays from its start.

use crate::{LabelKind, Note};

/// Length of one audio section.
pub const SECTION_MS: u64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section {
    pub start_ms: u64,
    pub end_ms: u64,
}

/// Splits a recording into `SECTION_MS` sections (the last may be shorter).
pub fn audio_sections(duration_ms: u64) -> Vec<Section> {
    (0..duration_ms.div_ceil(SECTION_MS))
        .map(|i| Section { start_ms: i * SECTION_MS, end_ms: ((i + 1) * SECTION_MS).min(duration_ms) })
        .collect()
}

/// Index of the section that contains `offset_ms`; the very end of the
/// recording belongs to the last section.
pub fn section_at(sections: &[Section], offset_ms: u64) -> Option<usize> {
    let last = sections.len().checked_sub(1)?;
    Some(sections.iter().position(|s| offset_ms < s.end_ms).unwrap_or(last))
}

/// The label each section is tinted with: the first label (in timeline
/// order) of a note inside the section. Headings structure the notes and
/// don't tint the audio.
pub fn section_labels(sections: &[Section], notes: &[Note]) -> Vec<Option<LabelKind>> {
    sections
        .iter()
        .map(|s| {
            notes
                .iter()
                .filter(|n| n.offset_ms >= s.start_ms && n.offset_ms < s.end_ms)
                .flat_map(|n| n.labels.iter().copied())
                .find(|&k| k != LabelKind::Heading)
        })
        .collect()
}
