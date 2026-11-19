//! Turns Whisper's output token ids back into text, and finds the ids of its
//! special tokens (`<|startoftranscript|>`, `<|en|>`, `<|0.00|>`, …).
//!
//! Whisper uses *byte-level BPE*: each token stands for a few bytes of UTF-8
//! text, written in `tokenizer.json` with a printable stand-in character for
//! every byte (GPT-2's "bytes to unicode" table). Decoding only needs that
//! table and the vocabulary, so no tokenizer library is needed (encoding text
//! into tokens, which is the hard part, is never needed here).

use std::collections::HashMap;

use serde_json::Value;

pub struct Tokenizer {
    /// Bytes for each id; `None` for special tokens (never shown as text).
    pieces: Vec<Option<Vec<u8>>>,
    special: HashMap<String, u32>,
}

/// GPT-2's table: byte → the printable char that stands for it.
fn byte_chars() -> Vec<(u8, char)> {
    let printable = |b: u8| (b'!'..=b'~').contains(&b) || (0xA1..=0xAC).contains(&b) || (0xAE..=0xFF).contains(&b);
    let mut extra = 0u32;
    (0..=255u8)
        .map(|b| {
            if printable(b) {
                (b, char::from(b))
            } else {
                let c = char::from_u32(256 + extra).unwrap_or('?');
                extra += 1;
                (b, c)
            }
        })
        .collect()
}

impl Tokenizer {
    pub fn from_json(json: &[u8]) -> Result<Self, String> {
        let root: Value = serde_json::from_slice(json).map_err(|e| format!("tokenizer.json is not valid JSON: {e}"))?;
        let vocab = root["model"]["vocab"].as_object().ok_or("tokenizer.json has no vocabulary")?;
        let char_to_byte: HashMap<char, u8> = byte_chars().into_iter().map(|(b, c)| (c, b)).collect();
        let mut pieces: Vec<Option<Vec<u8>>> = Vec::new();
        let mut put = |id: usize, value: Option<Vec<u8>>| {
            if pieces.len() <= id {
                pieces.resize(id + 1, None);
            }
            pieces[id] = value;
        };
        for (token, id) in vocab {
            let id = id.as_u64().ok_or("a vocabulary id is not a number")? as usize;
            let bytes = token.chars().filter_map(|c| char_to_byte.get(&c).copied()).collect();
            put(id, Some(bytes));
        }
        let mut special = HashMap::new();
        for added in root["added_tokens"].as_array().into_iter().flatten() {
            let (Some(id), Some(content)) = (added["id"].as_u64(), added["content"].as_str()) else { continue };
            put(id as usize, None);
            special.insert(content.to_owned(), id as u32);
        }
        Ok(Self { pieces, special })
    }

    /// The id of a special token such as `<|en|>`.
    pub fn id(&self, token: &str) -> Option<u32> {
        self.special.get(token).copied()
    }

    /// Text for `ids`; special and timestamp tokens are skipped.
    pub fn decode(&self, ids: &[u32]) -> String {
        let bytes: Vec<u8> =
            ids.iter().filter_map(|&id| self.pieces.get(id as usize).and_then(Option::as_ref)).flatten().copied().collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Language tokens the model knows, as (code, id), e.g. ("en", 50259).
    pub fn languages(&self) -> Vec<(String, u32)> {
        let mut out: Vec<(String, u32)> = LANGUAGES
            .iter()
            .filter_map(|(code, _)| self.id(&format!("<|{code}|>")).map(|id| ((*code).to_owned(), id)))
            .collect();
        out.sort_by_key(|(_, id)| *id);
        out
    }
}

/// Whisper's languages: (code, English name). The UI lists these.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"), ("zh", "Chinese"), ("de", "German"), ("es", "Spanish"), ("ru", "Russian"), ("ko", "Korean"),
    ("fr", "French"), ("ja", "Japanese"), ("pt", "Portuguese"), ("tr", "Turkish"), ("pl", "Polish"), ("ca", "Catalan"),
    ("nl", "Dutch"), ("ar", "Arabic"), ("sv", "Swedish"), ("it", "Italian"), ("id", "Indonesian"), ("hi", "Hindi"),
    ("fi", "Finnish"), ("vi", "Vietnamese"), ("he", "Hebrew"), ("uk", "Ukrainian"), ("el", "Greek"), ("ms", "Malay"),
    ("cs", "Czech"), ("ro", "Romanian"), ("da", "Danish"), ("hu", "Hungarian"), ("ta", "Tamil"), ("no", "Norwegian"),
    ("th", "Thai"), ("ur", "Urdu"), ("hr", "Croatian"), ("bg", "Bulgarian"), ("lt", "Lithuanian"), ("la", "Latin"),
    ("mi", "Maori"), ("ml", "Malayalam"), ("cy", "Welsh"), ("sk", "Slovak"), ("te", "Telugu"), ("fa", "Persian"),
    ("lv", "Latvian"), ("bn", "Bengali"), ("sr", "Serbian"), ("az", "Azerbaijani"), ("sl", "Slovenian"),
    ("kn", "Kannada"), ("et", "Estonian"), ("mk", "Macedonian"), ("br", "Breton"), ("eu", "Basque"),
    ("is", "Icelandic"), ("hy", "Armenian"), ("ne", "Nepali"), ("mn", "Mongolian"), ("bs", "Bosnian"),
    ("kk", "Kazakh"), ("sq", "Albanian"), ("sw", "Swahili"), ("gl", "Galician"), ("mr", "Marathi"), ("pa", "Punjabi"),
    ("si", "Sinhala"), ("km", "Khmer"), ("sn", "Shona"), ("yo", "Yoruba"), ("so", "Somali"), ("af", "Afrikaans"),
    ("oc", "Occitan"), ("ka", "Georgian"), ("be", "Belarusian"), ("tg", "Tajik"), ("sd", "Sindhi"),
    ("gu", "Gujarati"), ("am", "Amharic"), ("yi", "Yiddish"), ("lo", "Lao"), ("uz", "Uzbek"), ("fo", "Faroese"),
    ("ht", "Haitian Creole"), ("ps", "Pashto"), ("tk", "Turkmen"), ("nn", "Nynorsk"), ("mt", "Maltese"),
    ("sa", "Sanskrit"), ("lb", "Luxembourgish"), ("my", "Myanmar"), ("bo", "Tibetan"), ("tl", "Tagalog"),
    ("mg", "Malagasy"), ("as", "Assamese"), ("tt", "Tatar"), ("haw", "Hawaiian"), ("ln", "Lingala"), ("ha", "Hausa"),
    ("ba", "Bashkir"), ("jw", "Javanese"), ("su", "Sundanese"),
];

/// "Turkish" for "tr"; the code itself if unknown.
pub fn language_name(code: &str) -> &str {
    LANGUAGES.iter().find(|(c, _)| *c == code).map_or(code, |(_, name)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: &str = r#"{
      "model": { "type": "BPE", "vocab": { "Hello": 0, "Ġworld": 1, "Ã©": 2, "!": 3 } },
      "added_tokens": [ { "id": 4, "content": "<|endoftext|>" }, { "id": 5, "content": "<|en|>" },
                        { "id": 6, "content": "<|tr|>" } ]
    }"#;

    #[test]
    fn byte_level_tokens_decode_to_utf8_text() {
        let tokenizer = Tokenizer::from_json(SMALL.as_bytes()).unwrap();
        assert_eq!(tokenizer.decode(&[0, 1, 3]), "Hello world!");
        assert_eq!(tokenizer.decode(&[2]), "é", "two stand-in chars make one two-byte character");
    }

    #[test]
    fn special_tokens_are_found_by_name_and_skipped_in_text() {
        let tokenizer = Tokenizer::from_json(SMALL.as_bytes()).unwrap();
        assert_eq!(tokenizer.id("<|endoftext|>"), Some(4));
        assert_eq!(tokenizer.decode(&[0, 4, 5]), "Hello");
        assert_eq!(tokenizer.languages(), vec![("en".to_owned(), 5), ("tr".to_owned(), 6)]);
        assert_eq!(language_name("tr"), "Turkish");
    }

    #[test]
    fn a_broken_file_is_an_error() {
        assert!(Tokenizer::from_json(b"{}").is_err());
        assert!(Tokenizer::from_json(b"not json").is_err());
    }

    #[test]
    fn the_byte_table_covers_every_byte_once() {
        let table = byte_chars();
        assert_eq!(table.len(), 256);
        let mut chars: Vec<char> = table.iter().map(|(_, c)| *c).collect();
        chars.sort_unstable();
        chars.dedup();
        assert_eq!(chars.len(), 256);
        assert_eq!(table[32].1, 'Ġ', "space is written as Ġ");
    }
}
