//! Phase 4b: slide ink (ADR-024), math formulas (ADR-023) and the paper
//! document model behind the paper view and every export (ADR-025).

use lectio_core::ink::{InkColor, InkStroke, InkTool, stroke_hits};
use lectio_core::material::{AttachmentKind, BlobKey, MediaKind, SlideLink};
use lectio_core::math::{GlyphExtent, MathFont, MathItem, Metrics, parse, typeset};
use lectio_core::paper::{AssetKey, Block, PaperOptions, assets, document, markdown};
use lectio_core::transcript::{Segment, Transcript};
use lectio_core::{DeckId, LabelKind, Library, ModelError, NoteKind, Recording, SessionId};

// ---------- slide ink ----------

fn session_with_deck() -> (Library, SessionId, DeckId) {
    let (lib, course) = Library::default().with_course("CSCA67").unwrap();
    let (lib, s) = lib.with_session(course, "Graphs", 0).unwrap();
    let (lib, deck) = lib.with_deck(s, "W3.pdf", BlobKey::new_random(MediaKind::Pdf), 3).unwrap();
    (lib, s, deck)
}

fn pen(points: &[[f32; 2]]) -> InkStroke {
    InkStroke { tool: InkTool::Pen, color: InkColor::Red, width: 0.004, points: points.to_vec() }
}

fn ink(lib: &Library, s: SessionId, page: u32) -> Vec<InkStroke> {
    lib.session(s).unwrap().decks[0].ink_on(page).to_vec()
}

#[test]
fn strokes_are_stored_per_page_in_page_fractions_rounded_to_a_ten_thousandth() {
    let (lib, s, deck) = session_with_deck();

    let lib = lib.with_ink_stroke(s, deck, 2, pen(&[[0.123_456, 0.5], [0.8, 0.654_321]])).unwrap();

    assert_eq!(ink(&lib, s, 2)[0].points, vec![[0.1235, 0.5], [0.8, 0.6543]]);
    assert!(ink(&lib, s, 1).is_empty(), "other pages are untouched");
    let reloaded = Library::from_json(&lib.to_json().unwrap()).unwrap();
    assert_eq!(ink(&reloaded, s, 2), ink(&lib, s, 2));
}

#[test]
fn bad_strokes_are_refused() {
    let (lib, s, deck) = session_with_deck();
    let too_long: Vec<[f32; 2]> = (0..2_001).map(|i| [i as f32 / 2_001.0, 0.5]).collect();

    assert!(matches!(lib.with_ink_stroke(s, deck, 4, pen(&[[0.1, 0.1]])), Err(ModelError::PageOutOfRange { .. })));
    assert!(matches!(lib.with_ink_stroke(s, deck, 1, pen(&[])), Err(ModelError::InvalidInk(_))));
    assert!(matches!(lib.with_ink_stroke(s, deck, 1, pen(&[[1.5, 0.1]])), Err(ModelError::InvalidInk(_))));
    assert!(matches!(lib.with_ink_stroke(s, deck, 1, pen(&[[f32::NAN, 0.1]])), Err(ModelError::InvalidInk(_))));
    assert!(matches!(lib.with_ink_stroke(s, deck, 1, pen(&too_long)), Err(ModelError::InvalidInk(_))));
    assert_eq!(lib.with_ink_stroke(s, DeckId::new_random(), 1, pen(&[[0.1, 0.1]])).unwrap_err(), ModelError::DeckNotFound);
}

#[test]
fn the_eraser_removes_whole_strokes_it_touches() {
    let (lib, s, deck) = session_with_deck();
    let lib = lib.with_ink_stroke(s, deck, 1, pen(&[[0.1, 0.1], [0.9, 0.1]])).unwrap(); // top line
    let lib = lib.with_ink_stroke(s, deck, 1, pen(&[[0.1, 0.9], [0.9, 0.9]])).unwrap(); // bottom line

    let (lib, removed) = lib.with_ink_erased(s, deck, 1, [0.5, 0.11], 0.02).unwrap();

    assert_eq!(removed, 1);
    assert_eq!(ink(&lib, s, 1)[0].points[0], [0.1, 0.9], "only the bottom line is left");
    let (_, removed) = lib.with_ink_erased(s, deck, 1, [0.5, 0.5], 0.02).unwrap();
    assert_eq!(removed, 0, "nothing near the middle of the page");
}

#[test]
fn hit_testing_measures_the_distance_to_each_segment() {
    let line = pen(&[[0.0, 0.0], [1.0, 0.0]]);
    assert!(stroke_hits(&line, [0.5, 0.01], 0.02));
    assert!(!stroke_hits(&line, [0.5, 0.05], 0.02));
    assert!(!stroke_hits(&line, [1.05, 0.0], 0.02), "beyond the end of the segment");
    assert!(stroke_hits(&pen(&[[0.3, 0.3]]), [0.31, 0.3], 0.02), "a dot");
}

#[test]
fn undo_and_clear_work_page_by_page() {
    let (lib, s, deck) = session_with_deck();
    let lib = lib.with_ink_stroke(s, deck, 1, pen(&[[0.1, 0.1]])).unwrap();
    let lib = lib.with_ink_stroke(s, deck, 1, pen(&[[0.2, 0.2]])).unwrap();
    let lib = lib.with_ink_stroke(s, deck, 2, pen(&[[0.3, 0.3]])).unwrap();

    let lib = lib.without_last_stroke(s, deck, 1).unwrap();
    assert_eq!(ink(&lib, s, 1).len(), 1);
    let lib = lib.without_page_ink(s, deck, 1).unwrap();
    assert!(ink(&lib, s, 1).is_empty());
    assert_eq!(ink(&lib, s, 2).len(), 1);
    assert!(lib.session(s).unwrap().decks[0].ink.iter().all(|p| !p.strokes.is_empty()), "no empty pages kept");
}

// ---------- math ----------

/// Every glyph is half as wide as the size, 0.7 above and 0.2 below the baseline.
struct Fixed;

impl Metrics for Fixed {
    fn glyph(&self, _ch: char, _font: MathFont, size: f32) -> GlyphExtent {
        GlyphExtent { advance: 0.5 * size, ascent: 0.7 * size, descent: 0.2 * size }
    }
}

fn glyphs(items: &[MathItem]) -> Vec<(char, f32, f32, f32)> {
    items
        .iter()
        .filter_map(|i| match i {
            MathItem::Glyph { ch, x, baseline, size, .. } => Some((*ch, *x, *baseline, *size)),
            _ => None,
        })
        .collect()
}

#[test]
fn letters_become_math_italic_and_greek_names_become_letters() {
    let shape = typeset(r"x + h \cdot \alpha", 20.0, &Fixed).unwrap();
    let chars: String = glyphs(&shape.items).iter().map(|g| g.0).collect();
    assert_eq!(chars, "𝑥+ℎ⋅𝛼");
}

#[test]
fn a_superscript_is_smaller_and_raised() {
    let shape = typeset("x^2", 20.0, &Fixed).unwrap();
    let g = glyphs(&shape.items);
    assert_eq!(g.len(), 2);
    assert!(g[1].3 < g[0].3, "smaller");
    assert!(g[1].2 < g[0].2, "its baseline is higher (y grows downwards)");
    assert!(g[1].1 >= g[0].1 + 10.0, "after the x");
}

#[test]
fn a_fraction_has_a_bar_with_the_numerator_above_and_the_denominator_below() {
    let shape = typeset(r"\frac{a}{b}", 20.0, &Fixed).unwrap();
    let bar = shape.items.iter().find_map(|i| match i {
        MathItem::Rule { y, width, .. } => Some((*y, *width)),
        _ => None,
    });
    let (bar_y, bar_width) = bar.expect("a fraction bar");
    let g = glyphs(&shape.items);
    assert!(g[0].2 < bar_y && g[1].2 > bar_y + 4.0);
    assert!(bar_width >= 10.0 && shape.width >= bar_width);
    assert!(shape.ascent > 20.0 * 0.7, "taller than a single letter");
}

#[test]
fn a_root_is_drawn_with_lines_over_its_contents() {
    let shape = typeset(r"\sqrt{x}", 20.0, &Fixed).unwrap();
    let lines = shape.items.iter().filter(|i| matches!(i, MathItem::Line { .. })).count();
    assert!(lines >= 3, "the tick, the long stroke and the bar over x");
    assert!(typeset(r"\sqrt[3]{x}", 20.0, &Fixed).unwrap().width > shape.width, "room for the index");
}

#[test]
fn a_sum_puts_its_limits_above_and_below() {
    let shape = typeset(r"\sum_{i=1}^{n} i", 20.0, &Fixed).unwrap();
    let g = glyphs(&shape.items);
    let sum = g.iter().find(|g| g.0 == '∑').unwrap();
    let n = g.iter().find(|g| g.0 == '𝑛').unwrap();
    let below = g.iter().find(|g| g.0 == '=').unwrap();
    assert!(n.2 < sum.2 - 10.0, "n above");
    assert!(below.2 > sum.2 + 4.0, "i = 1 below");
}

#[test]
fn text_inside_a_formula_uses_the_text_font_and_keeps_its_spaces() {
    let shape = typeset(r"\text{if } x > 0", 20.0, &Fixed).unwrap();
    let text: Vec<_> = shape
        .items
        .iter()
        .filter_map(|i| match i {
            MathItem::Glyph { ch, font: MathFont::Text, .. } => Some(*ch),
            _ => None,
        })
        .collect();
    assert_eq!(text, vec!['i', 'f']);
    assert!(shape.width > 20.0 * 0.5 * 5.0, "the space after \"if\" and around > take room");
}

#[test]
fn formula_errors_say_what_is_wrong() {
    let message = |src: &str| parse(src).unwrap_err().message;
    assert!(message(r"\foo + 1").contains(r"\foo"));
    assert!(message("{x").contains("}"));
    assert!(message("x^").contains("^"));
    assert!(message(r"\begin{matrix} a \end{matrix}").contains("not supported"));
    assert!(message("   ").contains("formula"));
    assert!(parse(r"\left( \frac{1}{2} \right)^2 \le \binom{n}{k} \to \infty").is_ok());
}

// ---------- paper document ----------

/// Heading, two notes about slide 2, slide 3 posted then a note about it,
/// a formula, a photo, a label-only moment.
fn lecture() -> (Library, SessionId, DeckId) {
    let (lib, s, deck) = session_with_deck();
    let rec = Recording { file_name: "g.wav".into(), duration_ms: 600_000, sample_rate: 16_000, cleaned_file: None };
    let lib = lib.with_recording(s, rec).unwrap();
    let (lib, _) = lib.with_note(s, 0, "Breadth-first search", &[LabelKind::Heading]).unwrap();
    let (lib, a) = lib.with_note(s, 1_000, "BFS uses a queue", &[]).unwrap();
    let lib = lib.with_slide_link(s, a, Some(SlideLink { deck, page: 2 })).unwrap();
    let (lib, b) = lib.with_note(s, 2_000, "level by level", &[LabelKind::Review]).unwrap();
    let lib = lib.with_slide_link(s, b, Some(SlideLink { deck, page: 2 })).unwrap();
    let (lib, _) = lib.with_slide_item(s, 3_000, SlideLink { deck, page: 3 }).unwrap();
    let (lib, c) = lib.with_note(s, 4_000, "distance order", &[]).unwrap();
    let lib = lib.with_slide_link(s, c, Some(SlideLink { deck, page: 3 })).unwrap();
    let (lib, _) = lib.with_math_note(s, 5_000, r"\frac{n(n+1)}{2}", &[]).unwrap();
    let photo = lectio_core::material::Attachment {
        kind: AttachmentKind::Photo,
        blob: BlobKey::new_random(MediaKind::Jpeg),
        width: 400,
        height: 300,
    };
    let (lib, _) = lib.with_attachment_note(s, 6_000, photo, "board", &[]).unwrap();
    let (lib, _) = lib.with_note(s, 7_000, "", &[LabelKind::Important]).unwrap();
    (lib, s, deck)
}

fn kinds(blocks: &[Block]) -> Vec<&'static str> {
    blocks
        .iter()
        .map(|b| match b {
            Block::Title { .. } => "title",
            Block::Heading { .. } => "heading",
            Block::Note { .. } => "note",
            Block::Formula { .. } => "formula",
            Block::Slide { .. } => "slide",
            Block::Image { .. } => "image",
            Block::Transcript { .. } => "transcript",
        })
        .collect()
}

#[test]
fn the_document_shows_each_slide_once_before_the_first_note_about_it() {
    let (lib, s, _) = lecture();

    let blocks = document(&lib, s, &PaperOptions::default()).unwrap();

    assert_eq!(
        kinds(&blocks),
        vec!["title", "heading", "slide", "note", "note", "slide", "note", "formula", "image", "note"]
    );
    let pages: Vec<u32> = blocks.iter().filter_map(|b| if let Block::Slide { page, .. } = b { Some(*page) } else { None }).collect();
    assert_eq!(pages, vec![2, 3]);
    match &blocks[4] {
        Block::Note { labels, offset_ms, .. } => assert_eq!((labels.clone(), *offset_ms), (vec!["Review".to_owned()], 2_000)),
        other => panic!("expected the second note, got {other:?}"),
    }
}

#[test]
fn the_transcript_is_added_at_the_end_only_when_asked() {
    let (lib, s, _) = lecture();
    let segments = vec![Segment { start_ms: 1_000, end_ms: 3_000, text: "Today: graphs.".into() }];
    let lib = lib.with_transcript(s, Transcript { language: "en".into(), model: "tiny".into(), segments, complete: true }).unwrap();

    let without = document(&lib, s, &PaperOptions::default()).unwrap();
    let with = document(&lib, s, &PaperOptions { transcript: true, ..PaperOptions::default() }).unwrap();

    assert!(!kinds(&without).contains(&"transcript"));
    assert_eq!(kinds(&with).last(), Some(&"transcript"));
}

#[test]
fn every_picture_the_exports_need_is_listed_once() {
    let (lib, s, deck) = lecture();
    let blocks = document(&lib, s, &PaperOptions::default()).unwrap();

    let needed = assets(&blocks);

    assert_eq!(needed.len(), 4);
    assert_eq!(needed[0], AssetKey::Slide(deck, 2));
    assert_eq!(needed[1], AssetKey::Slide(deck, 3));
    assert_eq!(needed[2], AssetKey::Formula(r"\frac{n(n+1)}{2}".into()));
    assert!(matches!(needed[3], AssetKey::Image(_)));
}

#[test]
fn markdown_keeps_headings_times_formulas_and_slide_references() {
    let (lib, s, _) = lecture();
    let blocks = document(&lib, s, &PaperOptions::default()).unwrap();

    let text = markdown(&blocks, &PaperOptions::default());

    assert!(text.starts_with("# Graphs\n"));
    assert!(text.contains("\n## Breadth-first search\n"));
    assert!(text.contains("- 0:01 BFS uses a queue\n"));
    assert!(text.contains("- 0:02 level by level *(Review)*\n"));
    assert!(text.contains("**Slide 2** (W3, page 2)"), "slide shown before the notes about it");
    assert!(text.contains("**Slide 3** (W3, page 3) · 0:03"), "a posted slide keeps its time");
    assert!(text.contains("$$\n\\frac{n(n+1)}{2}\n$$"));
    assert!(text.contains("[photo] board"));
    assert!(text.contains("(marked moment) *(Important)*"));
    let no_times = markdown(&blocks, &PaperOptions { times: false, ..PaperOptions::default() });
    assert!(no_times.contains("- BFS uses a queue\n"));
}

#[test]
fn formula_notes_must_be_valid_latex() {
    let (lib, s, _) = session_with_deck();

    assert!(matches!(lib.with_math_note(s, 0, r"\frac{1}{", &[]), Err(ModelError::InvalidFormula(_))));
    let (lib, id) = lib.with_math_note(s, 0, "  e^{i\\pi} + 1 = 0 ", &[]).unwrap();
    let note = lib.session(s).unwrap().note(id).unwrap().clone();
    assert_eq!((note.kind, note.body.as_str()), (NoteKind::Math, "e^{i\\pi} + 1 = 0"));
    assert!(matches!(lib.with_note_text(s, id, r"\nope"), Err(ModelError::InvalidFormula(_))), "editing is checked too");
}
