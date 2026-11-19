//! The user's layout and theme choices, saved with the library so they are
//! remembered on every platform (ADR-015).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    /// Follow the operating system's light/dark setting.
    #[default]
    System,
    Light,
    Dark,
}

/// Which workspace panels are shown. Older files have none of this: defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub show_slides: bool,
    pub show_feed: bool,
    pub show_audio: bool,
    pub theme: ThemeChoice,
}

impl Default for Preferences {
    fn default() -> Self {
        Self { show_slides: true, show_feed: true, show_audio: true, theme: ThemeChoice::System }
    }
}
