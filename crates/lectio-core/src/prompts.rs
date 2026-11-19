//! What the study tools ask a model (ADR-021). Small local models follow
//! short, concrete instructions and simple line formats far better than JSON,
//! so each prompt shows the exact format, and the parsers (`outline`,
//! `quiz`, `cards`) ignore anything else. Every prompt insists on source
//! numbers, so each item can be traced to a moment or a slide.

use crate::sources::{Chunk, Source, estimate_tokens};

/// Least material worth outlining (about 600 characters: ten short notes or
/// a minute of transcript). Measured: six notes (about 95 tokens) were only
/// repeated back, which the parser then drops; fourteen (about 250) gave
/// eight real headings (PROBLEMS P-036).
pub const MIN_OUTLINE_TOKENS: usize = 150;

/// Is there enough to summarise, or would an outline just repeat it?
pub fn enough_for_outline(sources: &[Source]) -> bool {
    sources.iter().map(|s| estimate_tokens(&s.text)).sum::<usize>() >= MIN_OUTLINE_TOKENS
}

/// Most tokens of material per request (the rest of the context is the
/// instructions and the answer). Small enough for the 0.5B model on a laptop.
pub const CHUNK_TOKENS: usize = 1_500;
/// Longest answer asked for, per request.
pub const OUTLINE_REPLY_TOKENS: usize = 300;
pub const QUIZ_REPLY_TOKENS: usize = 700;
pub const CARDS_REPLY_TOKENS: usize = 500;
/// Flashcards asked for per chunk.
pub const CARDS_PER_CHUNK: usize = 5;
/// A whole collection reads at most this many chunks.
pub const MAX_CHUNKS: usize = 8;

const RULES: &str = "You help a student study their own lecture material. Use only the numbered material \
                     given. After every item, write the numbers of the material it comes from, like [S3] or \
                     Sources: S3, S7. Never invent numbers. Write nothing except the requested lines.";

fn material(chunk: &Chunk) -> String {
    format!("Material:\n{}", chunk.text)
}

// The examples use an unrelated topic (the solar system) on purpose: given
// placeholders like "Q: question", small models copy them word for word.

/// (system, user) for an outline of one chunk.
pub fn outline(chunk: &Chunk) -> (String, String) {
    let user = format!(
        "{}\nGroup this material into 3 to 6 sections, in order. For each section write a short title of 2 to 5 words \
         in your own words (do not copy a line), then the numbers of the lines it covers.\n\
         Example for other material:\n- Planets near the Sun [S1, S2]\n- Gas giants [S3, S5]\n- How orbits work [S6]",
        material(chunk)
    );
    (RULES.to_owned(), user)
}

/// (system, user) for `count` quiz questions on one chunk, about `topics`
/// (empty = everything).
pub fn quiz(chunk: &Chunk, count: usize, topics: &[String]) -> (String, String) {
    let focus = if topics.is_empty() { String::new() } else { format!("Focus on: {}.\n", topics.join("; ")) };
    let user = format!(
        "{}\n{focus}Write {count} questions that test understanding of this material. Mix multiple choice (4 different \
         choices, one right) and short answer. Follow the format of this example about other material:\n\
         Q: Which planet is closest to the Sun?\nA) Venus\nB) Mercury\nC) Mars\nD) Earth\nAnswer: B\n\
         Why: Mercury has the smallest orbit.\nSources: S1\n\n\
         Q: What keeps the planets in orbit?\nAnswer: Gravity\nWhy: The Sun's gravity pulls on each planet.\nSources: S6",
        material(chunk)
    );
    (RULES.to_owned(), user)
}

/// (system, user) for up to `count` flashcards on one chunk.
pub fn cards(chunk: &Chunk, count: usize) -> (String, String) {
    let user = format!(
        "{}\nWrite {count} flashcards for the key terms and facts in this material. Follow the format of this example \
         about other material:\nFront: Orbit\nBack: The path a planet takes around the Sun\nSources: S6",
        material(chunk)
    );
    (RULES.to_owned(), user)
}
