//! Desktop speech service (ADR-016): one background thread owns the loaded
//! Whisper model and works through requests in order. Models live in
//! `<data>/models/whisper-<size>/`; they are downloaded on first use over
//! HTTPS from a pinned revision and checked against SHA-256 checksums both
//! after the download and before each load.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use lectio_core::prefs::SpeechModel;
use lectio_speech::Engine;
use lectio_speech::job::{self, WindowDone};
use lectio_speech::models::{self, ModelFile, ModelInfo};

use super::fetch::{self, file_has_size};
use crate::platform::speech::{SpeechEvent, SpeechRequest, SpeechService};
use crate::platform::wav::decode_mono_i16;

pub struct NativeSpeech {
    requests: Sender<SpeechRequest>,
    events: Mutex<Receiver<SpeechEvent>>,
    cancel: Arc<AtomicBool>,
    models_dir: PathBuf,
}

impl NativeSpeech {
    /// `audio_path` maps a recording's file name to its path on disk.
    pub fn start(models_dir: PathBuf, audio_path: impl Fn(&str) -> PathBuf + Send + 'static) -> Self {
        let (requests, inbox) = mpsc::channel();
        let (outbox, events) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker = Worker { models_dir: models_dir.clone(), engine: None, events: outbox, cancel: Arc::clone(&cancel) };
        std::thread::Builder::new()
            .name("lectio-speech".into())
            .spawn(move || worker.run(inbox, audio_path))
            .expect("the speech thread could not start");
        Self { requests, events: Mutex::new(events), cancel, models_dir }
    }
}

impl SpeechService for NativeSpeech {
    fn send(&self, request: SpeechRequest) {
        if matches!(request, SpeechRequest::Cancel) {
            self.cancel.store(true, Ordering::SeqCst);
            return;
        }
        // The thread only stops when the app closes; a failed send then is harmless.
        let _ = self.requests.send(request);
    }

    fn poll(&self) -> Vec<SpeechEvent> {
        self.events.lock().map(|rx| rx.try_iter().collect()).unwrap_or_default()
    }

    fn is_downloaded(&self, model: SpeechModel) -> bool {
        files_present(&self.models_dir, model)
    }

    fn unavailable(&self, _model: SpeechModel) -> Option<&'static str> {
        None
    }

    fn supports_captions(&self) -> bool {
        true
    }
}

fn model_path(dir: &Path, info: &ModelInfo, file: &ModelFile) -> PathBuf {
    dir.join(format!("whisper-{}", info.model.name())).join(file.name)
}

fn files_present(dir: &Path, model: SpeechModel) -> bool {
    let info = models::info(model);
    info.files.iter().all(|f| file_has_size(&model_path(dir, info, f), f.bytes))
}

struct Worker {
    models_dir: PathBuf,
    engine: Option<(SpeechModel, Engine)>,
    events: Sender<SpeechEvent>,
    cancel: Arc<AtomicBool>,
}

impl Worker {
    fn run(mut self, inbox: Receiver<SpeechRequest>, audio_path: impl Fn(&str) -> PathBuf) {
        while let Ok(request) = inbox.recv() {
            // Only the newest caption matters; skip older ones still queued.
            let request = match request {
                SpeechRequest::Caption { .. } => inbox.try_iter().last().unwrap_or(request),
                other => other,
            };
            match request {
                SpeechRequest::Prepare { model } => {
                    if let Err(message) = self.ensure_engine(model) {
                        self.emit(SpeechEvent::Failed { session: None, message });
                    }
                }
                SpeechRequest::Transcribe { session, file_name, model, language, start_ms } => {
                    self.cancel.store(false, Ordering::SeqCst);
                    let path = audio_path(&file_name);
                    if let Err(message) = self.transcribe(session, &path, model, language.as_deref(), start_ms) {
                        self.emit(SpeechEvent::Failed { session: Some(session), message });
                    }
                }
                SpeechRequest::Caption { pcm, model, language } => self.caption(&pcm, model, language.as_deref()),
                SpeechRequest::Cancel => {}
            }
        }
    }

    fn emit(&self, event: SpeechEvent) {
        let _ = self.events.send(event);
    }

    fn transcribe(
        &mut self,
        session: lectio_core::SessionId,
        path: &Path,
        model: SpeechModel,
        language: Option<&str>,
        start_ms: u64,
    ) -> Result<(), String> {
        let pcm = read_pcm(path)?;
        let events = self.events.clone();
        let cancel = Arc::clone(&self.cancel);
        let engine = self.ensure_engine(model)?;
        let mut stopped = false;
        let language = job::transcribe(engine, &pcm, start_ms, language, |w: WindowDone| {
            let _ = events.send(SpeechEvent::Segments { session, segments: w.segments, language: w.language });
            let _ = events.send(SpeechEvent::Progress { session, done_ms: w.done_ms, total_ms: w.total_ms });
            stopped = cancel.load(Ordering::SeqCst);
            !stopped
        })?;
        self.emit(SpeechEvent::Finished { session, language, complete: !stopped });
        Ok(())
    }

    fn caption(&mut self, pcm: &[f32], model: SpeechModel, language: Option<&str>) {
        if !self.is_loaded(model) && !files_present(&self.models_dir, model) {
            let message = "Live captions need the speech model. Download it in the Transcript tab first.".into();
            return self.emit(SpeechEvent::Failed { session: None, message });
        }
        let result = self.ensure_engine(model).and_then(|engine| job::caption(engine, pcm, language));
        match result {
            Ok(text) => self.emit(SpeechEvent::Caption(text)),
            Err(message) => self.emit(SpeechEvent::Failed { session: None, message }),
        }
    }

    fn is_loaded(&self, model: SpeechModel) -> bool {
        self.engine.as_ref().is_some_and(|(m, _)| *m == model)
    }

    /// Downloads the model if needed, checks it and loads it (once).
    fn ensure_engine(&mut self, model: SpeechModel) -> Result<&mut Engine, String> {
        if !self.is_loaded(model) {
            self.engine = None;
            let events = self.events.clone();
            let engine = load_model(&self.models_dir, model, &mut |done, total| {
                let _ = events.send(SpeechEvent::Downloading { model, done, total });
            })?;
            self.engine = Some((model, engine));
            self.emit(SpeechEvent::Ready { model });
        }
        match &mut self.engine {
            Some((_, engine)) => Ok(engine),
            None => Err("The speech model could not be loaded.".into()),
        }
    }
}

/// 16 kHz mono samples of a stored recording.
fn read_pcm(path: &Path) -> Result<Vec<f32>, String> {
    let file = File::open(path).map_err(|e| format!("Could not open the recording: {e}"))?;
    let (samples, rate) = decode_mono_i16(BufReader::new(file)).map_err(|e| e.to_string())?;
    if rate != lectio_speech::mel::SAMPLE_RATE {
        return Err(format!("Recordings must be 16 kHz for transcription (this one is {rate} Hz)."));
    }
    Ok(samples.iter().map(|&s| f32::from(s) / f32::from(i16::MAX)).collect())
}

/// Downloads missing files (reporting bytes done / total), verifies all
/// three files, and loads the engine.
pub fn load_model(dir: &Path, model: SpeechModel, progress: &mut dyn FnMut(u64, u64)) -> Result<Engine, String> {
    let info = models::info(model);
    let total = info.total_bytes();
    let mut done = 0;
    for file in &info.files {
        let path = model_path(dir, info, file);
        if !file_has_size(&path, file.bytes) {
            fetch::download(&info.url(file), file, &path, done, total, progress)?;
        }
        done += file.bytes;
    }
    progress(total, total);
    let mut contents = Vec::new();
    for file in &info.files {
        let path = model_path(dir, info, file);
        let bytes = std::fs::read(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
        if let Err(problem) = models::verify(file, &bytes) {
            let _ = std::fs::remove_file(&path);
            return Err(problem);
        }
        contents.push(bytes);
    }
    let weights = contents.pop().unwrap_or_default();
    let tokenizer = contents.pop().unwrap_or_default();
    let config = contents.pop().unwrap_or_default();
    Engine::load(&config, &tokenizer, weights)
}

/// `lectio --transcribe <file.wav> [--model tiny|base|small] [--language xx]`:
/// downloads the model if needed, transcribes, prints the segments and the
/// real-time factor (processing time ÷ audio length; below 1 is faster than
/// real time).
pub fn run_cli(dir: &Path, wav: &Path, model: SpeechModel, language: Option<&str>) -> Result<(), String> {
    let loading = Instant::now();
    let mut last_percent = u64::MAX;
    let mut engine = load_model(dir, model, &mut |done, total| {
        let percent = done * 100 / total.max(1);
        if percent != last_percent && percent % 10 == 0 {
            last_percent = percent;
            eprintln!("model {}: {percent}%", model.name());
        }
    })?;
    eprintln!("model ready in {:.1} s", loading.elapsed().as_secs_f64());
    let pcm = read_pcm(wav)?;
    let audio_s = pcm.len() as f64 / f64::from(lectio_speech::mel::SAMPLE_RATE);
    let started = Instant::now();
    let language = job::transcribe(&mut engine, &pcm, 0, language, |w| {
        for s in &w.segments {
            println!("[{:>7.2} → {:>7.2}] {}", s.start_ms as f64 / 1e3, s.end_ms as f64 / 1e3, s.text);
        }
        true
    })?;
    let took = started.elapsed().as_secs_f64();
    println!(
        "language {language}; {audio_s:.1} s of audio in {took:.1} s; real-time factor {:.3} (model {})",
        took / audio_s.max(1e-9),
        model.name()
    );
    Ok(())
}
