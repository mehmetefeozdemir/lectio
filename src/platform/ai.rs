//! The AI provider layer at the platform boundary (ADR-021). The app sends
//! requests and polls for events each frame. The desktop serves them on a
//! background thread; the browser runs the local model in a Web Worker and
//! calls Ollama or an endpoint with `fetch`.

use lectio_core::prefs::{AiProvider, LocalModel};

#[derive(Clone)]
pub enum AiRequest {
    /// Local: download (if needed) and load the model. Ollama: list models.
    Prepare { provider: AiProvider },
    /// Is Ollama running on this computer, and which models has it?
    ProbeOllama,
    /// Answer `system` + `user` with at most `max_tokens` tokens.
    Complete { id: u64, provider: AiProvider, key: Option<String>, system: String, user: String, max_tokens: usize },
}

/// Never prints the key.
impl std::fmt::Debug for AiRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Prepare { provider } => write!(f, "Prepare({provider:?})"),
            Self::ProbeOllama => write!(f, "ProbeOllama"),
            Self::Complete { id, provider, max_tokens, .. } => {
                write!(f, "Complete(#{id}, {provider:?}, {max_tokens} tokens, key hidden)")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AiEvent {
    Downloading { model: LocalModel, done: u64, total: u64 },
    /// The local model is loaded.
    Ready { model: LocalModel },
    /// `None` = Ollama isn't running here.
    Ollama(Option<Vec<String>>),
    Reply { id: u64, text: String },
    /// How long the local model took for its last answer (prompt and reply tokens).
    Timing { tokens: usize, seconds: f64 },
    Failed { id: Option<u64>, message: String },
}

/// The study model behind every AI feature.
pub trait StudyModel {
    fn send(&self, request: AiRequest);
    fn poll(&self) -> Vec<AiEvent>;
    fn is_downloaded(&self, model: LocalModel) -> bool;
    /// Why a local model can't be used here (e.g. too big for a browser tab).
    fn unavailable(&self, model: LocalModel) -> Option<&'static str>;
}

/// Ollama's address; it only listens on this computer by default.
pub const OLLAMA_URL: &str = "http://127.0.0.1:11434";

/// The JSON for Ollama's `/api/chat` (no streaming, deterministic).
pub fn ollama_body(model: &str, system: &str, user: &str, max_tokens: usize) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        "stream": false,
        "options": {"temperature": 0, "num_predict": max_tokens},
    })
}

/// The JSON for an OpenAI-compatible `/chat/completions`.
pub fn openai_body(model: &str, system: &str, user: &str, max_tokens: usize) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        "max_tokens": max_tokens,
        "temperature": 0,
    })
}

/// `https://host/v1/` → `https://host/v1/chat/completions`.
pub fn openai_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim().trim_end_matches('/'))
}

/// The reply text in either provider's answer.
pub fn reply_text(answer: &serde_json::Value) -> Option<String> {
    answer["message"]["content"]
        .as_str()
        .or_else(|| answer["choices"][0]["message"]["content"].as_str())
        .map(str::to_owned)
}

/// `https://example.org/v1/x` → `example.org`. Network errors name only
/// the host: never the path, the body or the key.
pub fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split('/').next().unwrap_or(rest)
}

/// Model names from Ollama's `/api/tags`.
pub fn ollama_models(answer: &serde_json::Value) -> Vec<String> {
    answer["models"].as_array().into_iter().flatten().filter_map(|m| m["name"].as_str().map(str::to_owned)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_answer_shapes_give_the_reply() {
        let ollama = serde_json::json!({"message": {"role": "assistant", "content": "- [S1] Graphs"}});
        let openai = serde_json::json!({"choices": [{"message": {"content": "Q: x"}}]});
        assert_eq!(reply_text(&ollama).as_deref(), Some("- [S1] Graphs"));
        assert_eq!(reply_text(&openai).as_deref(), Some("Q: x"));
        assert_eq!(reply_text(&serde_json::json!({})), None);
    }

    #[test]
    fn endpoint_urls_and_bodies_are_built_without_the_key() {
        assert_eq!(openai_url("https://example.org/v1/"), "https://example.org/v1/chat/completions");
        let body = openai_body("m", "s", "u", 50).to_string();
        assert!(body.contains("\"max_tokens\":50") && !body.to_lowercase().contains("key"));
        let request = AiRequest::Complete {
            id: 1,
            provider: AiProvider::None,
            key: Some("sk-secret".into()),
            system: String::new(),
            user: String::new(),
            max_tokens: 1,
        };
        assert!(!format!("{request:?}").contains("sk-secret"));
    }

    #[test]
    fn errors_name_only_the_host() {
        assert_eq!(host("https://example.org/v1/chat/completions"), "example.org");
        assert_eq!(host("http://127.0.0.1:11434/api/tags"), "127.0.0.1:11434");
        assert_eq!(host("example.org"), "example.org");
    }

    #[test]
    fn ollama_lists_its_models() {
        let tags = serde_json::json!({"models": [{"name": "llama3.2:3b"}, {"name": "qwen2.5:1.5b"}]});
        assert_eq!(ollama_models(&tags), vec!["llama3.2:3b", "qwen2.5:1.5b"]);
    }
}
