//! Export structure (ADR-025): HTML for printing, DOCX and PDF, all written
//! from the paper view's blocks.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use lectio_core::material::{AttachmentKind, BlobKey, MediaKind, SlideLink};
use lectio_core::paper::{AssetKey, Block, PaperOptions, document};
use lectio_core::{LabelKind, Library, Recording};
use lectio_export::{Fonts, Picture, base64, docx, html, pdf};

const REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");

fn fonts() -> Fonts {
    Fonts { regular: REGULAR.to_vec(), bold: BOLD.to_vec() }
}

fn png(width: u32, height: u32) -> Picture {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([40, 90, 200, 255]));
    let mut bytes = Vec::new();
    image.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
    Picture { png: bytes, width, height }
}

/// A heading, a note about slide 2, a formula, a photo, a tricky note.
fn blocks() -> (Vec<Block>, HashMap<AssetKey, Picture>) {
    let (lib, course) = Library::default().with_course("CSCA67").unwrap();
    let (lib, s) = lib.with_session(course, "Graphs", 0).unwrap();
    let rec = Recording { file_name: "g.wav".into(), duration_ms: 600_000, sample_rate: 16_000, cleaned_file: None };
    let lib = lib.with_recording(s, rec).unwrap();
    let (lib, deck) = lib.with_deck(s, "W3.pdf", BlobKey::new_random(MediaKind::Pdf), 3).unwrap();
    let (lib, _) = lib.with_note(s, 0, "Breadth-first search", &[LabelKind::Heading]).unwrap();
    let (lib, a) = lib.with_note(s, 1_000, "BFS uses a queue", &[LabelKind::Review]).unwrap();
    let lib = lib.with_slide_link(s, a, Some(SlideLink { deck, page: 2 })).unwrap();
    let (lib, _) = lib.with_math_note(s, 2_000, r"\frac{n}{2}", &[]).unwrap();
    let photo = lectio_core::material::Attachment {
        kind: AttachmentKind::Photo,
        blob: BlobKey::new_random(MediaKind::Jpeg),
        width: 40,
        height: 30,
    };
    let (lib, _) = lib.with_attachment_note(s, 3_000, photo, "board", &[]).unwrap();
    let (lib, _) = lib.with_note(s, 4_000, "a < b & \"c\"", &[]).unwrap();
    let blocks = document(&lib, s, &PaperOptions::default()).unwrap();
    // The photo's picture is missing on purpose: exports keep its caption.
    let pictures = HashMap::from([
        (AssetKey::Slide(deck, 2), png(160, 90)),
        (AssetKey::Formula(r"\frac{n}{2}".into()), png(60, 40)),
    ]);
    (blocks, pictures)
}

#[test]
fn base64_matches_the_standard_alphabet_and_padding() {
    assert_eq!(base64(b"Man"), "TWFu");
    assert_eq!(base64(b"Ma"), "TWE=");
    assert_eq!(base64(b"M"), "TQ==");
    assert_eq!(base64(&[0xFB, 0xFF]), "+/8=");
}

#[test]
fn html_is_a_printable_page_with_pictures_inline_and_text_escaped() {
    let (blocks, pictures) = blocks();

    let page = html(&blocks, &PaperOptions::default(), &pictures);

    assert!(page.starts_with("<!DOCTYPE html>"));
    assert!(page.contains("<html lang=\"en\">"));
    assert!(page.contains("<h1>Graphs</h1>"));
    assert!(page.contains("<h2>Breadth-first search</h2>"));
    assert!(page.contains("alt=\"Slide 2 (W3, page 2)\" src=\"data:image/png;base64,"));
    assert!(page.contains("alt=\"\\frac{n}{2}\""), "a formula's alt text is its LaTeX");
    assert!(page.contains("a &lt; b &amp; &quot;c&quot;"));
    assert!(page.contains("[photo] board") && page.matches("<img").count() == 2, "a missing picture is skipped");
    assert!(page.contains("@media print"));
}

fn unzip(bytes: &[u8]) -> HashMap<String, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut files = HashMap::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let mut content = Vec::new();
        file.read_to_end(&mut content).unwrap();
        let name: String = file.name().unwrap().to_string();
        files.insert(name, String::from_utf8_lossy(&content).into_owned());
    }
    files
}

#[test]
fn docx_is_a_zip_of_the_word_parts_with_headings_and_pictures() {
    let (blocks, pictures) = blocks();

    let bytes = docx(&blocks, &PaperOptions::default(), &pictures).unwrap();

    let files = unzip(&bytes);
    for part in ["[Content_Types].xml", "_rels/.rels", "word/document.xml", "word/styles.xml", "word/_rels/document.xml.rels"] {
        assert!(files.contains_key(part), "missing {part}");
    }
    assert!(files.contains_key("word/media/image1.png") && files.contains_key("word/media/image2.png"));
    assert!(!files.contains_key("word/media/image3.png"));
    let body = &files["word/document.xml"];
    assert!(body.contains(r#"<w:pStyle w:val="Title"/>"#) && body.contains(">Graphs<"));
    assert!(body.contains(r#"<w:pStyle w:val="Heading1"/>"#) && body.contains(">Breadth-first search<"));
    assert!(body.contains("BFS uses a queue"));
    assert!(body.contains("a &lt; b &amp; &quot;c&quot;"));
    assert!(body.contains(r#"descr="\frac{n}{2}""#), "pictures carry alt text");
    assert!(files["word/_rels/document.xml.rels"].contains("media/image2.png"));
    assert!(files["[Content_Types].xml"].contains(r#"Extension="png""#));
}

#[test]
fn pdf_is_a_valid_document_that_breaks_onto_new_pages() {
    let (blocks, pictures) = blocks();
    let short = pdf(&blocks, &PaperOptions::default(), &pictures, &fonts()).unwrap();
    let mut long_blocks = blocks.clone();
    for i in 0..200 {
        long_blocks.push(Block::Note { offset_ms: i * 1_000, text: format!("note {i} about graph search"), labels: vec![] });
    }

    let long = pdf(&long_blocks, &PaperOptions::default(), &pictures, &fonts()).unwrap();

    assert!(short.starts_with(b"%PDF-"));
    let pages = |bytes: &[u8]| hayro::hayro_syntax::Pdf::new(bytes.to_vec()).unwrap().pages().len();
    assert_eq!(pages(&short), 1);
    assert!(pages(&long) >= 4, "200 notes need several A4 pages");
}

#[test]
fn a_bad_font_is_an_error_not_a_crash() {
    let (blocks, pictures) = blocks();
    let broken = Fonts { regular: vec![1, 2, 3], bold: vec![] };
    assert!(pdf(&blocks, &PaperOptions::default(), &pictures, &broken).is_err());
}
