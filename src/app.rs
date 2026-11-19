//! The app shell: owns the current `Library` value, the open session's
//! recorder or player, and applies `Action`s coming from the views.
//!
//! The `Library` itself is immutable: every change produces a new value
//! (see `lectio_core::model`), which `apply` swaps in and then saves.

mod material_actions;
mod ui_state;
mod workflow;
mod audio_actions;
mod speech_actions;

use std::collections::HashMap;

use eframe::egui;
use lectio_core::clock::{NoteDraft, RecorderCommand, RecorderPhase, RecordingClock};
use lectio_core::material::{BlobKey, orphaned_blobs};
use lectio_core::timeline::{clamp_offset, samples_to_ms};
use lectio_core::{LabelKind, Library, ModelError, NoteId, Recording, SessionId};

use crate::action::Action;
use crate::material::MaterialState;
use crate::platform::{
    AudioCapture, AudioPlayback, CaptureSource, Pending, Platform, STORE_SAMPLE_RATE, now_unix_ms,
};
use lectio_core::dsp::FilterSettings;
use crate::ui::{self, Banner, Buffers, View};

/// How often to redraw while recording or playing, so the clock moves.
const LIVE_REDRAW: std::time::Duration = std::time::Duration::from_millis(100);
/// Where the playhead sits in the sample session.
const SAMPLE_PLAYHEAD_MS: u64 = 17 * 60_000 + 40_000;

/// How the app starts.
pub enum Startup {
    /// Built-in sample data, nothing saved, no microphone (`--screenshot`, web `#sample`).
    Sample,
    /// The user's own data on this platform, or why it couldn't be opened.
    Saved(Result<Box<dyn Platform>, String>),
}

/// A session with no recording yet: the recorder lives here.
struct Capture {
    session: SessionId,
    clock: RecordingClock,
    recorder: Option<Box<dyn AudioCapture>>,
}

/// A session that has a recording: the player lives here.
struct Playback {
    session: SessionId,
    player: Option<Box<dyn AudioPlayback>>,
    /// Position used when no player could be opened (sample data, no speaker).
    fallback_position_ms: u64,
    problem: Option<String>,
}

pub struct LectioApp {
    library: Library,
    platform: Option<Box<dyn Platform>>,
    /// False if the saved data couldn't be read safely: then nothing is
    /// written, so it can't be overwritten.
    can_save: bool,
    sample_mode: bool,
    selected: Option<SessionId>,
    capture: Option<Capture>,
    playback: Option<Playback>,
    draft: NoteDraft,
    buffers: Buffers,
    banner: Option<Banner>,
    /// Phase 2: imports in progress, the slide viewer, image/slide textures.
    material: MaterialState,
    /// Files for the built-in sample data (no platform storage in that mode).
    sample_blobs: HashMap<BlobKey, Vec<u8>>,
    /// Redesign (ADR-014): undo, level meter, panel and theme choices.
    ui_state: ui_state::UiState,
    /// Phase 3 (ADR-016): the speech service, transcription, captions, search.
    speech: speech_actions::SpeechState,
    #[cfg(not(target_arch = "wasm32"))]
    screenshot: Option<crate::screenshot::ScreenshotDriver>,
}

impl LectioApp {
    pub fn new(cc: &eframe::CreationContext<'_>, startup: Startup) -> Self {
        ui::theme::install(&cc.egui_ctx);
        let mut app = match startup {
            Startup::Sample => Self::with_sample_data(),
            Startup::Saved(Ok(platform)) => Self::with_saved_data(platform),
            Startup::Saved(Err(problem)) => Self {
                banner: Some(Banner::error(format!("{problem} Changes in this window will not be saved."))),
                ..Self::empty(Library::default(), None)
            },
        };
        app.purge_expired_trash();
        app.speech.service = app.platform.as_ref().filter(|_| app.can_save).and_then(|p| p.speech());
        // The saved theme choice (preferences are part of the library).
        cc.egui_ctx.set_theme(workflow::theme_from(app.library.preferences().theme));
        app
    }

    /// `--screenshot`: draw, capture the window to `path`, exit.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_screenshot(self, path: std::path::PathBuf) -> Self {
        Self { screenshot: Some(crate::screenshot::ScreenshotDriver::new(path)), ..self }
    }

    fn empty(library: Library, platform: Option<Box<dyn Platform>>) -> Self {
        Self {
            library,
            can_save: platform.is_some(),
            platform,
            sample_mode: false,
            selected: None,
            capture: None,
            playback: None,
            draft: NoteDraft::default(),
            buffers: Buffers::default(),
            banner: None,
            material: MaterialState::default(),
            sample_blobs: HashMap::new(),
            ui_state: ui_state::UiState::default(),
            speech: speech_actions::SpeechState::default(),
            #[cfg(not(target_arch = "wasm32"))]
            screenshot: None,
        }
    }

    fn with_sample_data() -> Self {
        let sample = crate::demo::sample_library();
        let session = sample.session;
        let mut app = Self {
            sample_mode: true,
            sample_blobs: sample.blobs.into_iter().collect(),
            ..Self::empty(sample.library, None)
        };
        app.select_session(session);
        if let Some(playback) = &mut app.playback {
            playback.fallback_position_ms = SAMPLE_PLAYHEAD_MS;
        }
        app
    }

    fn with_saved_data(platform: Box<dyn Platform>) -> Self {
        match platform.store().load() {
            Ok(outcome) => {
                let banner = outcome.recovered_from.map(|place| {
                    Banner::warning(format!(
                        "Your saved notes could not be read, so Lectio started fresh. \
                         The old data was kept at {place}."
                    ))
                });
                Self { banner, ..Self::empty(outcome.library, Some(platform)) }
            }
            // Don't risk overwriting data we couldn't read: run without saving.
            Err(e) => Self {
                banner: Some(Banner::error(format!("{e} Changes in this window will not be saved."))),
                can_save: false,
                ..Self::empty(Library::default(), Some(platform))
            },
        }
    }

    // ---------- reading state for the views ----------

    fn recorder_phase(&self) -> Option<RecorderPhase> {
        self.capture.as_ref().map(|c| c.clock.phase())
    }

    fn is_recording_active(&self) -> bool {
        matches!(self.recorder_phase(), Some(RecorderPhase::Recording | RecorderPhase::Paused))
    }

    fn current_offset_ms(&self) -> u64 {
        if let Some(capture) = &self.capture {
            return capture.clock.offset_ms();
        }
        match &self.playback {
            Some(Playback { player: Some(player), .. }) => player.position_ms(),
            Some(playback) => playback.fallback_position_ms,
            None => 0,
        }
    }

    fn duration_ms(&self) -> Option<u64> {
        let session = self.library.session(self.playback.as_ref()?.session)?;
        Some(session.recording.as_ref()?.duration_ms)
    }

    fn can_take_notes(&self) -> bool {
        self.playback.is_some() || self.is_recording_active()
    }

    /// Audio lost in the current recording: (dropped ms, overrun reports).
    fn audio_loss(&self) -> (u64, u64) {
        let Some(Capture { recorder: Some(recorder), .. }) = &self.capture else { return (0, 0) };
        let stats = recorder.stats();
        let dropped_ms = samples_to_ms(stats.frames_dropped, stats.device_sample_rate);
        (dropped_ms, stats.overruns)
    }

    fn view(&self) -> View<'_> {
        let playback = self.playback.as_ref();
        let (audio_dropped_ms, audio_overruns) = self.audio_loss();
        View {
            library: &self.library,
            selected: self.selected,
            recorder_phase: self.recorder_phase(),
            offset_ms: self.current_offset_ms(),
            duration_ms: self.duration_ms(),
            playing: playback.and_then(|p| p.player.as_ref()).is_some_and(|p| p.is_playing()),
            can_play: playback.and_then(|p| p.player.as_ref()).is_some_and(|p| p.is_ready()),
            playback_problem: playback.and_then(|p| p.problem.as_deref()),
            draft: &self.draft,
            can_take_notes: self.can_take_notes(),
            busy_recording: self.is_recording_active(),
            sample_mode: self.sample_mode,
            audio_dropped_ms,
            audio_overruns,
            material: &self.material,
            camera_available: self.platform.as_ref().is_some_and(|p| p.supports_camera()),
            can_import: self.platform.is_some() && self.can_save,
            undo_message: self.ui_state.undo.as_ref().map(|u| u.message),
            levels: self.ui_state.levels.iter().copied().collect(),
            sidebar: self.ui_state.sidebar,
            theme: self.ui_state.theme.unwrap_or(egui::ThemePreference::System),
            screen: self.ui_state.screen,
            narrow_tab: self.ui_state.narrow_tab,
            slide_zoom: self.ui_state.slide_zoom.max(1.0),
            fullscreen: self.ui_state.fullscreen,
            quiet_for_s: self.ui_state.quiet_for_s,
            prefs: self.library.preferences(),
            speech: self.speech_view(),
            source: self.ui_state.source,
            sources: CaptureSource::ALL.map(|s| match &self.platform {
                Some(platform) if self.can_save => platform.source_support(s),
                _ => Err("Recording is turned off in this window."),
            }),
            screenshots: self.platform.as_ref().is_some_and(|p| p.supports_screenshot()),
            screenshot: self.ui_state.screenshot.as_ref().map(|s| s.texture.as_ref()),
            cleaning: self.ui_state.cleaning.is_some(),
            has_cleaned: self
                .selected
                .and_then(|id| self.library.session(id))
                .and_then(|s| s.recording.as_ref())
                .is_some_and(|r| r.cleaned_file.is_some()),
        }
    }

    fn speech_view(&self) -> ui::SpeechView<'_> {
        let service = self.speech.service.as_deref();
        let model = self.library.preferences().speech_model;
        ui::SpeechView {
            available: service.is_some(),
            downloaded: service.is_some_and(|s| s.is_downloaded(model)),
            unavailable: lectio_core::prefs::SpeechModel::ALL.map(|m| service.and_then(|s| s.unavailable(m))),
            download: self.speech.download,
            job: self.speech.job.as_ref().map(|j| (j.session, j.done_ms, j.total_ms)),
            caption: &self.speech.caption,
            captions_supported: service.is_some_and(|s| s.supports_captions()),
            hits: &self.speech.hits,
            query: &self.speech.query,
        }
    }

    // ---------- selecting a session ----------

    fn select_session(&mut self, id: SessionId) {
        let Some(session) = self.library.session(id) else { return };
        if self.selected != Some(id) {
            self.material.open_session(session);
        }
        self.selected = Some(id);
        self.ui_state.screen = crate::action::Screen::Workspace;
        self.draft = NoteDraft::default();
        match &session.recording {
            None => {
                self.playback = None;
                self.capture = Some(Capture {
                    session: id,
                    clock: RecordingClock::new(STORE_SAMPLE_RATE),
                    recorder: None,
                });
            }
            Some(recording) => {
                let playback = self.open_playback(id, recording);
                self.capture = None;
                self.playback = Some(playback);
            }
        }
    }

    fn open_playback(&self, session: SessionId, recording: &Recording) -> Playback {
        let empty = Playback { session, player: None, fallback_position_ms: 0, problem: None };
        if self.sample_mode {
            return Playback { problem: Some("Sample data: there is no audio to play.".into()), ..empty };
        }
        let Some(platform) = &self.platform else { return empty };
        match platform.open_playback(&self.playback_file(recording)) {
            Ok(player) => {
                self.configure_player(player.as_ref());
                Playback { player: Some(player), ..empty }
            }
            Err(e) => Playback { problem: Some(e.to_string()), ..empty },
        }
    }

    // ---------- applying actions ----------

    fn apply_all(&mut self, actions: Vec<Action>) {
        if actions.is_empty() {
            return;
        }
        let before = self.library.clone();
        for action in actions {
            self.apply(action);
        }
        if self.library != before {
            self.persist();
            if self.ui_state.undo.is_none() {
                self.delete_orphaned_blobs(&before);
            }
        }
        self.end_undo_if_library_moved_on();
    }

    /// After a delete, files no note or deck refers to any more are removed
    /// (ADR-012). Runs after saving, so a crash can't leave a note pointing
    /// at a deleted file.
    fn delete_orphaned_blobs(&mut self, before: &Library) {
        for key in orphaned_blobs(before, &self.library) {
            self.material.forget_image(&key);
            let deleted = self.platform.as_ref().filter(|_| self.can_save).map(|p| p.delete_blob(&key));
            if let Some(Err(e)) = deleted {
                self.show_error(format!("An unused file could not be removed: {e}"));
            }
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::AddCourse(name) => self.add_course(&name),
            Action::AddSession(course) => self.add_session(course),
            Action::SelectSession(id) if !self.is_recording_active() => self.select_session(id),
            Action::SelectSession(_) => {}
            Action::Recorder(command) => self.handle_recorder(command),
            Action::DraftChanged(text) => {
                self.draft = self.draft.with_text(&text, self.current_offset_ms());
            }
            Action::SubmitNote => self.save_draft(&[]),
            Action::LabelMoment(kind) => self.label_moment(kind),
            Action::ToggleLabel(note, kind) => {
                self.change_session(|lib, s| lib.with_label_toggled(s, note, kind));
            }
            Action::DeleteNote(note) => self.delete_note_with_undo(note),
            Action::TogglePlay => self.toggle_play(),
            Action::Seek(offset) => self.seek(offset),
            Action::PlayFrom(offset) => {
                self.seek(offset);
                self.set_playing(true);
            }
            Action::SeekBy(delta) => {
                let target = self.current_offset_ms().saturating_add_signed(delta);
                self.seek(target);
            }
            Action::OpenLabelEditor => self.buffers.open_label_editor(self.library.labels()),
            Action::SaveLabels(names) => self.rename_labels(&names),
            Action::CloseLabelEditor => self.buffers.label_names = None,
            Action::DismissBanner => self.banner = None,
            other => self.apply_ui(other),
        }
    }

    fn add_course(&mut self, name: &str) {
        match self.library.with_course(name) {
            Ok((library, _)) => {
                self.library = library;
                self.buffers.new_course.clear();
            }
            Err(e) => self.show_error(e),
        }
    }

    fn add_session(&mut self, course: lectio_core::CourseId) {
        let count = self.library.course(course).map_or(0, |c| c.sessions.len());
        let title = format!("Lecture {}", count + 1);
        match self.library.with_session(course, &title, now_unix_ms()) {
            Ok((library, id)) => {
                self.library = library;
                if !self.is_recording_active() {
                    self.select_session(id);
                }
            }
            Err(e) => self.show_error(e),
        }
    }

    fn save_draft(&mut self, labels: &[LabelKind]) {
        let Some(session) = self.selected.filter(|_| self.can_take_notes()) else { return };
        let offset = self.draft.anchor_ms(self.current_offset_ms());
        let body = self.draft.text().to_owned();
        let added = self.library.with_note(session, offset, &body, labels);
        if self.adopt_new_note(session, added) {
            self.draft = NoteDraft::default();
        }
    }

    /// Takes a library with a freshly added note, links that note to the
    /// slide on screen (if "link new notes" is on), and shows any error.
    /// Returns whether the note was added.
    fn adopt_new_note(&mut self, session: SessionId, added: Result<(Library, NoteId), ModelError>) -> bool {
        let (library, note) = match added {
            Ok(added) => added,
            Err(e) => {
                self.show_error(e);
                return false;
            }
        };
        let link = library.session(session).and_then(|s| self.material.current_link(s));
        self.library = match link {
            Some(link) => library.with_slide_link(session, note, Some(link)).unwrap_or(library),
            None => library,
        };
        true
    }


    /// One tap on a label: if a note is being typed, save it with the label;
    /// otherwise drop a label-only marker at the current moment.
    fn label_moment(&mut self, kind: LabelKind) {
        if self.draft.text().trim().is_empty() {
            self.draft = NoteDraft::default();
        }
        self.save_draft(&[kind]);
    }

    fn rename_labels(&mut self, names: &[(LabelKind, String)]) {
        let renamed = names
            .iter()
            .try_fold(self.library.clone(), |lib, (kind, name)| lib.with_label_renamed(*kind, name));
        match renamed {
            Ok(library) => {
                self.library = library;
                self.buffers.label_names = None;
            }
            Err(e) => self.show_error(e),
        }
    }

    fn change_session(&mut self, change: impl FnOnce(&Library, SessionId) -> Result<Library, ModelError>) {
        let Some(session) = self.selected else { return };
        match change(&self.library, session) {
            Ok(library) => self.library = library,
            Err(e) => self.show_error(e),
        }
    }

    // ---------- recording ----------

    fn handle_recorder(&mut self, command: RecorderCommand) {
        let Some(capture) = &self.capture else { return };
        let next_clock = match capture.clock.apply(command) {
            Ok(clock) => clock,
            Err(e) => return self.show_error(e),
        };
        match command {
            RecorderCommand::Start => self.start_recording(next_clock),
            RecorderCommand::Pause | RecorderCommand::Resume => {
                if let Some(capture) = &mut self.capture {
                    if let Some(recorder) = &capture.recorder {
                        recorder.set_paused(command == RecorderCommand::Pause);
                    }
                    capture.clock = next_clock;
                }
            }
            RecorderCommand::Stop => self.finish_recording(),
        }
    }

    fn start_recording(&mut self, next_clock: RecordingClock) {
        let Some(platform) = &self.platform else {
            self.banner = Some(Banner::warning("Recording is turned off in this window.".into()));
            return;
        };
        let Some(capture) = &mut self.capture else { return };
        match platform.start_capture(&recording_file_name(capture.session), self.ui_state.source) {
            Ok(recorder) => {
                capture.recorder = Some(recorder);
                capture.clock = next_clock;
                self.after_recording_started();
            }
            Err(e) => self.banner = Some(Banner::error(format!("{e}. {}", platform.mic_help()))),
        }
    }

    fn mic_help(&self) -> &'static str {
        self.platform.as_ref().map_or("", |p| p.mic_help())
    }

    fn finish_recording(&mut self) {
        let Some(mut capture) = self.capture.take() else { return };
        let Some(recorder) = capture.recorder.take() else { return };
        let total = recorder.finish().unwrap_or_else(|e| {
            self.show_error(e);
            0
        });
        let duration_ms = samples_to_ms(total, STORE_SAMPLE_RATE).max(capture.clock.offset_ms());
        let recording = Recording {
            file_name: recording_file_name(capture.session),
            duration_ms,
            sample_rate: STORE_SAMPLE_RATE,
            cleaned_file: None,
        };
        match self.library.with_recording(capture.session, recording) {
            Ok(library) => self.library = library,
            Err(e) => self.show_error(e),
        }
        if duration_ms == 0 {
            self.banner = Some(Banner::warning(format!("No audio was captured. {}", self.mic_help())));
        }
        self.persist();
        self.select_session(capture.session);
    }

    // ---------- playback ----------

    fn set_playing(&self, playing: bool) {
        if let Some(Playback { player: Some(player), .. }) = &self.playback {
            player.set_playing(playing);
        }
    }

    fn toggle_play(&self) {
        if let Some(Playback { player: Some(player), .. }) = &self.playback {
            player.set_playing(!player.is_playing());
        }
    }

    fn seek(&mut self, offset_ms: u64) {
        let duration = self.duration_ms().unwrap_or(0);
        if let Some(playback) = &mut self.playback {
            let target = clamp_offset(offset_ms, duration);
            playback.fallback_position_ms = target;
            if let Some(player) = &playback.player {
                player.seek_ms(target);
            }
        }
    }

    // ---------- saving, errors, per-frame upkeep ----------

    fn persist(&mut self) {
        let Some(platform) = self.platform.as_ref().filter(|_| self.can_save) else { return };
        if let Err(e) = platform.store().save(&self.library) {
            self.banner = Some(Banner::error(format!("Could not save: {e}")));
        }
    }

    /// Problems reported later by things running in the background: browser
    /// storage writes, and audio loading on the web.
    fn collect_background_errors(&mut self) {
        if let Some(e) = self.platform.as_ref().and_then(|p| p.store().take_background_error()) {
            self.banner = Some(Banner::error(format!("Could not save: {e}")));
        }
        let playback = self.playback.as_mut();
        if let Some((playback, problem)) =
            playback.and_then(|p| p.player.as_ref().and_then(|pl| pl.take_error()).map(|e| (p, e)))
        {
            playback.problem = Some(problem);
            playback.player = None;
        }
    }

    fn show_error(&mut self, e: impl std::fmt::Display) {
        self.banner = Some(Banner::error(e.to_string()));
    }

    fn tick(&mut self, ctx: &egui::Context) {
        if let Some(Capture { clock, recorder: Some(recorder), .. }) = &mut self.capture {
            *clock = clock.with_total_samples(recorder.samples_written());
            if let Some(problem) = recorder.take_error() {
                self.banner = Some(Banner::error(problem));
            }
        }
        self.collect_background_errors();
        let now = ctx.input(|i| i.time);
        let material_busy = self.tick_material(ctx)
            | self.tick_speech(now)
            | self.tick_cleaning()
            | self.tick_screenshot(ctx);
        self.tick_ui_state(now);
        let loading = self.playback.as_ref().and_then(|p| p.player.as_ref()).is_some_and(|p| !p.is_ready());
        if self.is_recording_active() || self.view().playing || loading || material_busy {
            ctx.request_repaint_after(LIVE_REDRAW);
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.is_recording_active() {
            // Closing the window mid-lecture must not lose the audio.
            self.finish_recording();
        }
    }
}

impl eframe::App for LectioApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui::widgets::clear_named_widgets(ui.ctx());
        self.ui_state.theme = Some(ui.ctx().options(|o| o.theme_preference));
        let mut actions = ui::shortcuts(ui.ctx(), &self.view());
        // The view borrows all of `self` read-only, so the text buffers are
        // moved out for the frame and put back after (see PROBLEMS P-003).
        let mut buffers = std::mem::take(&mut self.buffers);
        ui::draw(ui, &self.view(), &mut buffers, self.banner.as_ref(), &mut actions);
        self.buffers = buffers;
        self.apply_all(actions);
        if let Some(theme) = self.ui_state.theme_request.take() {
            ui.ctx().set_theme(theme);
        }
        if let Some(text) = self.ui_state.clipboard.take() {
            ui.ctx().copy_text(text);
            self.banner = Some(Banner::warning("Copied the session as Markdown.".into()));
        }
        #[cfg(target_arch = "wasm32")]
        crate::entry_web::publish_named_widgets(ui.ctx());
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(driver) = &mut self.screenshot {
            driver.on_frame(ui.ctx());
        }
    }
}

fn recording_file_name(session: SessionId) -> String {
    format!("{session}.wav")
}
