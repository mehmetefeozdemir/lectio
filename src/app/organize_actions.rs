//! Phase 4a actions that need no AI: tasks across courses, the focus timer,
//! and dictionary look-ups (ADR-022).

use lectio_core::dictionary::Sense;
use lectio_core::focus::{FocusEntry, FocusTimer};
use lectio_core::{CourseId, LabelKind};

use super::*;
use crate::platform::dictionary::{DictEvent, DictRequest, DictionaryService};

const MS_PER_MINUTE: u64 = 60_000;
/// Longest focus session offered.
pub const MAX_FOCUS_MINUTES: u32 = 180;

/// A running (or paused) focus session.
pub struct FocusRun {
    pub course: CourseId,
    pub timer: FocusTimer,
    pub started_unix_ms: u64,
}

/// A word being looked up, and where its note would go.
pub struct LookUpState {
    pub word: String,
    pub session: Option<SessionId>,
    pub offset_ms: Option<u64>,
    /// `None` while waiting; then the senses and their source, or why not.
    pub result: Option<Result<(Vec<Sense>, String), String>>,
}

#[derive(Default)]
pub struct OrganizeState {
    pub focus: Option<FocusRun>,
    pub dictionary: Option<Box<dyn DictionaryService>>,
    pub dict_download: Option<(u64, u64)>,
    pub dict_indexing: bool,
    pub lookup: Option<LookUpState>,
}

impl LectioApp {
    pub(super) fn apply_organize(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::SetTaskDone { session, note, done } => self.change_library(|lib| lib.with_task_done(session, note, done)),
            Action::SetTaskDue { session, note, due_unix_ms } => {
                self.change_library(|lib| lib.with_task_due(session, note, due_unix_ms));
            }
            Action::StartFocus { course, minutes } => {
                let minutes = minutes.clamp(1, MAX_FOCUS_MINUTES);
                let now = now_unix_ms();
                let timer = FocusTimer::new(u64::from(minutes) * MS_PER_MINUTE).start(now);
                self.organize.focus = Some(FocusRun { course, timer, started_unix_ms: now });
            }
            Action::PauseFocus => self.change_focus(|t, now| t.pause(now)),
            Action::ResumeFocus => self.change_focus(|t, now| t.resume(now)),
            Action::FinishFocus => self.finish_focus(),
            Action::CancelFocus => self.organize.focus = None,
            Action::LookUp { word, session, offset_ms } => self.look_up(word, session, offset_ms),
            Action::DefinitionToFeed(index) => self.definition_to_feed(index),
            Action::CloseLookUp => self.organize.lookup = None,
            Action::DownloadDictionary => self.send_dictionary(DictRequest::Prepare),
            Action::SetOnlineDictionary(on) => self.change_prefs(|p| p.online_dictionary = on),
            other => return Some(other),
        }
        None
    }

    fn change_focus(&mut self, change: impl FnOnce(FocusTimer, u64) -> FocusTimer) {
        if let Some(run) = &mut self.organize.focus {
            run.timer = change(run.timer, now_unix_ms());
        }
    }

    /// Logs the minutes actually spent (at least one) for the course.
    fn finish_focus(&mut self) {
        let Some(run) = self.organize.focus.take() else { return };
        let minutes = (run.timer.elapsed_ms(now_unix_ms()) / MS_PER_MINUTE) as u32;
        if minutes == 0 {
            return self.show_error("Less than a minute of focus; nothing was logged.");
        }
        let entry = FocusEntry { course: run.course, started_unix_ms: run.started_unix_ms, minutes };
        self.change_library(|lib| lib.with_focus_entry(entry));
        self.banner = Some(Banner::warning(format!("Logged {minutes} minutes of focus.")));
    }

    fn send_dictionary(&mut self, request: DictRequest) {
        match &self.organize.dictionary {
            Some(dictionary) => dictionary.send(request),
            None => self.show_error("The dictionary is turned off in this window (sample data)."),
        }
    }

    fn look_up(&mut self, word: String, session: Option<SessionId>, offset_ms: Option<u64>) {
        let word: String = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '\'').to_owned();
        if word.is_empty() {
            return;
        }
        let online = self.prefs().online_dictionary;
        self.organize.lookup = Some(LookUpState { word: word.clone(), session, offset_ms, result: None });
        self.send_dictionary(DictRequest::Lookup { word, online });
    }

    /// "word: definition" as a Definition note where the word was found
    /// (or at the current moment).
    fn definition_to_feed(&mut self, index: usize) {
        let Some(lookup) = &self.organize.lookup else { return };
        let Some(Ok((senses, _))) = &lookup.result else { return };
        let Some(sense) = senses.get(index) else { return };
        let Some(session) = lookup.session.or(self.selected) else { return };
        let body = format!("{}: {}", lookup.word, sense.definition);
        let offset = lookup.offset_ms.unwrap_or_else(|| self.current_offset_ms());
        let added = self.library.with_note(session, offset, &body, &[LabelKind::Definition]);
        if self.adopt_new_note(session, added) {
            self.organize.lookup = None;
        }
    }

    /// Per-frame: dictionary events and the focus timer's end.
    pub(super) fn tick_organize(&mut self) -> bool {
        let events = self.organize.dictionary.as_ref().map(|d| d.poll()).unwrap_or_default();
        for event in events {
            match event {
                DictEvent::Downloading { done, total } => self.organize.dict_download = Some((done, total)),
                DictEvent::Indexing => {
                    self.organize.dict_download = None;
                    self.organize.dict_indexing = true;
                }
                DictEvent::Ready => {
                    self.organize.dict_download = None;
                    self.organize.dict_indexing = false;
                }
                DictEvent::Found { word, senses, source } => {
                    if let Some(lookup) = self.organize.lookup.as_mut().filter(|l| l.word.eq_ignore_ascii_case(&word)) {
                        lookup.result = Some(Ok((senses, source)));
                    }
                }
                DictEvent::Failed { message } => {
                    self.organize.dict_download = None;
                    self.organize.dict_indexing = false;
                    match self.organize.lookup.as_mut().filter(|l| l.result.is_none()) {
                        Some(lookup) => lookup.result = Some(Err(message)),
                        None => self.show_error(message),
                    }
                }
            }
        }
        let finished = self.organize.focus.as_ref().is_some_and(|f| f.timer.is_done(now_unix_ms()));
        if finished {
            self.finish_focus();
        }
        let waiting = self.organize.lookup.as_ref().is_some_and(|l| l.result.is_none());
        self.organize.focus.as_ref().is_some_and(|f| f.timer.is_running())
            || waiting
            || self.organize.dict_download.is_some()
            || self.organize.dict_indexing
    }
}
