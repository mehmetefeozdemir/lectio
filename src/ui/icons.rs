//! Phosphor icon codepoints (regular weight, MIT). The font file is
//! `assets/fonts/Phosphor.ttf`; only the icons Lectio uses are listed.

use lectio_core::LabelKind;

pub const RECORD: &str = "\u{E3EE}";
pub const PAUSE: &str = "\u{E39E}";
pub const STOP: &str = "\u{E46C}";
pub const PLAY: &str = "\u{E3D0}";
pub const BACK: &str = "\u{E1A0}"; // clock-counter-clockwise: back 5 s
pub const FORWARD: &str = "\u{E19E}"; // clock-clockwise: forward 5 s
pub const STAR: &str = "\u{E46A}";
pub const REVIEW: &str = "\u{E038}"; // arrow-counter-clockwise
pub const TASK: &str = "\u{E186}"; // check-square
pub const DEFINITION: &str = "\u{E0E6}"; // book-open
pub const IMAGE: &str = "\u{E2CA}";
pub const CAMERA: &str = "\u{E10E}";
pub const SKETCH: &str = "\u{E3B4}"; // pencil-simple
pub const SEND: &str = "\u{E396}"; // paper-plane-right
pub const MORE: &str = "\u{E1FE}"; // dots-three
pub const TRASH: &str = "\u{E4A6}";
pub const PLUS: &str = "\u{E3D4}";
pub const PREVIOUS: &str = "\u{E138}"; // caret-left
pub const NEXT: &str = "\u{E13A}"; // caret-right
pub const SIDEBAR: &str = "\u{EC24}";
pub const MOON: &str = "\u{E330}";
pub const SUN: &str = "\u{E472}";
pub const SYSTEM: &str = "\u{E32E}"; // monitor
pub const READING: &str = "\u{E8F2}"; // book-open-text
pub const SEARCH: &str = "\u{E30C}";
pub const SLIDES: &str = "\u{E654}"; // presentation
pub const LINK: &str = "\u{E2E2}";
pub const UNLINK: &str = "\u{E2E4}";
pub const PDF: &str = "\u{E702}";
pub const TAG: &str = "\u{E478}";
pub const CLOSE: &str = "\u{E4F6}";
pub const UNDO: &str = "\u{E08A}";
pub const SETTINGS: &str = "\u{E272}";
pub const EDIT: &str = "\u{E34C}"; // note-pencil
pub const MICROPHONE: &str = "\u{E326}";
pub const HEADING: &str = "\u{E6BA}"; // text-h
pub const HOME: &str = "\u{E2C2}"; // house
pub const COLLECTIONS: &str = "\u{E260}"; // folders
pub const TASKS: &str = "\u{E184}"; // check-circle
pub const BACK_ARROW: &str = "\u{E058}"; // arrow-left
pub const LAYOUT: &str = "\u{E6D6}";
pub const ZOOM_IN: &str = "\u{E310}";
pub const ZOOM_OUT: &str = "\u{E30E}";
pub const FULLSCREEN: &str = "\u{E0A2}"; // arrows-out
pub const COPY: &str = "\u{E1CA}";
pub const EXPORT: &str = "\u{EAF0}";
pub const WAVEFORM: &str = "\u{E802}";
pub const CAPTIONS: &str = "\u{E1A4}"; // closed-captioning
pub const SPEED: &str = "\u{E628}"; // gauge
pub const SOUND: &str = "\u{E434}"; // sliders-horizontal
pub const SCREENSHOT: &str = "\u{E69A}"; // selection
pub const DOWNLOAD: &str = "\u{E20C}"; // download-simple
pub const TRANSCRIPT: &str = "\u{E1A8}"; // subtitles
pub const TO_FEED: &str = "\u{E17A}"; // chat-text
pub const COMPUTER: &str = "\u{E560}"; // desktop
pub const STOP_CIRCLE: &str = "\u{E4F8}"; // x-circle
// Phase 4b: formulas, slide ink, paper view, collections.
pub const FORMULA: &str = "\u{EAB8}"; // sigma
pub const PEN: &str = "\u{E3AA}";
pub const HIGHLIGHTER: &str = "\u{EC76}";
pub const ERASER: &str = "\u{E21E}";
pub const UNDO_STROKE: &str = "\u{E08A}"; // arrow-u-up-left
pub const CLEAR: &str = "\u{EC54}"; // broom
pub const PRINT: &str = "\u{E3DC}";
pub const FILE_PDF: &str = "\u{E702}";
pub const FILE_DOC: &str = "\u{EB1E}";
pub const FILE_TEXT: &str = "\u{E23A}";
pub const FILTER: &str = "\u{E268}"; // funnel-simple
pub const SORT: &str = "\u{E444}"; // sort-ascending
pub const COLLECTION: &str = "\u{E468}"; // stack-simple

/// Each label has its own icon, so labels differ by shape, not only colour.
pub fn label(kind: LabelKind) -> &'static str {
    match kind {
        LabelKind::Important => STAR,
        LabelKind::Review => REVIEW,
        LabelKind::Task => TASK,
        LabelKind::Definition => DEFINITION,
        LabelKind::Heading => HEADING,
    }
}

#[cfg(test)]
mod tests {
    use ab_glyph::{Font, FontRef};

    use super::*;

    #[test]
    fn every_icon_is_in_the_bundled_icon_font() {
        let font = FontRef::try_from_slice(crate::media::fonts::ICONS).unwrap();
        for icon in [RECORD, PAUSE, STOP, PLAY, BACK, FORWARD, STAR, REVIEW, TASK, DEFINITION, IMAGE, CAMERA, SKETCH, SEND, MORE, TRASH, PLUS, PREVIOUS, NEXT, SIDEBAR, MOON, SUN, SYSTEM, READING, SEARCH, SLIDES, LINK, UNLINK, PDF, TAG, CLOSE, UNDO, SETTINGS, EDIT, MICROPHONE, HEADING, HOME, COLLECTIONS, TASKS, BACK_ARROW, LAYOUT, ZOOM_IN, ZOOM_OUT, FULLSCREEN, COPY, EXPORT, WAVEFORM, CAPTIONS, SPEED, SOUND, SCREENSHOT, DOWNLOAD, TRANSCRIPT, TO_FEED, COMPUTER, STOP_CIRCLE, FORMULA, PEN, HIGHLIGHTER, ERASER, UNDO_STROKE, CLEAR, PRINT, FILE_PDF, FILE_DOC, FILE_TEXT, FILTER, SORT, COLLECTION] {
            let ch = icon.chars().next().unwrap();
            assert_ne!(font.glyph_id(ch).0, 0, "U+{:04X} is missing from Phosphor.ttf", ch as u32);
        }
    }
}
