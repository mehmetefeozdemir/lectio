//! HTTPS on the desktop (ADR-016, ADR-021, ADR-022): checked downloads of
//! model and dictionary files, and the small JSON requests the study
//! providers make. Always called from a background thread, never the UI.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use lectio_speech::models::{Checksum, ModelFile};

use crate::platform::ai::host;

/// Report download progress about every this many bytes.
const PROGRESS_STEP: u64 = 1 << 20;
const READ_BUFFER: usize = 1 << 16;
/// A local model reply can take minutes on a slow machine.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into()
}

/// Streams `url` to `<path>.part`, checking size and SHA-256 as it arrives,
/// then renames it into place (so a half download is never mistaken for a
/// file). `progress(done, total)` counts from `done_before`.
pub fn download(
    url: &str,
    file: &ModelFile,
    path: &Path,
    done_before: u64,
    total: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let dir = path.parent().ok_or("bad download folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let part = path.with_extension("part");
    let offline = |e: ureq::Error| format!("Could not download {} ({e}). Check the internet connection.", file.name);
    let response = agent().get(url).call().map_err(offline)?;
    if !response.status().is_success() {
        return Err(format!("The server answered {} for {}.", response.status(), file.name));
    }
    let mut reader = response.into_body().into_reader();
    let mut out = File::create(&part).map_err(|e| format!("Could not save {}: {e}", file.name))?;
    let mut checksum = Checksum::default();
    let mut buffer = vec![0u8; READ_BUFFER];
    let (mut got, mut reported) = (0u64, 0u64);
    loop {
        let n = reader.read(&mut buffer).map_err(|e| format!("The download stopped: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buffer[..n]).map_err(|e| format!("Could not save {}: {e}", file.name))?;
        checksum.update(&buffer[..n]);
        got += n as u64;
        if got - reported >= PROGRESS_STEP {
            reported = got;
            progress(done_before + got, total);
        }
    }
    drop(out);
    if let Err(problem) = checksum.check(file) {
        let _ = std::fs::remove_file(&part);
        return Err(problem);
    }
    std::fs::rename(&part, path).map_err(|e| format!("Could not save {}: {e}", file.name))
}

pub fn file_has_size(path: &Path, bytes: u64) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.len() == bytes)
}

/// GET returning the body as text. `Ok(None)` = 404 (e.g. a word the online
/// dictionary doesn't know).
pub fn get_text(url: &str) -> Result<Option<String>, String> {
    let response = agent().get(url).call().map_err(|e| format!("Could not reach {} ({e}).", host(url)))?;
    if response.status().as_u16() == 404 {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!("{} answered {}.", host(url), response.status()));
    }
    response.into_body().read_to_string().map(Some).map_err(|e| format!("Could not read the answer ({e})."))
}

/// POSTs JSON; `bearer` is an API key sent as `Authorization: Bearer …`.
/// Errors name the host only: never the key, never the request body.
pub fn post_json(url: &str, bearer: Option<&str>, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let mut request = agent().post(url).header("Content-Type", "application/json");
    if let Some(key) = bearer {
        request = request.header("Authorization", &format!("Bearer {key}"));
    }
    let response = request.send(body.to_string()).map_err(|e| format!("Could not reach {} ({e}).", host(url)))?;
    let status = response.status();
    let text = response.into_body().read_to_string().map_err(|e| format!("Could not read the answer ({e})."))?;
    if !status.is_success() {
        let detail: String = text.chars().take(200).collect();
        return Err(format!("{} answered {status}: {detail}", host(url)));
    }
    serde_json::from_str(&text).map_err(|e| format!("{} sent an answer Lectio can't read ({e}).", host(url)))
}
