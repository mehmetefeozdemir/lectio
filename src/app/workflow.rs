//! Session workflow on `LectioApp` (ADR-015): screens, trash, posting slides
//! to the feed, saved layout/theme preferences, and Markdown copy/export.

use eframe::egui::ThemePreference;
use lectio_core::material::{SlideLink, orphaned_blobs};
use lectio_core::prefs::{Preferences, ThemeChoice};

use super::*;
use crate::action::{Panel, Screen};

/// Zoom range for the slide panel (1.0 = fit width).
const MIN_ZOOM: f32 = 1.0;
const MAX_ZOOM: f32 = 3.0;

pub fn theme_from(choice: ThemeChoice) -> ThemePreference {
    match choice {
        ThemeChoice::System => ThemePreference::System,
        ThemeChoice::Light => ThemePreference::Light,
        ThemeChoice::Dark => ThemePreference::Dark,
    }
}

fn choice_from(theme: ThemePreference) -> ThemeChoice {
    match theme {
        ThemePreference::System => ThemeChoice::System,
        ThemePreference::Light => ThemeChoice::Light,
        ThemePreference::Dark => ThemeChoice::Dark,
    }
}

impl LectioApp {
    pub(super) fn apply_workflow(&mut self, action: Action) {
        match action {
            Action::GoTo(Screen::Workspace) => {}
            Action::GoTo(_) if self.is_recording_active() => {
                self.show_error("Stop the recording before leaving the session.");
            }
            Action::GoTo(screen) => {
                self.ui_state.screen = screen;
                self.material.reading = false;
                if screen == Screen::Search {
                    // Results reflect edits made since the last search.
                    self.run_search(self.speech.query.clone());
                }
            }
            Action::TrashSession(session) => self.trash_session(session),
            Action::RestoreSession(session) => {
                if let Ok(library) = self.library.with_session_restored(session) {
                    self.library = library;
                }
            }
            Action::PostSlide { next } => self.post_slide(next),
            Action::SetPanel(panel, shown) => self.set_panel(panel, shown),
            Action::ShowTab(panel) => self.ui_state.narrow_tab = panel,
            Action::EditNoteText(note, text) => self.change_session(|lib, s| lib.with_note_text(s, note, &text)),
            Action::CopyAll => self.ui_state.clipboard = self.session_markdown(),
            Action::ExportMarkdown => self.export_markdown(),
            Action::SlideZoom(zoom) => self.ui_state.slide_zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            Action::SetFullscreen(on) => self.ui_state.fullscreen = on,
            Action::SetTheme(theme) => {
                self.ui_state.theme_request = Some(theme);
                let prefs = Preferences { theme: choice_from(theme), ..self.library.preferences().clone() };
                self.library = self.library.with_preferences(prefs);
            }
            other => {
                let rest = self.apply_speech(other).and_then(|a| self.apply_audio(a));
                if let Some(action) = rest {
                    self.apply_material(action);
                }
            }
        }
    }

    fn set_panel(&mut self, panel: Panel, shown: bool) {
        let mut prefs = self.library.preferences().clone();
        match panel {
            Panel::Slides => prefs.show_slides = shown,
            Panel::Feed => prefs.show_feed = shown,
            Panel::Audio => prefs.show_audio = shown,
        }
        self.library = self.library.with_preferences(prefs);
    }

    fn trash_session(&mut self, session: SessionId) {
        if self.selected == Some(session) && self.is_recording_active() {
            return self.show_error("Stop the recording before moving this session to the trash.");
        }
        match self.library.with_session_trashed(session, now_unix_ms()) {
            Ok(library) => {
                self.library = library;
                if self.selected == Some(session) {
                    self.selected = None;
                    self.capture = None;
                    self.playback = None;
                    self.ui_state.screen = Screen::Home;
                }
            }
            Err(e) => self.show_error(e),
        }
    }

    /// Sessions trashed 30+ days ago are removed for good, with their files.
    pub(super) fn purge_expired_trash(&mut self) {
        let (library, removed) = self.library.without_expired_trash(now_unix_ms());
        if removed.is_empty() {
            return;
        }
        let before = std::mem::replace(&mut self.library, library);
        self.persist();
        let Some(platform) = self.platform.as_ref().filter(|_| self.can_save) else { return };
        let blob_problems = orphaned_blobs(&before, &self.library).into_iter().filter_map(|key| platform.delete_blob(&key).err());
        let audio_problems = removed
            .iter()
            .filter_map(|s| s.recording.as_ref())
            .filter_map(|r| platform.delete_recording(&r.file_name).err());
        let problems: Vec<String> = blob_problems.chain(audio_problems).collect();
        if let Some(problem) = problems.first() {
            self.show_error(format!("Some old files in the trash could not be removed: {problem}"));
        }
    }

    /// Posts the next slide (moving the viewer there) or the shown slide.
    fn post_slide(&mut self, next: bool) {
        let Some(session) = self.selected else { return };
        if !self.can_take_notes() {
            return self.show_error("Start recording (or open a recorded session) to post slides to the feed.");
        }
        let shown = self.library.session(session).and_then(|s| self.material.shown_slide(s));
        let Some(deck) = shown.map(|l| l.deck) else {
            return self.show_error("Import the lecture's slides first.");
        };
        let count = self
            .library
            .session(session)
            .and_then(|s| s.decks.iter().find(|d| d.id == deck))
            .map_or(1, |d| d.page_count);
        let page = if next { (self.material.viewer.page + 1).min(count) } else { self.material.viewer.page };
        self.material.viewer.page = page;
        let offset = self.current_offset_ms();
        match self.library.with_slide_item(session, offset, SlideLink { deck, page }) {
            Ok((library, _)) => self.library = library,
            Err(e) => self.show_error(e),
        }
    }

    /// When recording starts with slides loaded, the shown slide is posted
    /// to the feed at 0:00 (unless the feed already has slide items).
    pub(super) fn after_recording_started(&mut self) {
        let Some(session) = self.selected.and_then(|id| self.library.session(id)) else { return };
        let has_slide_items = session.notes.iter().any(|n| n.kind == lectio_core::NoteKind::Slide);
        if let (false, Some(link)) = (has_slide_items, self.material.shown_slide(session))
            && let Ok((library, _)) = self.library.with_slide_item(session.id, 0, link)
        {
            self.library = library;
        }
    }

    fn session_markdown(&self) -> Option<String> {
        self.selected.and_then(|id| lectio_core::export::session_markdown(&self.library, id).ok())
    }

    fn export_markdown(&mut self) {
        let text = self.session_markdown();
        let title = self.selected.and_then(|id| self.library.session(id)).map(|s| s.title.clone());
        let (Some(text), Some(title)) = (text, title) else { return };
        let name = format!("{}.md", title.replace(['/', '\\', ':'], "-"));
        let Some(platform) = self.platform.as_ref() else {
            return self.show_error("Exporting is turned off in this window. Use \"Copy all\" instead.");
        };
        if let Err(e) = platform.save_text_file(&name, &text) {
            self.show_error(e);
        }
    }

    /// Shows only the named panels (used for screenshots).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn with_panels(mut self, panels: &str) -> Self {
        let shown = |name: &str| panels.split(',').any(|p| p.trim() == name);
        let prefs = Preferences {
            show_slides: shown("slides"),
            show_feed: shown("feed"),
            show_audio: shown("audio") || shown("transcript"),
            show_transcript_tab: shown("transcript"),
            ..self.library.preferences().clone()
        };
        self.library = self.library.with_preferences(prefs);
        self
    }

    /// Opens the Search page with `query` already typed (used for screenshots).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn with_search(mut self, query: &str) -> Self {
        self.ui_state.screen = Screen::Search;
        self.buffers.search = query.to_owned();
        self.run_search(query.to_owned());
        self
    }

    /// Opens on another screen (used for screenshots).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn with_screen(mut self, screen: Screen) -> Self {
        self.ui_state.screen = screen;
        self
    }
}
