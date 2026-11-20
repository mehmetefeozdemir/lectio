//! The paper view as one self-contained HTML page: pictures are embedded as
//! `data:` URLs, so the page works offline and the browser can print it
//! (Print in the web app) or save it.

use lectio_core::paper::{Block, PaperOptions, note_text, photo_word};
use lectio_core::timeline::format_offset;

use crate::{Pictures, base64, escape, picture_of};

const STYLE: &str = "body{font-family:Inter,system-ui,-apple-system,'Segoe UI',sans-serif;color:#1a1a1a;margin:0}\
main{max-width:46rem;margin:2rem auto;padding:0 1.5rem;line-height:1.55;font-size:1.05rem}\
h1{font-size:1.9rem;margin:0 0 .2rem}h2{font-size:1.3rem;margin:2rem 0 .6rem}\
.meta,.time,.labels,figcaption{color:#5c5c5c}.time{font-family:ui-monospace,monospace;font-size:.85em;margin-right:.5em}\
figure{margin:1.2rem 0}figure img{max-width:100%;height:auto;border:1px solid #ddd}\
p.formula img{vertical-align:middle;max-width:100%}code{white-space:pre-wrap}\
@media print{main{margin:0;max-width:none}figure,p.formula{break-inside:avoid}h2{break-after:avoid}}";

fn time(offset_ms: u64, options: &PaperOptions) -> String {
    if options.times { format!("<span class=\"time\">{}</span>", format_offset(offset_ms)) } else { String::new() }
}

fn labels(labels: &[String]) -> String {
    if labels.is_empty() { String::new() } else { format!(" <span class=\"labels\">({})</span>", escape(&labels.join(", "))) }
}

fn img(picture: &crate::Picture, alt: &str) -> String {
    format!(
        "<img alt=\"{}\" src=\"data:image/png;base64,{}\" width=\"{}\" height=\"{}\">",
        escape(alt),
        base64(&picture.png),
        picture.width,
        picture.height
    )
}

pub fn html(blocks: &[Block], options: &PaperOptions, pictures: &Pictures) -> String {
    let title = blocks.iter().find_map(|b| if let Block::Title { title, .. } = b { Some(title.as_str()) } else { None });
    let mut body = String::new();
    for block in blocks {
        let picture = picture_of(block, pictures);
        match block {
            Block::Title { title, subtitle } => {
                body.push_str(&format!("<h1>{}</h1>\n<p class=\"meta\">{}</p>\n", escape(title), escape(subtitle)));
            }
            Block::Heading { text, .. } => body.push_str(&format!("<h2>{}</h2>\n", escape(text))),
            Block::Note { offset_ms, text, labels: l } => {
                body.push_str(&format!("<p>{}{}{}</p>\n", time(*offset_ms, options), escape(note_text(text)), labels(l)));
            }
            Block::Formula { offset_ms, latex, labels: l } => {
                let shown = match &picture {
                    Some((p, alt)) => img(p, alt),
                    None => format!("<code>{}</code>", escape(latex)),
                };
                body.push_str(&format!("<p class=\"formula\">{}{shown}{}</p>\n", time(*offset_ms, options), labels(l)));
            }
            Block::Slide { page, deck_title, caption, offset_ms, .. } => {
                let when = match offset_ms {
                    Some(ms) if options.times => format!(" · {}", format_offset(*ms)),
                    _ => String::new(),
                };
                let caption = format!("{} · {}, page {page}{when}", escape(caption), escape(deck_title));
                match &picture {
                    Some((p, alt)) => body.push_str(&format!("<figure>{}<figcaption>{caption}</figcaption></figure>\n", img(p, alt))),
                    None => body.push_str(&format!("<p class=\"meta\">{caption}</p>\n")),
                }
            }
            Block::Image { kind, offset_ms, caption, .. } => {
                body.push_str(&format!("<p>{}{} {}</p>\n", time(*offset_ms, options), photo_word(*kind), escape(caption)));
                if let Some((p, alt)) = &picture {
                    body.push_str(&format!("<figure>{}</figure>\n", img(p, alt)));
                }
            }
            Block::Transcript { lines } => {
                body.push_str("<section>\n<h2>Transcript</h2>\n");
                for (ms, text) in lines {
                    body.push_str(&format!("<p>{}{}</p>\n", time(*ms, options), escape(text)));
                }
                body.push_str("</section>\n");
            }
        }
    }
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n<main>\n{body}</main>\n</body>\n</html>\n",
        escape(title.unwrap_or("Lectio notes"))
    )
}
