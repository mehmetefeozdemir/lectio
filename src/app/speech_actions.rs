//! Phase 3 text actions on `LectioApp` (ADR-016, ADR-018): transcription,
//! the model download, live captions, and search. The speech service runs
//! elsewhere (a thread or a Web Worker); here the app sends requests and
//! folds the events it gets back into the library.

use lectio_core::prefs::{Preferences, SpeechModel};
use lectio_core::search::{Hit, search};
use lectio_core::transcript::{Segment, Transcript, merged};

use super::*;
use crate::platform::speech::{SpeechEvent, SpeechRequest, SpeechService};

/// How much recent audio a live caption covers.
const CAPTION_WINDOW_MS: u64 = 8_000;
/// How often a new caption is asked for (seconds).
const CAPTION_EVERY_S: f64 = 2.0;
/// Search results shown at most.
const SEARCH_LIMIT: usize = 60;

/// A transcription in progress.
pub struct TranscribeJob {
    pub session: SessionId,
    pub model: SpeechModel,
    pub done_ms: u64,
    pub total_ms: u64,
}

#[derive(Default)]
pub struct SpeechState {
    pub service: Option<Box<dyn SpeechService>>,
    /// (model, bytes done, bytes total) while a model downloads.
    pub download: Option<(SpeechModel, u64, u64)>,
    pub job: Option<TranscribeJob>,
    pub caption: String,
    caption_due_at: f64,
    caption_busy: bool,
    pub hits: Vec<Hit>,
    pub query: String,
}

impl LectioApp {
    /// Handles the text actions; returns any other action unchanged.
    pub(super) fn apply_speech(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::ShowTranscript(on) => self.change_prefs(|p| p.show_transcript_tab = on),
            Action::SetSpeechModel(model) => self.change_prefs(|p| p.speech_model = model),
            Action::SetLanguage(language) => self.change_prefs(|p| p.language = language),
            Action::DownloadModel => self.send_speech(SpeechRequest::Prepare { model: self.prefs().speech_model }),
            Action::Transcribe => self.start_transcription(),
            Action::CancelTranscription => self.send_speech(SpeechRequest::Cancel),
            Action::ClearTranscript => self.change_session(|lib, s| lib.without_transcript(s)),
            Action::SegmentToFeed(index) => self.segment_to_feed(index),
            Action::SetCaptions(on) => {
                self.change_prefs(|p| p.captions = on);
                self.speech.caption.clear();
            }
            Action::SearchFor(query) => self.run_search(query),
            Action::OpenHit { session, offset_ms, slide } => self.open_hit(session, offset_ms, slide),
            other => return Some(other),
        }
        None
    }

    pub(super) fn prefs(&self) -> &Preferences {
        self.library.preferences()
    }

    pub(super) fn change_prefs(&mut self, change: impl FnOnce(&mut Preferences)) {
        let mut prefs = self.prefs().clone();
        change(&mut prefs);
        self.library = self.library.with_preferences(prefs);
    }

    fn send_speech(&mut self, request: SpeechRequest) {
        match &self.speech.service {
            Some(service) => service.send(request),
            None => self.show_error("Speech-to-text is turned off in this window (sample data)."),
        }
    }

    fn start_transcription(&mut self) {
        let Some(session) = self.selected.and_then(|id| self.library.session(id)) else { return };
        let Some(recording) = &session.recording else {
            return self.show_error("Record the lecture first, then transcribe it.");
        };
        if self.speech.job.is_some() {
            return self.show_error("A transcription is already running. Wait for it, or stop it first.");
        }
        let model = self.prefs().speech_model;
        if let Some(why) = self.speech.service.as_ref().and_then(|s| s.unavailable(model)) {
            return self.show_error(why);
        }
        // An unfinished transcript continues where it stopped; a finished one is redone.
        let start_ms = match &session.transcript {
            Some(t) if !t.complete => t.segments.last().map_or(0, |s| s.end_ms),
            _ => 0,
        };
        let (id, file_name, total_ms) = (session.id, recording.file_name.clone(), recording.duration_ms);
        if start_ms == 0 {
            let empty =
                Transcript { language: String::new(), model: model.name().into(), segments: Vec::new(), complete: false };
            self.change_session(|lib, s| lib.with_transcript(s, empty));
        }
        let language = self.prefs().language.clone();
        self.speech.job = Some(TranscribeJob { session: id, model, done_ms: start_ms, total_ms });
        self.send_speech(SpeechRequest::Transcribe { session: id, file_name, model, language, start_ms });
    }

    fn segment_to_feed(&mut self, index: usize) {
        let Some(session) = self.selected else { return };
        let segment = self
            .library
            .session(session)
            .and_then(|s| s.transcript.as_ref())
            .and_then(|t| t.segments.get(index))
            .cloned();
        let Some(segment) = segment else { return };
        let added = self.library.with_note(session, segment.start_ms, &segment.text, &[]);
        self.adopt_new_note(session, added);
    }

    pub(super) fn run_search(&mut self, query: String) {
        self.speech.hits = search(&self.library, &query, SEARCH_LIMIT);
        self.speech.query = query;
    }

    fn open_hit(&mut self, session: SessionId, offset_ms: Option<u64>, slide: Option<lectio_core::material::SlideLink>) {
        if self.is_recording_active() && self.selected != Some(session) {
            return self.show_error("Stop the recording before opening another session.");
        }
        self.select_session(session);
        if let Some(offset) = offset_ms {
            self.seek(offset);
        }
        if let Some(link) = slide {
            self.material.show_deck(link.deck, link.page);
        }
    }

    /// Per-frame: folds speech events into the library and asks for captions.
    /// Returns true while something is downloading or transcribing.
    pub(super) fn tick_speech(&mut self, now: f64) -> bool {
        let events = self.speech.service.as_ref().map(|s| s.poll()).unwrap_or_default();
        let before = self.library.clone();
        for event in events {
            self.on_speech_event(event);
        }
        if self.library != before {
            self.persist();
        }
        self.request_caption(now);
        self.speech.download.is_some() || self.speech.job.is_some() || self.speech.caption_busy
    }

    fn on_speech_event(&mut self, event: SpeechEvent) {
        match event {
            SpeechEvent::Downloading { model, done, total } => self.speech.download = Some((model, done, total)),
            SpeechEvent::Ready { .. } => self.speech.download = None,
            SpeechEvent::Progress { session, done_ms, total_ms } => {
                if let Some(job) = self.speech.job.as_mut().filter(|j| j.session == session) {
                    (job.done_ms, job.total_ms) = (done_ms, total_ms);
                }
            }
            SpeechEvent::Segments { session, segments, language } => {
                self.add_segments(session, &segments, language, false);
            }
            SpeechEvent::Finished { session, language, complete } => {
                self.add_segments(session, &[], language, complete);
                self.speech.job = None;
            }
            SpeechEvent::Caption(text) => {
                self.speech.caption = text;
                self.speech.caption_busy = false;
            }
            SpeechEvent::Failed { session, message } => {
                if session.is_some() {
                    self.speech.job = None;
                }
                if session.is_none() && self.speech.caption_busy {
                    self.speech.caption_busy = false;
                    self.change_prefs(|p| p.captions = false);
                }
                self.speech.download = None;
                self.show_error(message);
            }
        }
    }

    fn add_segments(&mut self, session: SessionId, new: &[Segment], language: String, complete: bool) {
        let model = self.speech.job.as_ref().map_or_else(|| self.prefs().speech_model, |j| j.model);
        let existing = self.library.session(session).and_then(|s| s.transcript.clone());
        let segments = existing.as_ref().map_or_else(|| new.to_vec(), |t| merged(&t.segments, new));
        let transcript = Transcript { language, model: model.name().into(), segments, complete };
        match self.library.with_transcript(session, transcript) {
            Ok(library) => self.library = library,
            Err(e) => self.show_error(e),
        }
    }

    fn request_caption(&mut self, now: f64) {
        let wanted = self.prefs().captions
            && self.recorder_phase() == Some(RecorderPhase::Recording)
            && self.speech.service.as_ref().is_some_and(|s| s.supports_captions());
        if !wanted || self.speech.caption_busy || now < self.speech.caption_due_at {
            return;
        }
        let Some(Capture { recorder: Some(recorder), .. }) = &self.capture else { return };
        let pcm = recorder.recent_audio(CAPTION_WINDOW_MS);
        if pcm.is_empty() {
            return;
        }
        let (model, language) = (self.prefs().speech_model, self.prefs().language.clone());
        self.speech.caption_busy = true;
        self.speech.caption_due_at = now + CAPTION_EVERY_S;
        self.send_speech(SpeechRequest::Caption { pcm, model, language });
    }
}
