//! The paper view as a Word document. A .docx is a zip of XML "parts"
//! (Office Open XML): the text in `word/document.xml`, styles in
//! `word/styles.xml`, pictures in `word/media/`, and small files that say
//! which part is what. I write only what notes need: a title, headings,
//! paragraphs and pictures with alt text.

use std::collections::HashMap;
use std::io::{Cursor, Write};

use lectio_core::paper::{AssetKey, Block, PaperOptions, note_text, photo_word};
use lectio_core::timeline::format_offset;
use zip::write::SimpleFileOptions;

use crate::{Picture, Pictures, escape, picture_of};

/// Word measures pictures in EMU: 914 400 per inch, 9 525 per CSS pixel.
const EMU_PER_PX: u64 = 9_525;
/// The text width of an A4 page with 2.54 cm margins (6.27 in).
const MAX_WIDTH_EMU: u64 = 5_731_510;
const GRAY: &str = "5C5C5C";

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/></Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>"#;

/// Normal text, Title, Subtitle, Heading 1 (with an outline level, so Word's
/// navigation pane and screen readers see it as a heading) and Caption.
const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/><w:lang w:val="en"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="120" w:line="300" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="60"/></w:pPr><w:rPr><w:b/><w:sz w:val="48"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Subtitle"><w:name w:val="Subtitle"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="360"/></w:pPr><w:rPr><w:color w:val="5C5C5C"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:pPr><w:keepNext/><w:spacing w:before="360" w:after="120"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Caption"><w:name w:val="caption"/><w:basedOn w:val="Normal"/><w:rPr><w:color w:val="5C5C5C"/><w:sz w:val="18"/></w:rPr></w:style></w:styles>"#;

/// Collects pictures as they are used; each picture is stored once.
#[derive(Default)]
struct Media {
    files: Vec<Vec<u8>>,
    index: HashMap<AssetKey, usize>,
}

fn key_of(block: &Block) -> Option<AssetKey> {
    match block {
        Block::Slide { deck, page, .. } => Some(AssetKey::Slide(*deck, *page)),
        Block::Formula { latex, .. } => Some(AssetKey::Formula(latex.clone())),
        Block::Image { blob, .. } => Some(AssetKey::Image(blob.clone())),
        _ => None,
    }
}

impl Media {
    /// The 1-based number of the picture's file, adding it if new.
    fn number(&mut self, key: AssetKey, picture: &Picture) -> usize {
        *self.index.entry(key).or_insert_with(|| {
            self.files.push(picture.png.clone());
            self.files.len()
        })
    }
}

fn run(text: &str, gray: bool) -> String {
    let props = if gray { format!("<w:rPr><w:color w:val=\"{GRAY}\"/></w:rPr>") } else { String::new() };
    format!("<w:r>{props}<w:t xml:space=\"preserve\">{}</w:t></w:r>", escape(text))
}

fn paragraph(style: Option<&str>, runs: &str) -> String {
    let props = style.map(|s| format!("<w:pPr><w:pStyle w:val=\"{s}\"/></w:pPr>")).unwrap_or_default();
    format!("<w:p>{props}{runs}</w:p>")
}

/// An inline picture, scaled down to the text width if needed.
fn drawing(number: usize, picture: &Picture, alt: &str) -> String {
    let (mut cx, mut cy) = (u64::from(picture.width) * EMU_PER_PX, u64::from(picture.height) * EMU_PER_PX);
    if cx > MAX_WIDTH_EMU {
        cy = cy * MAX_WIDTH_EMU / cx;
        cx = MAX_WIDTH_EMU;
    }
    let alt = escape(alt);
    format!(
        "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/>\
         <wp:docPr id=\"{number}\" name=\"Picture {number}\" descr=\"{alt}\"/>\
         <a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
         <pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:nvPicPr><pic:cNvPr id=\"{number}\" name=\"image{number}.png\" descr=\"{alt}\"/><pic:cNvPicPr/></pic:nvPicPr>\
         <pic:blipFill><a:blip r:embed=\"rIdImg{number}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
         <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>\
         </a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
    )
}

fn time(offset_ms: u64, options: &PaperOptions) -> String {
    if options.times { run(&format!("{}  ", format_offset(offset_ms)), true) } else { String::new() }
}

fn labels(labels: &[String]) -> String {
    if labels.is_empty() { String::new() } else { run(&format!("  ({})", labels.join(", ")), true) }
}

fn body(blocks: &[Block], options: &PaperOptions, pictures: &Pictures, media: &mut Media) -> String {
    let mut out = String::new();
    for block in blocks {
        let picture = picture_of(block, pictures)
            .and_then(|(p, alt)| key_of(block).map(|key| drawing(media.number(key, p), p, &alt)));
        let paragraphs = match block {
            Block::Title { title, subtitle } => paragraph(Some("Title"), &run(title, false)) + &paragraph(Some("Subtitle"), &run(subtitle, false)),
            Block::Heading { text, .. } => paragraph(Some("Heading1"), &run(text, false)),
            Block::Note { offset_ms, text, labels: l } => {
                paragraph(None, &(time(*offset_ms, options) + &run(note_text(text), false) + &labels(l)))
            }
            Block::Formula { offset_ms, latex, labels: l } => {
                let shown = picture.unwrap_or_else(|| run(latex, false));
                paragraph(None, &(time(*offset_ms, options) + &shown + &labels(l)))
            }
            Block::Slide { page, deck_title, caption, offset_ms, .. } => {
                let when = match offset_ms {
                    Some(ms) if options.times => format!(" · {}", format_offset(*ms)),
                    _ => String::new(),
                };
                let caption = paragraph(Some("Caption"), &run(&format!("{caption} · {deck_title}, page {page}{when}"), false));
                picture.map(|p| paragraph(None, &p)).unwrap_or_default() + &caption
            }
            Block::Image { kind, offset_ms, caption, .. } => {
                let line = paragraph(None, &(time(*offset_ms, options) + &run(&format!("{} {caption}", photo_word(*kind)), false)));
                line + &picture.map(|p| paragraph(None, &p)).unwrap_or_default()
            }
            Block::Transcript { lines } => {
                let mut section = paragraph(Some("Heading1"), &run("Transcript", false));
                for (ms, text) in lines {
                    section.push_str(&paragraph(None, &(time(*ms, options) + &run(text, false))));
                }
                section
            }
        };
        out.push_str(&paragraphs);
    }
    out
}

fn document_xml(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
         xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\"><w:body>{body}\
         <w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/></w:sectPr>\
         </w:body></w:document>"
    )
}

fn relationships(images: usize) -> String {
    let mut rels = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
         <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
    );
    for n in 1..=images {
        rels.push_str(&format!(
            "<Relationship Id=\"rIdImg{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/image{n}.png\"/>"
        ));
    }
    rels.push_str("</Relationships>");
    rels
}

fn core_properties(title: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cp:coreProperties \
         xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title><dc:creator>Lectio</dc:creator><dc:language>en</dc:language></cp:coreProperties>",
        escape(title)
    )
}

pub fn docx(blocks: &[Block], options: &PaperOptions, pictures: &Pictures) -> Result<Vec<u8>, String> {
    let mut media = Media::default();
    let body = body(blocks, options, pictures, &mut media);
    let title = blocks.iter().find_map(|b| if let Block::Title { title, .. } = b { Some(title.as_str()) } else { None });
    let mut parts: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".into(), CONTENT_TYPES.into()),
        ("_rels/.rels".into(), ROOT_RELS.into()),
        ("docProps/core.xml".into(), core_properties(title.unwrap_or("Lectio notes")).into_bytes()),
        ("word/document.xml".into(), document_xml(&body).into_bytes()),
        ("word/styles.xml".into(), STYLES.into()),
        ("word/_rels/document.xml.rels".into(), relationships(media.files.len()).into_bytes()),
    ];
    for (i, png) in media.files.into_iter().enumerate() {
        parts.push((format!("word/media/image{}.png", i + 1), png));
    }
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let deflated = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let failed = |e: &dyn std::fmt::Display| format!("The Word file could not be written ({e}).");
    for (name, bytes) in parts {
        zip.start_file(name, deflated).map_err(|e| failed(&e))?;
        zip.write_all(&bytes).map_err(|e| failed(&e))?;
    }
    zip.finish().map(Cursor::into_inner).map_err(|e| failed(&e))
}
