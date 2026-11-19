//! Phase 3 audio actions on `LectioApp` (ADR-017, ADR-019): playback speed,
//! filters and the cleaned copy; the recording source; screenshots into the
//! feed.

use std::io::Cursor;

use eframe::egui::ColorImage;
use lectio_core::dsp::{FilterChain, analyse};
use lectio_core::material::{AttachmentKind, MediaKind};
use lectio_core::stretch::clamp_speed;

use super::material_actions::NewImage;
use super::*;
use crate::media::images::encode_jpeg;
use crate::platform::wav::{decode_mono_i16, encode_mono_i16};

/// A screen picture waiting for the user to choose the region.
pub struct ScreenshotState {
    pub pending: Option<Pending<ColorImage>>,
    pub image: Option<ColorImage>,
    pub texture: Option<egui::TextureHandle>,
    /// When "Screenshot" was pressed: where the note lands.
    pub offset_ms: u64,
}

/// "Save cleaned copy": the original is being read.
pub struct Cleaning {
    session: SessionId,
    bytes: Pending<Vec<u8>>,
}

impl LectioApp {
    pub(super) fn apply_audio(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::SetSpeed(percent) => {
                let percent = clamp_speed(percent);
                self.change_prefs(|p| p.speed_percent = percent);
                self.with_player(|player| player.set_speed(percent));
            }
            Action::SetFilters(filters) => {
                self.change_prefs(|p| p.filters = filters);
                let live = if self.playing_cleaned() { FilterSettings::default() } else { filters };
                self.with_player(|player| player.set_filters(live));
            }
            Action::SaveCleanedCopy => self.start_cleaning(),
            Action::SetPlayCleaned(on) => {
                self.change_prefs(|p| p.play_cleaned = on);
                self.reopen_playback();
            }
            Action::SetSource(source) => self.choose_source(source),
            Action::TakeScreenshot => self.take_screenshot(),
            Action::SaveScreenshot { x, y, w, h } => self.save_screenshot([x, y, w, h]),
            Action::CloseScreenshot => self.ui_state.screenshot = None,
            other => return Some(other),
        }
        None
    }

    fn with_player(&self, f: impl FnOnce(&dyn AudioPlayback)) {
        if let Some(Playback { player: Some(player), .. }) = &self.playback {
            f(player.as_ref());
        }
    }

    /// Whether the open session plays its cleaned copy (then the live
    /// filters stay off, so they aren't applied twice).
    pub(super) fn playing_cleaned(&self) -> bool {
        let recording = self.selected.and_then(|id| self.library.session(id)).and_then(|s| s.recording.as_ref());
        self.prefs().play_cleaned && recording.is_some_and(|r| r.cleaned_file.is_some())
    }

    /// The file to play: the cleaned copy if there is one and it's wanted.
    pub(super) fn playback_file(&self, recording: &Recording) -> String {
        match &recording.cleaned_file {
            Some(cleaned) if self.prefs().play_cleaned => cleaned.clone(),
            _ => recording.file_name.clone(),
        }
    }

    /// Applies the saved speed and filters to a newly opened player.
    pub(super) fn configure_player(&self, player: &dyn AudioPlayback) {
        player.set_speed(self.prefs().speed_percent);
        if !self.playing_cleaned() {
            player.set_filters(self.prefs().filters);
        }
    }

    fn reopen_playback(&mut self) {
        let Some(session) = self.selected.and_then(|id| self.library.session(id)) else { return };
        let Some(recording) = session.recording.clone() else { return };
        let position = self.current_offset_ms();
        let playback = self.open_playback(session.id, &recording);
        self.playback = Some(playback);
        self.seek(position);
    }

    fn choose_source(&mut self, source: CaptureSource) {
        if self.is_recording_active() {
            return self.show_error("The source can't change during a recording.");
        }
        match self.platform.as_ref().map(|p| p.source_support(source)) {
            Some(Err(why)) => self.show_error(why),
            _ => self.ui_state.source = source,
        }
    }

    fn start_cleaning(&mut self) {
        let Some(session) = self.selected.and_then(|id| self.library.session(id)) else { return };
        let Some(recording) = &session.recording else { return };
        if !self.prefs().filters.any() {
            return self.show_error("Turn on at least one filter first; the copy is made with the filters that are on.");
        }
        let Some(platform) = self.platform.as_ref().filter(|_| self.can_save) else {
            return self.show_error("Saving files is turned off in this window.");
        };
        let bytes = platform.read_recording(&recording.file_name);
        self.ui_state.cleaning = Some(Cleaning { session: session.id, bytes });
    }

    /// Per-frame: finishes a cleaned copy once its original has been read.
    pub(super) fn tick_cleaning(&mut self) -> bool {
        let Some(cleaning) = &self.ui_state.cleaning else { return false };
        let Some(result) = cleaning.bytes.take() else { return true };
        let session = cleaning.session;
        self.ui_state.cleaning = None;
        let saved = result.and_then(|bytes| self.write_cleaned_copy(session, bytes));
        match saved {
            Ok(()) => {
                self.persist();
                if self.selected == Some(session) {
                    self.reopen_playback();
                }
                self.banner = Some(Banner::warning("Saved a cleaned copy. Playback now uses it.".into()));
            }
            Err(e) => self.show_error(format!("The cleaned copy could not be saved: {e}")),
        }
        false
    }

    /// Filters the whole recording with the current settings and stores it
    /// as `<session>-clean.wav`.
    fn write_cleaned_copy(&mut self, session: SessionId, bytes: Vec<u8>) -> Result<(), String> {
        let (samples, rate) = decode_mono_i16(Cursor::new(bytes)).map_err(|e| e.to_string())?;
        let mut floats: Vec<f32> = samples.iter().map(|&s| f32::from(s) / f32::from(i16::MAX)).collect();
        let analysis = analyse(&floats, rate);
        FilterChain::new(self.prefs().filters, &analysis, rate).process(&mut floats);
        let pcm: Vec<i16> = floats.iter().map(|&s| lectio_core::resample::to_i16(s)).collect();
        let mut wav = Cursor::new(Vec::new());
        encode_mono_i16(&pcm, rate, &mut wav).map_err(|e| e.to_string())?;
        let wav = wav.into_inner();
        let name = format!("{session}-clean.wav");
        let platform = self.platform.as_ref().ok_or("no storage")?;
        platform.write_recording(&name, wav)?;
        self.library = self.library.with_cleaned_copy(session, Some(&name)).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn take_screenshot(&mut self) {
        if !self.can_take_notes() {
            return self.show_error("Start recording (or open a recorded session) to add screenshots to its timeline.");
        }
        let Some(platform) = self.platform.as_ref().filter(|p| p.supports_screenshot()) else {
            return self.show_error("Screenshots aren't possible in this window.");
        };
        let pending = platform.capture_screen();
        let offset_ms = self.current_offset_ms();
        self.ui_state.screenshot = Some(ScreenshotState { pending: Some(pending), image: None, texture: None, offset_ms });
    }

    /// Per-frame: turns a finished screen capture into a texture to crop.
    pub(super) fn tick_screenshot(&mut self, ctx: &egui::Context) -> bool {
        let Some(shot) = &mut self.ui_state.screenshot else { return false };
        let Some(result) = shot.pending.as_ref().and_then(Pending::take) else { return shot.pending.is_some() };
        shot.pending = None;
        match result {
            Ok(image) => {
                shot.texture = Some(ctx.load_texture("screenshot", image.clone(), egui::TextureOptions::LINEAR));
                shot.image = Some(image);
            }
            Err(e) => {
                self.ui_state.screenshot = None;
                self.show_error(e);
            }
        }
        false
    }

    /// Crops the chosen region (fractions of the picture) into a photo note.
    fn save_screenshot(&mut self, region: [f32; 4]) {
        let Some(session) = self.selected else { return };
        let Some(shot) = self.ui_state.screenshot.take() else { return };
        let Some(image) = shot.image else { return };
        let saved = crop(&image, region).and_then(|(rgba, width, height)| {
            let jpeg = encode_jpeg(&rgba, width, height)?;
            let picture = NewImage { kind: AttachmentKind::Photo, media: MediaKind::Jpeg, bytes: jpeg, width, height };
            self.add_image_note(session, shot.offset_ms, picture, "Screenshot")
        });
        if let Err(e) = saved {
            self.show_error(e);
        }
    }
}

/// RGBA bytes of a region given as fractions (x, y, width, height) of the image.
fn crop(image: &ColorImage, [x, y, w, h]: [f32; 4]) -> Result<(Vec<u8>, u32, u32), String> {
    let [full_w, full_h] = image.size;
    let px = |f: f32, full: usize| ((f.clamp(0.0, 1.0) * full as f32).round() as usize).min(full);
    let (left, top) = (px(x, full_w), px(y, full_h));
    let (right, bottom) = (px(x + w, full_w), px(y + h, full_h));
    if right <= left + 4 || bottom <= top + 4 {
        return Err("Drag over the part of the screen to keep.".into());
    }
    let mut rgba = Vec::with_capacity((right - left) * (bottom - top) * 4);
    for row in top..bottom {
        for pixel in &image.pixels[row * full_w + left..row * full_w + right] {
            rgba.extend_from_slice(&pixel.to_srgba_unmultiplied());
        }
    }
    Ok((rgba, (right - left) as u32, (bottom - top) as u32))
}
