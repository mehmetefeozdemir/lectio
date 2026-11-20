# Lectio

Open-source lecture note-taking. Record a class, take notes that are timestamped to the audio, and jump back to any moment by clicking a note. Local-first: your recordings and notes stay on your device, with no account.

One Rust codebase runs as a native app on **Windows, macOS and Linux** and as a **web app** (WebAssembly).

**Try it in your browser: https://mehmetefeozdemir.github.io/lectio/** (no install, notes stay in your browser).

![Lectio in the browser](shots/web-sample.png)

## Features
**Capture**
- [x] Home page of sessions; courses; trash with 30-day restore
- [x] Record from the microphone, the computer's audio, or both; pause/resume; live level meter
- [x] Feed of timestamped notes, headings, photos, sketches and screenshots; one-tap labels (Heading, Important, Review, Task, Definition)
- [x] PDF slides beside the feed; slides posted into the feed as the lecture moves on
- [x] Live captions while recording

**Refine**
- [x] Offline transcription with Whisper (tiny / base / small), on the device; click a line to play from there
- [x] Search across notes, transcripts and slide text
- [x] Playback 0.5x–2x with pitch kept, ±10 s skips, noise reduction, voice boost, normalise
- [x] Reading view with Copy all and Markdown export; hideable panels; light and dark themes

**Study**
- [x] Optional AI study tools: a small model on the device, Ollama, or an OpenAI-compatible endpoint with your own key. Every item cites the moment it came from
- [x] Outline, Quiz me (with an offline fill-in-the-blank mode), flashcards with spaced repetition
- [x] Tasks across courses, focus timer, dictionary look-ups (offline WordNet)
- [x] Formula notes (LaTeX) with live preview; drawing and highlighting on slides
- [x] Paper view with Copy, Print, and PDF / Word / Markdown export
- [x] Collections across courses, with filter and sort
- [x] Screen-reader support on desktop and web (accessible names, keyboard focus order)

## Install
Prebuilt installers for macOS, Windows, Linux and the web are attached to each [GitHub release](../../releases). See [docs/README.md](docs/README.md) for how to install and open each one.

## Build and run
Requires a recent stable Rust toolchain.

```bash
# Desktop
cargo run --release

# Web (needs trunk: cargo install trunk --locked)
rustup target add wasm32-unknown-unknown
trunk serve --release        # then open http://127.0.0.1:8080

# Tests
cargo test --workspace

# Check the microphone pipeline on your machine (records N seconds, reports dropped frames / overruns)
cargo run --release -- --self-test-audio 30
```

On Linux you'll need the ALSA and X11/Wayland development packages; see `.github/workflows/ci.yml` for the exact list.

## How it's built
- `crates/lectio-core`: the data model, the recording clock, seek logic, resampling and storage. No UI, no audio, no platform code, so it's fully unit-tested and compiles unchanged for the browser.
- `src/platform/{native,web}`: recording, playback, storage and file access behind traits. Native uses `cpal` with a lock-free ring buffer for real-time-safe capture; the web uses an AudioWorklet and IndexedDB.
- `src/ui`: the egui interface.

The design is written up in the open:

| Doc | What's in it |
|---|---|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Structure, data model, threads and flows |
| [docs/DECISIONS.md](docs/DECISIONS.md) | Architecture decision records: what I chose, the alternatives, and why |
| [docs/PROBLEMS.md](docs/PROBLEMS.md) | Bugs and dead ends, with the root cause and the fix |

## Known limitations
- Local AI and transcription models are downloaded on first use (about 150 MB to 1 GB each, depending on the model).
- Linux and Windows are verified in CI. Day-to-day testing is on macOS.

## License
MIT
