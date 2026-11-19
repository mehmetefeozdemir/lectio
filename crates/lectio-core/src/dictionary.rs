//! The offline dictionary (Phase 4a, ADR-022): Open English WordNet turned
//! into a compact word → senses index, and look-ups that also try simple
//! base forms ("queues" → "queue", "studies" → "study").
//!
//! The download is JSON Lines (one *synset*, a group of words sharing one
//! meaning, per line). It is converted once into tab-separated lines
//! `word \t part of speech \t definition \t example`, which load quickly.

use std::collections::HashMap;

use serde_json::Value;

/// Shown with every offline result (CC BY 4.0 asks for attribution).
pub const ATTRIBUTION: &str = "Open English WordNet (CC BY 4.0)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sense {
    pub pos: String,
    pub definition: String,
    pub example: String,
}

fn part_of_speech(code: &str) -> &'static str {
    match code {
        "n" => "noun",
        "v" => "verb",
        "a" | "s" => "adjective",
        "r" => "adverb",
        _ => "",
    }
}

/// `oewn-o-ap-clock-r` → `o'clock` (WordNet ids spell some characters out).
fn member_word(member: &str) -> Option<String> {
    let id = member.strip_prefix("oewn-")?;
    let (word, _pos) = id.rsplit_once('-')?;
    let mut word = word.to_owned();
    for (code, ch) in [("-ap-", "'"), ("-cm-", ","), ("-cl-", ":"), ("-ex-", "!"), ("-lb-", "("), ("-rb-", ")"), ("-pl-", "+"), ("-sl-", "/"), ("-fs-", ".")] {
        word = word.replace(code, ch);
    }
    Some(word.replace('_', " "))
}

/// The first text of a field that may be a string or a list of strings.
fn first_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().find_map(Value::as_str).unwrap_or("").to_owned(),
        _ => String::new(),
    }
}

/// The (word, sense) pairs of one WordNet JSON line; nothing if it can't be read.
pub fn parse_oewn_line(line: &str) -> Vec<(String, Sense)> {
    let Ok(value) = serde_json::from_str::<Value>(line) else { return Vec::new() };
    let definition = first_text(&value["Definition"]);
    if definition.is_empty() {
        return Vec::new();
    }
    let sense = Sense {
        pos: part_of_speech(value["@partOfSpeech"].as_str().unwrap_or("")).to_owned(),
        definition,
        example: first_text(&value["Example"]),
    };
    let members = value["@members"].as_str().unwrap_or("");
    members.split_whitespace().filter_map(member_word).map(|w| (w, sense.clone())).collect()
}

fn clean(field: &str) -> String {
    field.replace(['\t', '\n', '\r'], " ")
}

/// The compact form: one `word \t pos \t definition \t example` line per sense.
pub fn to_tsv(entries: &[(String, Sense)]) -> String {
    let mut out = String::new();
    for (word, sense) in entries {
        out.push_str(&format!("{}\t{}\t{}\t{}\n", clean(word), clean(&sense.pos), clean(&sense.definition), clean(&sense.example)));
    }
    out
}

/// The whole Open English WordNet JSON Lines download as index text.
pub fn index_from_jsonl(jsonl: &str) -> String {
    let entries: Vec<(String, Sense)> = jsonl.lines().flat_map(parse_oewn_line).collect();
    to_tsv(&entries)
}

/// The word itself, then forms without common endings (most likely first).
pub fn base_forms(word: &str) -> Vec<String> {
    let w = word.trim().trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    let w = w.strip_suffix("'s").map_or(w.clone(), str::to_owned);
    let mut forms = vec![w.clone()];
    let mut add = |stem: Option<&str>, tail: &str| {
        if let Some(stem) = stem.filter(|s| s.len() >= 2) {
            forms.push(format!("{stem}{tail}"));
        }
    };
    add(w.strip_suffix("ies"), "y");
    add(w.strip_suffix("es"), "");
    add(w.strip_suffix('s'), "");
    add(w.strip_suffix("ing"), "");
    add(w.strip_suffix("ing"), "e");
    add(w.strip_suffix("ed"), "");
    add(w.strip_suffix('d'), "");
    add(w.strip_suffix("ied"), "y");
    let mut unique = Vec::new();
    for f in forms {
        if !unique.contains(&f) {
            unique.push(f);
        }
    }
    unique
}

/// An online dictionary (ADR-022): where to ask, and how to read the answer.
/// The platform does the HTTP request; this part is pure, so it is tested.
pub trait DefinitionProvider {
    /// Shown with its results.
    fn name(&self) -> &'static str;
    fn url(&self, word: &str) -> String;
    fn parse(&self, body: &str) -> Vec<Sense>;
}

/// dictionaryapi.dev: free, no key, English.
pub struct FreeDictionary;

impl DefinitionProvider for FreeDictionary {
    fn name(&self) -> &'static str {
        "dictionaryapi.dev (online)"
    }

    fn url(&self, word: &str) -> String {
        let word: String = base_forms(word).into_iter().next().unwrap_or_default();
        let safe: String = word.chars().filter(|c| c.is_alphanumeric() || *c == '-').collect();
        format!("https://api.dictionaryapi.dev/api/v2/entries/en/{safe}")
    }

    fn parse(&self, body: &str) -> Vec<Sense> {
        let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(body) else { return Vec::new() };
        let mut senses = Vec::new();
        for meaning in entries.iter().flat_map(|e| e["meanings"].as_array().into_iter().flatten()) {
            let pos = meaning["partOfSpeech"].as_str().unwrap_or("").to_owned();
            for definition in meaning["definitions"].as_array().into_iter().flatten() {
                let Some(text) = definition["definition"].as_str() else { continue };
                let example = definition["example"].as_str().unwrap_or("").to_owned();
                senses.push(Sense { pos: pos.clone(), definition: text.to_owned(), example });
            }
        }
        senses
    }
}

/// Word → senses, built from the compact form.
#[derive(Debug, Default)]
pub struct DictIndex {
    senses: HashMap<String, Vec<Sense>>,
}

impl DictIndex {
    pub fn from_tsv(tsv: &str) -> Self {
        let mut senses: HashMap<String, Vec<Sense>> = HashMap::new();
        for line in tsv.lines() {
            let mut fields = line.split('\t');
            let (Some(word), Some(pos), Some(definition)) = (fields.next(), fields.next(), fields.next()) else { continue };
            let example = fields.next().unwrap_or("").to_owned();
            let sense = Sense { pos: pos.to_owned(), definition: definition.to_owned(), example };
            senses.entry(word.to_lowercase()).or_default().push(sense);
        }
        Self { senses }
    }

    pub fn len(&self) -> usize {
        self.senses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.senses.is_empty()
    }

    /// Senses of the first base form that is in the dictionary.
    pub fn lookup(&self, word: &str) -> Vec<Sense> {
        base_forms(word).iter().find_map(|form| self.senses.get(form)).cloned().unwrap_or_default()
    }
}
