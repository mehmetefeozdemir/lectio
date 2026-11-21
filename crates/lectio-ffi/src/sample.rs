//! The sample lecture for the mobile apps' store screenshots and UI tests:
//! the desktop's sample (`lectio_core::sample`), its slides, a silent
//! recording of the right length, and flashcards from its Definition notes.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use lectio_core::sample::{SAMPLE_RECORDING_MS, SAMPLE_SLIDES, sample_library};
use lectio_core::storage::LibraryStore;

use crate::library::LectioLibrary;
use crate::material::SAMPLE_RATE;
use crate::{LectioError, Result, now_ms};

/// A 16 kHz mono 16-bit WAV of silence, `ms` long: the header, then the file
/// is extended with zeros (instant, even for a 50-minute lecture).
fn write_silent_wav(path: &Path, ms: u64) -> std::io::Result<()> {
    let data_bytes = u32::try_from(ms * u64::from(SAMPLE_RATE) / 1_000 * 2).map_err(std::io::Error::other)?;
    let byte_rate = SAMPLE_RATE * 2;
    let mut header = Vec::with_capacity(44);
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    header.extend_from_slice(b"WAVEfmt ");
    header.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    header.extend_from_slice(&1u16.to_le_bytes()); // PCM
    header.extend_from_slice(&1u16.to_le_bytes()); // mono
    header.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    header.extend_from_slice(&byte_rate.to_le_bytes());
    header.extend_from_slice(&2u16.to_le_bytes()); // block align
    header.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    header.extend_from_slice(b"data");
    header.extend_from_slice(&data_bytes.to_le_bytes());
    let mut file = File::create(path)?;
    file.write_all(&header)?;
    file.set_len(44 + u64::from(data_bytes))
}

fn failed(what: &str, e: impl std::fmt::Display) -> LectioError {
    LectioError::Failed(format!("Can't add the sample lecture ({what}): {e}"))
}

#[uniffi::export]
impl LectioLibrary {
    /// Fills an empty library with the sample lecture ("Graph traversal" and
    /// its course, collection and neighbours); returns the lecture's id.
    pub fn add_sample_library(&self) -> Result<String> {
        let sample = sample_library()?;
        {
            let mut inner = self.lock()?;
            if !inner.library.courses().is_empty() {
                return Err(LectioError::Failed("The sample lecture can only be added to an empty library.".into()));
            }
            inner.blobs.put(&sample.slides, SAMPLE_SLIDES)?;
            write_silent_wav(&inner.store.audio_path(&sample.recording_file), SAMPLE_RECORDING_MS).map_err(|e| failed("recording", e))?;
            inner.store.save(&sample.library)?;
            inner.library = sample.library;
        }
        let session = sample.session.to_string();
        self.make_definition_cards(session.clone(), now_ms())?;
        Ok(session)
    }
}
