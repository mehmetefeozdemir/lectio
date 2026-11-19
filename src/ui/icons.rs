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
