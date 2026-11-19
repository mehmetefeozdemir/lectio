//! UI-only state on `LectioApp` (ADR-014): undo for deletes, the level-meter
//! history, panel visibility overrides, and the theme choice.
//!
//! Undo works because the library is immutable: the value from before the
//! delete is kept for a few seconds. Files (blobs) the delete orphaned are
//! only removed once undo is no longer possible.

use std::collections::VecDeque;

use eframe::egui::ThemePreference;
use lectio_core::DeckId;

use super::*;

/// How long "Undo" stays available after a delete.
const UNDO_SECONDS: f64 = 8.0;
/// Level-meter bars kept (one per frame while recording).
const LEVEL_HISTORY: usize = 64;

pub struct Undo {
    pub before: Library,
    pub after: Library,
    pub message: &'static str,
    pub expires_at: Option<f64>,
}

#[derive(Default)]
pub struct UiState {
    pub undo: Option<Undo>,
    pub levels: VecDeque<f32>,
    /// `None` = automatic (by window width); `Some` = the user's choice.
    pub sidebar: Option<bool>,
    pub theme: Option<ThemePreference>,
    pub theme_request: Option<ThemePreference>,
    pub screen: crate::action::Screen,
    pub narrow_tab: crate::action::Panel,
    /// Slide zoom (1.0 = fit width) and the full-screen slide view.
    pub slide_zoom: f32,
    pub fullscreen: bool,
    /// When the microphone last heard something while recording (seconds).
    pub last_sound_at: Option<f64>,
    pub quiet_for_s: f64,
    /// Text to put on the clipboard on the next frame ("Copy all").
    pub clipboard: Option<String>,
}

/// Below this level the microphone counts as "not hearing anything".
const SILENCE_LEVEL: f32 = 0.02;

impl LectioApp {
    pub(super) fn apply_ui(&mut self, action: Action) {
        match action {
            Action::RenameSession(session, title) => match self.library.with_session_renamed(session, &title) {
                Ok(library) => self.library = library,
                Err(e) => self.show_error(e),
            },
            Action::Undo => self.undo(),
            Action::DismissUndo => self.finalize_undo(),
            Action::SetSidebar(open) => self.ui_state.sidebar = Some(open),
            other => self.apply_workflow(other),
        }
    }

    pub(super) fn delete_note_with_undo(&mut self, note: NoteId) {
        self.with_undo("Note deleted", |app| app.change_session(|lib, s| lib.without_note(s, note)));
    }

    pub(super) fn remove_deck_with_undo(&mut self, deck: DeckId) {
        self.with_undo("Slides removed", |app| {
            app.change_session(|lib, s| lib.without_deck(s, deck));
            app.material.forget_deck(deck);
        });
    }

    fn with_undo(&mut self, message: &'static str, change: impl FnOnce(&mut Self)) {
        self.finalize_undo();
        let before = self.library.clone();
        change(self);
        if self.library != before {
            let after = self.library.clone();
            self.ui_state.undo = Some(Undo { before, after, message, expires_at: None });
        }
    }

    fn undo(&mut self) {
        let Some(undo) = self.ui_state.undo.take() else { return };
        if self.library == undo.after {
            self.library = undo.before;
        }
    }

    /// Undo is no longer offered: now delete the files nothing refers to.
    pub(super) fn finalize_undo(&mut self) {
        if let Some(undo) = self.ui_state.undo.take() {
            self.delete_orphaned_blobs(&undo.before);
        }
    }

    /// Any other change after a delete ends the undo window (restoring the
    /// old library would silently drop that change).
    pub(super) fn end_undo_if_library_moved_on(&mut self) {
        let moved_on = self.ui_state.undo.as_ref().is_some_and(|u| u.after != self.library);
        if moved_on {
            self.finalize_undo();
        }
    }

    pub(super) fn tick_ui_state(&mut self, now: f64) {
        if let Some(undo) = &mut self.ui_state.undo {
            let expires = *undo.expires_at.get_or_insert(now + UNDO_SECONDS);
            if now >= expires {
                self.finalize_undo();
            }
        }
        let level = match &self.capture {
            Some(Capture { recorder: Some(recorder), clock, .. }) if clock.phase() == RecorderPhase::Recording => {
                Some(recorder.level())
            }
            _ => None,
        };
        if let Some(level) = level {
            let heard = *self.ui_state.last_sound_at.get_or_insert(now);
            if level > SILENCE_LEVEL {
                self.ui_state.last_sound_at = Some(now);
            }
            self.ui_state.quiet_for_s = now - self.ui_state.last_sound_at.unwrap_or(heard);
            self.ui_state.levels.push_back(level);
            while self.ui_state.levels.len() > LEVEL_HISTORY {
                self.ui_state.levels.pop_front();
            }
        } else if !self.is_recording_active() {
            self.ui_state.levels.clear();
            self.ui_state.last_sound_at = None;
            self.ui_state.quiet_for_s = 0.0;
        }
    }
}
