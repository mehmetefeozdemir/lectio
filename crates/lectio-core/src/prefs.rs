//! The user's layout, theme, playback and speech choices, saved with the
//! library so they are remembered on every platform (ADR-015, ADR-016).

use serde::{Deserialize, Serialize};

use crate::dsp::FilterSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    /// Follow the operating system's light/dark setting.
    #[default]
    System,
    Light,
    Dark,
}

/// Which Whisper model transcribes (ADR-016). Bigger is more accurate and slower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechModel {
    #[default]
    Tiny,
    Base,
    Small,
}

impl SpeechModel {
    pub const ALL: [SpeechModel; 3] = [SpeechModel::Tiny, SpeechModel::Base, SpeechModel::Small];

    pub fn name(self) -> &'static str {
        match self {
            Self::Tiny => "tiny",
            Self::Base => "base",
            Self::Small => "small",
        }
    }
}

/// Panels, theme, playback and speech settings. Older files have none of
/// the newer fields: they get the defaults below.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub show_slides: bool,
    pub show_feed: bool,
    pub show_audio: bool,
    pub theme: ThemeChoice,
    /// Playback speed in percent (50–200; see `stretch`).
    pub speed_percent: u16,
    /// Filters applied while playing (and baked into a cleaned copy).
    pub filters: FilterSettings,
    /// Play the cleaned copy instead of the original, when there is one.
    pub play_cleaned: bool,
    /// Live captions while recording.
    pub captions: bool,
    pub speech_model: SpeechModel,
    /// Spoken language as a Whisper code ("en", "tr", …); `None` = detect.
    pub language: Option<String>,
    /// The right panel shows the transcript instead of the audio stream.
    pub show_transcript_tab: bool,
    /// The right panel shows the outline (wins over the transcript tab).
    pub show_outline_tab: bool,
    /// Which AI provider the study tools use (ADR-021). The key for an
    /// OpenAI-compatible endpoint is never stored here.
    pub ai: AiProvider,
    /// Look words up online when the offline dictionary has nothing (ADR-022).
    pub online_dictionary: bool,
    /// What the Home list shows, and in which order (Phase 4b, ADR-026).
    pub home_filter: crate::collections::HomeFilter,
    pub home_sort: crate::collections::SessionSort,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            show_slides: true,
            show_feed: true,
            show_audio: true,
            theme: ThemeChoice::System,
            speed_percent: 100,
            filters: FilterSettings::default(),
            play_cleaned: true,
            captions: false,
            speech_model: SpeechModel::Tiny,
            language: None,
            show_transcript_tab: false,
            show_outline_tab: false,
            ai: AiProvider::None,
            online_dictionary: false,
            home_filter: crate::collections::HomeFilter::default(),
            home_sort: crate::collections::SessionSort::default(),
        }
    }
}

/// The local study models (ADR-021).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalModel {
    /// Qwen2.5-0.5B-Instruct: every target.
    #[default]
    Small,
    /// Qwen2.5-1.5B-Instruct: desktop only, better answers.
    Large,
}

impl LocalModel {
    pub const ALL: [LocalModel; 2] = [LocalModel::Small, LocalModel::Large];
}

/// Where AI study tools get their answers. `None` hides them (ADR-021).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AiProvider {
    #[default]
    None,
    /// A model run by Lectio itself, downloaded on first use.
    Local { model: LocalModel },
    /// Ollama on this computer (`http://localhost:11434`).
    Ollama { model: String },
    /// Any OpenAI-compatible chat endpoint; the key is kept elsewhere.
    OpenAi { base_url: String, model: String },
}

impl AiProvider {
    pub fn is_set(&self) -> bool {
        *self != AiProvider::None
    }
}
