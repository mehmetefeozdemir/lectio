//! Generates a reply with Qwen2.5-Instruct: build the chat prompt, encode it,
//! run the model one token at a time (keeping its key/value cache, so each
//! new token only costs one step), and stop at the end-of-turn token.
//!
//! Decoding is greedy (always the most likely token) with a mild repeat
//! penalty: study tools want the same answer for the same notes, and small
//! models otherwise tend to loop.

use std::io::Cursor;

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::{LogitsProcessor, Sampling};
use candle_transformers::models::quantized_qwen2::ModelWeights;
use candle_transformers::utils::apply_repeat_penalty;
use tokenizers::Tokenizer;

const REPEAT_PENALTY: f32 = 1.1;
/// How many recent tokens the repeat penalty looks at.
const REPEAT_WINDOW: usize = 64;
/// Prompts longer than this are refused (the chunker keeps them far below).
pub const MAX_PROMPT_TOKENS: usize = 8_000;

fn e(err: impl std::fmt::Display) -> String {
    format!("study model: {err}")
}

pub struct LlmEngine {
    model: ModelWeights,
    tokenizer: Tokenizer,
    stop: Vec<u32>,
    device: Device,
}

/// Speed of the last reply.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    pub prompt_tokens: usize,
    pub generated_tokens: usize,
    pub prompt_seconds: f64,
    pub generate_seconds: f64,
}

/// Qwen's chat format.
pub fn chat_prompt(system: &str, user: &str) -> String {
    format!("<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n")
}

impl LlmEngine {
    pub fn load(gguf: Vec<u8>, tokenizer_json: &[u8]) -> Result<Self, String> {
        let device = Device::Cpu;
        let mut reader = Cursor::new(gguf);
        let content = gguf_file::Content::read(&mut reader).map_err(e)?;
        let model = ModelWeights::from_gguf(content, &mut reader, &device).map_err(e)?;
        let tokenizer = Tokenizer::from_bytes(tokenizer_json).map_err(e)?;
        let stop = ["<|im_end|>", "<|endoftext|>"].iter().filter_map(|t| tokenizer.token_to_id(t)).collect();
        Ok(Self { model, tokenizer, stop, device })
    }

    /// The model's reply to `system` + `user`, at most `max_tokens` long.
    /// `keep_going` is asked after each token (false = stop early).
    pub fn generate(
        &mut self,
        system: &str,
        user: &str,
        max_tokens: usize,
        mut keep_going: impl FnMut(usize) -> bool,
    ) -> Result<(String, Stats), String> {
        let clock = Clock::start();
        let prompt = self.tokenizer.encode(chat_prompt(system, user), false).map_err(e)?.get_ids().to_vec();
        if prompt.len() > MAX_PROMPT_TOKENS {
            return Err(format!("study model: the request is too long ({} tokens).", prompt.len()));
        }
        self.model.clear_kv_cache();
        let mut sampler = LogitsProcessor::from_sampling(0, Sampling::ArgMax);
        let mut context = prompt.clone();
        let mut generated: Vec<u32> = Vec::new();
        let input = Tensor::new(prompt.as_slice(), &self.device).and_then(|t| t.unsqueeze(0)).map_err(e)?;
        let mut logits = self.model.forward(&input, 0).and_then(|l| l.squeeze(0)).map_err(e)?;
        let prompt_seconds = clock.seconds();
        while generated.len() < max_tokens {
            let recent = &context[context.len().saturating_sub(REPEAT_WINDOW)..];
            let penalised = apply_repeat_penalty(&logits, REPEAT_PENALTY, recent).map_err(e)?;
            let next = sampler.sample(&penalised).map_err(e)?;
            if self.stop.contains(&next) || !keep_going(generated.len()) {
                break;
            }
            generated.push(next);
            context.push(next);
            let input = Tensor::new(&[next], &self.device).and_then(|t| t.unsqueeze(0)).map_err(e)?;
            logits = self.model.forward(&input, context.len() - 1).and_then(|l| l.squeeze(0)).map_err(e)?;
        }
        let text = self.tokenizer.decode(&generated, true).map_err(e)?;
        let stats = Stats {
            prompt_tokens: prompt.len(),
            generated_tokens: generated.len(),
            prompt_seconds,
            generate_seconds: clock.seconds() - prompt_seconds,
        };
        Ok((text, stats))
    }
}

/// Wall-clock time where there is one (the browser worker has no
/// `std::time::Instant`; there the stats stay at zero).
struct Clock {
    #[cfg(not(target_arch = "wasm32"))]
    started: std::time::Instant,
}

impl Clock {
    fn start() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            started: std::time::Instant::now(),
        }
    }

    fn seconds(&self) -> f64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.started.elapsed().as_secs_f64()
        }
        #[cfg(target_arch = "wasm32")]
        {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chat_prompt_uses_qwens_turn_markers() {
        let prompt = chat_prompt("Be brief.", "Outline this.");
        assert!(prompt.starts_with("<|im_start|>system\nBe brief.<|im_end|>"));
        assert!(prompt.ends_with("<|im_start|>assistant\n"));
    }
}
