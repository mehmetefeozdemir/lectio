//! Runs Whisper on one 30-second window: spectrogram → encoder → greedy
//! decoding with timestamp tokens → phrases with start and end times.
//!
//! *Greedy decoding* picks the most likely next token each step. Whisper's
//! timestamp rules (from OpenAI's reference implementation) keep the output
//! well-formed: the text starts with a timestamp, timestamps come in pairs
//! around each phrase, never go backwards, and a timestamp is forced when the
//! model is more sure about "a phrase ends here" than about any word.

use candle_core::{DType, Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::Config;
use candle_transformers::models::whisper::model::Whisper;
use lectio_core::transcript::{RawSegment, timestamp_ms};

use crate::mel::{MelSpectrogram, SAMPLE_RATE};
use crate::tokenizer::Tokenizer;

/// Most tokens decoded per window (Whisper's own limit is half its context).
const MAX_TOKENS: usize = 224;
/// The first timestamp may be at most 1 s into the window (50 × 20 ms).
const MAX_INITIAL_TIMESTAMP: u32 = 50;
/// A window counts as silence if "no speech" is this likely…
const NO_SPEECH_THRESHOLD: f32 = 0.6;
/// …and the decoded text this unsure (average log-probability).
const LOGPROB_THRESHOLD: f32 = -1.0;
/// Token for a leading space; suppressed as the very first token.
const SPACE_TOKEN: u32 = 220;

fn e(err: impl std::fmt::Display) -> String {
    format!("speech model: {err}")
}

struct Special {
    sot: u32,
    eot: u32,
    transcribe: u32,
    no_timestamps: u32,
    timestamp_begin: u32,
    no_speech: Option<u32>,
}

/// What one window produced.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowResult {
    pub segments: Vec<RawSegment>,
    pub language: String,
}

pub struct Engine {
    model: Whisper,
    mel: MelSpectrogram,
    tokenizer: Tokenizer,
    special: Special,
    languages: Vec<(String, u32)>,
    suppress: Vec<u32>,
    device: Device,
}

impl Engine {
    /// Loads a model from the three downloaded files.
    pub fn load(config_json: &[u8], tokenizer_json: &[u8], weights: Vec<u8>) -> Result<Self, String> {
        let config: Config = serde_json::from_slice(config_json).map_err(e)?;
        let tokenizer = Tokenizer::from_json(tokenizer_json)?;
        let id = |name: &str| tokenizer.id(name).ok_or_else(|| format!("speech model: the tokenizer has no {name}"));
        let special = Special {
            sot: id("<|startoftranscript|>")?,
            eot: id("<|endoftext|>")?,
            transcribe: id("<|transcribe|>")?,
            no_timestamps: id("<|notimestamps|>")?,
            timestamp_begin: id("<|0.00|>")?,
            no_speech: tokenizer.id("<|nospeech|>").or_else(|| tokenizer.id("<|nocaptions|>")),
        };
        let device = Device::Cpu;
        let vb = VarBuilder::from_buffered_safetensors(weights, DType::F32, &device).map_err(e)?;
        let mel = MelSpectrogram::new(config.num_mel_bins);
        let suppress = config.suppress_tokens.clone();
        let model = Whisper::load(&vb, config).map_err(e)?;
        let languages = tokenizer.languages();
        Ok(Self { model, mel, tokenizer, special, languages, suppress, device })
    }

    /// Transcribes up to 30 s of 16 kHz audio. `language` is a code like
    /// "en"; `None` detects it. On the last window an unfinished phrase is
    /// kept (ending at the end of the audio); otherwise it is left for the
    /// next window, which starts where the last finished phrase ended.
    pub fn transcribe_window(&mut self, pcm: &[f32], language: Option<&str>, last: bool) -> Result<WindowResult, String> {
        let mel = self.mel.compute(pcm);
        let n_mels = self.mel.n_mels();
        let mel = Tensor::from_vec(mel, (1, n_mels, crate::mel::FRAMES), &self.device).map_err(e)?;
        let features = self.model.encoder.forward(&mel, true).map_err(e)?;
        let language = match language.and_then(|l| self.language_id(l).map(|id| (l.to_owned(), id))) {
            Some(known) => known,
            None => self.detect_language(&features)?,
        };
        let (tokens, no_speech, avg_logprob) = self.decode(&features, language.1)?;
        let window_ms = pcm.len() as u64 * 1_000 / u64::from(SAMPLE_RATE);
        let silent = no_speech > NO_SPEECH_THRESHOLD && avg_logprob < LOGPROB_THRESHOLD;
        let segments = if silent { Vec::new() } else { self.segments(&tokens, last, window_ms) };
        Ok(WindowResult { segments, language: language.0 })
    }

    fn language_id(&self, code: &str) -> Option<u32> {
        self.languages.iter().find(|(c, _)| c == code).map(|(_, id)| *id)
    }

    fn logits(&mut self, tokens: &[u32], features: &Tensor, first: bool) -> Result<Vec<f32>, String> {
        let input = Tensor::new(tokens, &self.device).map_err(e)?.unsqueeze(0).map_err(e)?;
        let hidden = self.model.decoder.forward(&input, features, first).map_err(e)?;
        let (_, len, _) = hidden.dims3().map_err(e)?;
        let last = hidden.i((..1, len - 1..)).map_err(e)?;
        let logits = self.model.decoder.final_linear(&last).map_err(e)?;
        logits.i(0).and_then(|t| t.i(0)).and_then(|t| t.to_dtype(DType::F32)).and_then(|t| t.to_vec1()).map_err(e)
    }

    fn detect_language(&mut self, features: &Tensor) -> Result<(String, u32), String> {
        let logits = self.logits(&[self.special.sot], features, true)?;
        self.languages
            .iter()
            .max_by(|a, b| logits[a.1 as usize].total_cmp(&logits[b.1 as usize]))
            .cloned()
            .ok_or_else(|| "speech model: no languages".to_owned())
    }

    /// Greedy decoding. Returns the sampled tokens, the "no speech"
    /// probability at the first step, and the average log-probability.
    fn decode(&mut self, features: &Tensor, language: u32) -> Result<(Vec<u32>, f32, f32), String> {
        let mut tokens = vec![self.special.sot, language, self.special.transcribe];
        let begin = tokens.len();
        let mut no_speech = 0.0;
        let mut logprob_sum = 0.0;
        for step in 0..MAX_TOKENS {
            let mut logits = self.logits(&tokens, features, step == 0)?;
            if step == 0 {
                no_speech = self.special.no_speech.map_or(0.0, |id| softmax_at(&logits, id as usize));
                for id in [SPACE_TOKEN, self.special.eot] {
                    suppress(&mut logits, id);
                }
            }
            for &id in &self.suppress {
                suppress(&mut logits, id);
            }
            self.apply_timestamp_rules(&mut logits, &tokens[begin..]);
            let next = argmax(&logits);
            logprob_sum += log_softmax_at(&logits, next as usize);
            if next == self.special.eot {
                break;
            }
            tokens.push(next);
        }
        let sampled = tokens.split_off(begin);
        let avg = logprob_sum / (sampled.len() + 1) as f32;
        Ok((sampled, no_speech, avg))
    }

    fn apply_timestamp_rules(&self, logits: &mut [f32], sampled: &[u32]) {
        let ts = self.special.timestamp_begin as usize;
        let eot = self.special.eot as usize;
        suppress(logits, self.special.no_timestamps);
        let is_ts = |t: &u32| *t >= self.special.timestamp_begin;
        let last_ts = sampled.last().is_some_and(is_ts);
        let penultimate_ts = sampled.len() < 2 || is_ts(&sampled[sampled.len() - 2]);
        if last_ts {
            let range = if penultimate_ts { ts..logits.len() } else { 0..eot };
            logits[range].fill(f32::NEG_INFINITY);
        }
        if let Some(&previous) = sampled.iter().rev().find(|t| is_ts(t)) {
            let floor = if last_ts && !penultimate_ts { previous } else { previous + 1 };
            let end = (floor as usize).min(logits.len());
            logits[ts..end].fill(f32::NEG_INFINITY);
        }
        if sampled.is_empty() {
            logits[..ts].fill(f32::NEG_INFINITY);
            let latest = ts + MAX_INITIAL_TIMESTAMP as usize + 1;
            if latest < logits.len() {
                logits[latest..].fill(f32::NEG_INFINITY);
            }
        }
        // If a timestamp is likelier than any single word, it must be a timestamp.
        let max_all = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let ts_mass: f32 = logits[ts..].iter().map(|&l| (l - max_all).exp()).sum();
        let best_text = logits[..ts].iter().copied().fold(f32::NEG_INFINITY, f32::max);
        if ts_mass.ln() + max_all > best_text {
            logits[..ts].fill(f32::NEG_INFINITY);
        }
    }

    /// Phrases between pairs of timestamp tokens.
    fn segments(&self, tokens: &[u32], last: bool, window_ms: u64) -> Vec<RawSegment> {
        let begin = self.special.timestamp_begin;
        let mut out = Vec::new();
        let mut start: Option<u64> = None;
        let mut text: Vec<u32> = Vec::new();
        for &token in tokens {
            if token >= begin {
                let at = timestamp_ms(token - begin);
                match start {
                    Some(from) if !text.is_empty() => {
                        out.push(RawSegment { start_ms: from, end_ms: at, text: self.tokenizer.decode(&text) });
                        text.clear();
                        start = None;
                    }
                    _ => start = Some(at),
                }
            } else if token < self.special.eot {
                text.push(token);
            }
        }
        if let (true, Some(from), false) = (last, start, text.is_empty()) {
            out.push(RawSegment { start_ms: from, end_ms: window_ms.max(from), text: self.tokenizer.decode(&text) });
        }
        out
    }
}

fn suppress(logits: &mut [f32], id: u32) {
    if let Some(l) = logits.get_mut(id as usize) {
        *l = f32::NEG_INFINITY;
    }
}

fn argmax(logits: &[f32]) -> u32 {
    logits.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map_or(0, |(i, _)| i as u32)
}

fn log_softmax_at(logits: &[f32], index: usize) -> f32 {
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let sum: f32 = logits.iter().map(|&l| (l - max).exp()).sum();
    logits[index] - max - sum.ln()
}

fn softmax_at(logits: &[f32], index: usize) -> f32 {
    log_softmax_at(logits, index).exp()
}
