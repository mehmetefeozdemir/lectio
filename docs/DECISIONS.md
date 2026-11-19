# Architecture Decision Records

## ADR-001: Clean-room build instead of cloning
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** I wanted an open-source alternative to the lecture note app my university's accessibility services use. The commercial app needs a login, and its name, design and text belong to its owner.
- **Options considered:**
  - Copy its UI screen by screen: legally risky, and it would make a weak portfolio story.
  - Build from a written spec of the features: original work, and I can explain every part.
- **Decision:** Write a spec from the general feature categories (recording, timestamped notes, slides, transcription, study tools) and build only from that.
- **Consequences:** The design is my own. I have to make UX decisions myself instead of copying them.

## ADR-002: Tech stack — Rust, with egui (eframe) for the UI and a UI-free core crate
- **Date:** 2026-11-19
- **Status:** accepted; **extended by ADR-009** (platform targets). ADR-009 keeps the language, UI library and core crate, and
  replaces the "desktop first" trade-off and the mobile notes below.
- **Context:** Phase 1 needs a window, microphone recording, audio playback with seeking, and storage that works with no account and no network. The spec asks for a stack that can later reach web and mobile and is local-first. Rust is already installed on my machine, and a desktop build is easy to check automatically with a screenshot.
- **Options considered:**
  - **TypeScript + React as a PWA (later wrapped with Capacitor for mobile).** Pros: runs on web and phones from one codebase; the browser gives `MediaRecorder` for audio and IndexedDB for local storage. Cons: Safari limits on `MediaRecorder` formats and background recording; storage can be evicted by the browser; a desktop build would need Electron or Tauri on top.
  - **Flutter (Dart).** Pros: one UI codebase for Android, iOS, desktop and web; good audio plugins. Cons: Flutter isn't installed here, I'd learn Dart and Rust at the same time, and recording on Flutter web depends on third-party plugins.
  - **Tauri 2 (Rust backend + web UI).** Pros: desktop and mobile targets, small binaries, Rust for audio and files. Cons: two languages and an IPC layer between them from day one; more moving parts than Phase 1 needs.
  - **Rust + egui/eframe (chosen).** Pros: one language; eframe compiles the same UI to native desktop and to WebAssembly for the browser; `cpal` gives cross-platform microphone and speaker access; the whole app is one `cargo build`; tests run with `cargo test` and no browser. Cons: egui's mobile support is immature (Android works, iOS is experimental), so a phone app would probably need a different UI shell; egui is an *immediate-mode* UI (the whole screen is redrawn from the current state every frame), which looks less native than platform widgets.
- **Decision:** Rust with eframe/egui for the desktop app. The rules for courses, sessions, notes, labels, the recording clock and seeking live in a separate crate, `crates/lectio-core`, that has **no UI and no audio dependencies**. The compiler then guarantees that the domain logic can be reused unchanged by a web build (eframe → WASM) or a future mobile shell (e.g. Swift/Kotlin bindings generated with `uniffi`).
- **Consequences:**
  - Easy: testing the data model and seek logic as plain Rust functions; shipping a single native binary; the WebAssembly path for the UI exists.
  - Hard: browser microphone capture is not covered by `cpal` the way desktop capture is, so the web build will need its own recorder (Web Audio / `MediaRecorder` via `web-sys`) behind the same interface. iOS would most likely need a native UI on top of `lectio-core`.
  - Trade-off I accept: desktop first and well-tested, instead of everywhere at once and shallow.

## ADR-003: Local-first storage — one JSON file plus a folder of WAV files, behind a `LibraryStore` trait
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** The spec says data must survive a restart with no account. Phase 1 data is small (courses, sessions, notes) except for the audio, which is large.
- **Options considered:**
  - **SQLite (via `rusqlite`).** Pros: queries, transactions, scales to many sessions. Cons: a schema and migrations from day one; doesn't compile to the browser without extra work; overkill for a few hundred notes.
  - **eframe's built-in key-value persistence.** Pros: zero code, and it maps to Local Storage on the web. Cons: it saves on a timer (every 30 s) and on exit, so a crash can lose the last notes; not meant for large data; hard to inspect.
  - **A JSON file for the library + WAV files for audio (chosen).** Pros: human-readable (`library.json` opens in any editor), trivial to back up, serde does the work, easy to test with a temp folder. Cons: the whole file is rewritten on each change (fine at this size); no queries (search arrives in Phase 3 and may need an index).
- **Decision:** `lectio_core::storage::JsonFileStore` writes `library.json` and keeps audio in `recordings/<session-id>.wav` inside the OS app-data folder (`~/Library/Application Support/org.Lectio.Lectio` on macOS; override with the `LECTIO_DATA_DIR` environment variable). It saves after **every** change, using write-to-temp-then-rename so a crash never leaves a half-written file (a rename on the same disk is atomic: it either fully happens or doesn't). The file carries `schema_version: 1`. A file from a newer version is refused and left untouched; a corrupt file is moved to `library.json.broken-<time>` and the app starts fresh and says where the old file is.
- **Consequences:** The app only talks to the `LibraryStore` trait, so a web build can add an IndexedDB store without touching the views. If Phase 3 search gets slow, SQLite can replace the JSON file behind the same trait.

## ADR-004: Audio — `cpal` for microphone and speaker, stored as 16 kHz mono 16-bit WAV (`hound`)
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** Phase 1 needs record, pause, resume, stop and playback with exact seeking. Phase 3 will transcribe the audio.
- **Options considered:**
  - **`rodio` for playback.** Pros: high-level and easy. Cons: seeking support depends on the decoder, and I'd still need `cpal` for recording, so two audio stacks.
  - **Compressed format (Opus/MP3).** Pros: files ~10× smaller. Cons: needs an encoder dependency, and seeking inside compressed audio is approximate. That is risky for "click a note, hear that exact moment".
  - **`cpal` for both directions + uncompressed WAV (chosen).** Pros: one low-level library; seeking is exact because the position is just a sample index. Cons: more code (resampling, sample formats); bigger files.
- **Decision:** Record from the default microphone with `cpal`, mix down to mono, downsample to **16 kHz**, and write 16-bit WAV with `hound`. 16 kHz is enough for speech and is the input rate open-source speech-to-text models like Whisper expect, so Phase 3 needs no conversion. Size: ~115 MB per hour (48 kHz stereo float would be ~1.4 GB). Playback loads the file into memory and plays it through `cpal`, using linear interpolation to match the speaker's rate.
- **Consequences:** Exact seeks and Whisper-ready files. Storage size may need compression later. The downsampler is a simple box filter (it averages each group of input samples); fine for speech, not hi-fi.

## ADR-005: The recording clock is the audio itself (sample count), not a timer
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** Every note stores `offset_ms`, its position in the recording. If that number drifts from the audio, "click a note to hear that moment" breaks.
- **Options considered:**
  - **Wall-clock timer (`Instant::now()` minus start, minus pauses).** Pros: simple. Cons: drifts from the audio when the sound card delivers samples late, when the laptop sleeps, or when pause/resume timing is slightly off; errors add up over a 2-hour lecture.
  - **Count the samples written to the file (chosen).** Pros: the offset *is* the position in the file by definition; pauses add no samples, so they add no time. Cons: the clock only moves when audio arrives (a dead microphone shows 0:00, which is honest).
- **Decision:** `RecordingClock` (`crates/lectio-core/src/clock.rs`) converts the writer thread's sample count to ms with `timeline::samples_to_ms`, rounding *down* so a note never points past the recorded audio. The count only grows (`with_total_samples` ignores stale lower readings). Also, `NoteDraft` stamps a note with the moment the student **started typing**, not when they pressed Enter, because that is when they heard the thing worth writing down.
- **Consequences:** Seek accuracy is one sample (1/16 000 s). Covered by `tests/clock_tests.rs` and `tests/seek_tests.rs`.

## ADR-006: State management — immutable `Library` values + an `Action` enum applied in one place
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** egui is immediate-mode: views are functions that run every frame. If views changed data directly, a bug could leave a half-applied change, and the code would be hard to test.
- **Options considered:**
  - **Views mutate a shared `&mut Library`.** Pros: least code. Cons: side effects spread across every view; hard to know when to save.
  - **Unidirectional data flow (chosen), the same idea as Redux/Elm.** Views read a read-only `View` snapshot and push `Action`s (`src/action.rs`); `LectioApp::apply` (`src/app.rs`) performs them. Every model change is a method like `Library::with_note(...)` that returns a **new** `Library` and leaves the old one unchanged.
- **Decision:** The second option. After applying a frame's actions, the app compares the new library with the old one and saves only if it changed.
- **Consequences:** The model is testable without a UI (the 20 model tests use plain values); undo later is "keep the previous `Library`". Cost: each change clones the library (microseconds at this size).

## ADR-007: Labels have a fixed identity and a renameable display name
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** The spec has four one-tap labels that the student can rename, and Phase 4 must collect every `Task` label.
- **Options considered:** store the label's *name* on each note (renaming breaks old notes and the Phase 4 task list) / store a **kind** (`LabelKind::Task`) on notes and keep the names in one `LabelSet` (chosen).
- **Decision:** Notes store `LabelKind`s; `Library::labels()` maps each kind to its current name. A one-tap label with no text typed creates a "marked moment" note; tapping a label while typing saves the typed note with that label.
- **Consequences:** Renaming is one change in one place; the Phase 4 task list can filter by `LabelKind::Task` whatever the student calls it. Fully custom labels would later need a new id type.

## ADR-008: Real-time-safe capture — lock-free SPSC ring buffer (`rtrb`), conversion on the writer thread
- **Date:** 2026-11-19
- **Status:** accepted (changes the recording pipeline from ADR-004; the stored format is unchanged)
- **Context:** P-006. The OS calls the microphone callback on a *real-time* thread with a hard deadline (512 frames at 48 kHz =
  10.7 ms). If it misses the deadline, macOS reports a processor overload and audio can glitch. The first version allocated
  hundreds of times per callback and used an unbounded `mpsc` channel. The rule for real-time code is **no heap allocation, no
  locks, no blocking**, because each of those can occasionally take an unbounded amount of time.
- **Options considered:**
  - **Keep `mpsc`, but bounded (`sync_channel` + `try_send`).** Pros: tiny change. Cons: still allocates a `Vec` per callback
    to send; the channel's internals are not designed or documented as real-time-safe.
  - **`Mutex<Vec<f32>>` shared buffer.** Cons: if the writer holds the lock when the callback runs, the callback waits. That
    is *priority inversion*: a low-priority thread blocks a high-priority one.
  - **Lock-free single-producer/single-consumer ring buffer, `rtrb` (chosen).** A fixed-size circular array with two atomic
    indexes: the callback is the only writer, the writer thread the only reader, so no locks are needed and nothing is
    allocated after creation. Pros: real-time-safe by design, made for audio, small dependency. Cons: fixed capacity, so if the
    writer thread stalls for longer than the buffer holds, samples are dropped (we count them, and the UI shows the count).
  - **`ringbuf` crate.** Similar SPSC design with more features (blocking wrappers, more traits) that we don't need. `rtrb`'s
    chunk API (`write_chunk_uninit` + `fill_from_iter`) copies and converts in one pass with no temporary buffer.
- **Decision:** The callback converts each raw sample to `f32` and copies it into a 2-second `rtrb` ring (whole frames only),
  and records stats in atomics: frames captured, samples dropped, callbacks, slowest callback time. `Xrun` errors increment an
  atomic counter. Downmix + downsample (`lectio_core::resample::SpeechConverter`, tested to give bit-identical output however
  the stream is chunked) and WAV writing run on the writer thread, which polls the ring every 10 ms. Lost audio appears in the
  capture view only when it is above zero, and `--self-test-audio <seconds>` prints all counters.
- **Consequences:**
  - Measured on the MacBook Air: slowest callback 29 µs (release) / 116 µs (debug) against a 10 667 µs deadline; 0 drops,
    0 overruns over 5 s (PROBLEMS P-006).
  - The 2 s ring costs 384 KB for 48 kHz mono f32 (768 KB for stereo), allocated once at start.
  - The writer thread now does more CPU work, but it isn't real-time, so a slow moment there only uses ring space.
  - The same pattern (real-time thread ↔ lock-free ring ↔ normal thread) is what a future web recorder (an `AudioWorklet`)
    would need too.

## ADR-009: Platform targets — one Rust codebase for Windows, macOS, Linux and the web
> Numbering note: the request called this "ADR-008", but ADR-008 was already used for the real-time capture pipeline (P-006),
> and ADR numbers are never reused, so this is ADR-009.
- **Date:** 2026-11-19
- **Status:** accepted. **Extends ADR-002** (same language, UI library and core crate) and **supersedes** ADR-002's
  "desktop first" trade-off and its mobile notes. **Extends ADR-003** (the same `LibraryStore` trait now has an IndexedDB
  implementation) and **ADR-004** (same 16 kHz WAV format everywhere).
- **Context:** Ship Lectio on Windows, macOS, Linux and the web from **one codebase**,
  and **stay on Rust**. It's a language I'm learning on purpose, so I don't stay in my comfort zone with TypeScript. Every
  phase must build on all four targets, and CI must prove it. Three parts of the app depend on the platform: **storage**,
  **recording** and **playback**. Mobile is not a target for now.
- **How the platform is chosen:** at **compile time** with `#[cfg(target_arch = "wasm32")]`. `src/platform/native/` is
  compiled for Windows, macOS and Linux; `src/platform/web/` only for the browser. The app talks to them through three traits
  in `src/platform/mod.rs`: `Platform` (storage + start a recording + open a recording), `AudioCapture` and `AudioPlayback`.
  The app holds a `Box<dyn Platform>`, a *trait object*: a pointer to "some type that implements `Platform`", so `app.rs`
  never names cpal or IndexedDB. Alternatives: generics (`LectioApp<P: Platform>`) avoid a tiny dynamic call but spread a
  type parameter through every view; Cargo features (`--features web`) can be turned on for the wrong target by mistake,
  while `cfg(target_arch)` can't.
- **Options per platform-specific part (web side; native is unchanged):**
  - **Storage** (behind `LibraryStore`, ADR-003)
    - *IndexedDB (chosen).* The browser's built-in database: stores strings and binary blobs, works in every current browser,
      allows hundreds of MB. Cons: asynchronous, callback-based API (wrapped in `src/platform/web/idb.rs`).
    - *OPFS (Origin Private File System).* Gives real files, so the web layout would match native (`library.json` +
      `recordings/*.wav`). Cons: the fast synchronous file access works only inside a Web Worker, and some browsers added
      writable files late, so support is less even.
    - *localStorage / eframe persistence.* Synchronous and trivial, but strings only and about 5 MB per site: fine for the JSON
      library, impossible for audio.
  - **Recording** (behind `AudioCapture`)
    - *`getUserMedia` + `AudioWorklet` posting batches (chosen).* The worklet (`web/lectio-capture-worklet.js`) runs on the
      browser's audio thread, mixes to mono and posts a `Float32Array` every 4096 samples (≈12 per second); Rust converts with
      the same `SpeechConverter` as native (ADR-008). Gives raw PCM, so the 16 kHz WAV and the sample-based clock (ADR-005) work
      exactly as on native.
    - *`AudioWorklet` + `SharedArrayBuffer` ring.* The browser version of ADR-008's lock-free ring: no allocation on the audio
      thread at all. Cons: `SharedArrayBuffer` needs the page served with special headers (COOP/COEP, which make the page
      "cross-origin isolated"), which simple static hosts such as GitHub Pages can't set. A good upgrade later.
    - *`MediaRecorder`.* Easiest, but it gives compressed Opus/WebM (MP4 on Safari). We'd need a decoder to get PCM for WAV and
      the clock, and time is only known per chunk.
    - *`ScriptProcessorNode`.* Deprecated, and runs on the main thread, where egui also draws, so it glitches.
    - *cpal's browser backends.* Would keep one audio API everywhere, but in cpal 0.18 **neither** browser backend records:
      both `webaudio` and `audioworklet` return `ErrorKind::UnsupportedOperation` from `build_input_stream_raw`. The
      `audioworklet` one also needs WebAssembly threads (`-Zbuild-std` with atomics on nightly Rust) and the COOP/COEP headers.
  - **Playback** (behind `AudioPlayback`)
    - *Web Audio `AudioBufferSourceNode` (chosen).* Decoded samples in an `AudioBuffer`; `start(0, offset)` begins at an exact
      offset, and the position comes from the audio clock `AudioContext.currentTime`. Seeking = start a new node.
    - *`<audio>` element + blob URL.* Less code, but seek accuracy depends on the browser's decoder and the position updates
      only a few times per second.
- **Decision:** the chosen options above. `lectio-core` stays platform-free and is checked for `wasm32-unknown-unknown` in CI.
  The only wasm-specific line is a Cargo setting that gives `uuid` the browser's random numbers (P-007). Rules that both
  stores need moved into the core (`storage::interpret_saved`, tested). The web build uses **trunk** (`index.html`,
  `Trunk.toml`), and `index.html#sample` opens sample data. GitHub Actions builds, tests and lints on ubuntu/windows/macos and
  builds the web version.
- **Consequences / trade-offs:**
  - Easy: one language, one UI, one set of rules on all four targets; the core's 60 tests cover all of them.
  - The browser keeps a recording **in memory until Stop** (≈115 MB per hour), then stores it in IndexedDB. If the tab
    crashes mid-lecture, the notes are safe (saved on every change) but that audio is lost. Native writes to disk as it
    records. Fix later: store chunks every few seconds.
  - The worklet still allocates a small `Float32Array` per batch on the browser's audio thread (12 per second; JavaScript's
    garbage collector handles it). The `SharedArrayBuffer` option above would remove that; not needed until a measurement
    says so (P-006's lesson).
  - Web download: 7.1 MB of WebAssembly for the release build (before the web server's gzip/brotli compression).
  - Async browser APIs behind sync traits: the web side starts work with `spawn_local` and the app polls `is_ready()` /
    `take_error()` each frame. That's simple, but errors show up a frame later, not at the call.
  - Verified locally on this Mac: native build/tests, Windows type-check, wasm build, and a scripted Chrome run. Linux is proven
    only by CI (P-011). Safari and Firefox are untested.
  - Mobile is out of scope; eframe's Android support and the core crate keep the door open.
  - **Accessibility gap on the web (P-012):** on desktop, egui exposes every widget to screen readers through AccessKit; in
    the browser, eframe 0.37 draws on a single `<canvas>` with no DOM accessibility tree, so screen readers can't navigate
    Lectio there. This is the biggest cost of choosing egui for the web, and it matters for the secondary user (SPEC). It
    must be addressed before the web version is called accessible (options in P-012).

## ADR-010: PDF slide rendering — `hayro` (pure Rust) on all four targets
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** Phase 2 shows imported PDF slides next to the notes, on Windows, macOS, Linux **and** in the browser
  (ADR-009). Showing a slide means *rasterizing* a page: turning the PDF's vector drawing commands into pixels for a texture.
  Rust has no PDF renderer in the standard library, so this is a library choice.
- **Options considered:**
  - **`hayro` 0.9 (chosen).** A PDF rasterizer written entirely in Rust (`#![forbid(unsafe_code)]`), MIT/Apache-2.0, with
    the 14 standard PDF fonts built in. Pros: the **same code on all four targets**, nothing to download or ship, and the
    regular `cargo build` / `trunk build` just work. Cons: young (0.x), CPU-only, no work on speed yet, and no
    password-protected PDFs or a few rare features (blend groups, knockout).
  - **`pdfium-render` (Google's PDFium, the engine inside Chrome).** Pros: very mature and fast. Cons: PDFium is a C++ library
    that `pdfium-render` loads at runtime. We would have to ship a prebuilt binary per OS (≈5 MB each, downloaded in CI) and
    a separate `pdfium.wasm` with JavaScript glue for the web: four packaging paths to maintain.
  - **pdf.js on the web (Mozilla, used by Firefox).** Pros: the most battle-tested renderer in browsers. Cons: JavaScript
    (≈1–2 MB plus a worker), so we'd need JS↔Rust glue, *and* a second renderer for native. Two renderers mean slides can look
    different per platform, and bugs must be fixed twice.
  - **MuPDF (`mupdf` crate).** Excellent, but **AGPL**-licensed: using it would force Lectio's whole source under the AGPL
    (or a paid licence). That conflicts with the MIT licence in SPEC. Rejected.
  - **`pdf` (pdf-rs) + `pdf_render`.** The parser is solid, but its renderer is incomplete. **Poppler**: GPL, and no web build.
- **Evidence (probe before deciding):** a 2-page test deck (`assets/sample-slides.pdf`: coloured shapes and Helvetica
  text) rendered correctly at 1.5× (1263×892 px) in **4.6 ms** (first page, includes font setup) and **0.5 ms** (second page)
  in a release build; `cargo check --target wasm32-unknown-unknown` passed. Picture: `shots/probe-hayro-page1.png`.
- **Decision:** `hayro` renders slides on every target, behind one small module (`src/media/slides.rs`). No `Platform` trait
  is needed for *rendering*, because the code is the same everywhere. Only *where it runs* differs: native renders on a
  background thread; the browser has no threads in our build, so it renders one page per frame on the main thread. Pages are
  cached as textures.
- **Consequences:**
  - One renderer, one set of bugs, no binaries to ship. Measured cost: the release `.wasm` grew from **7.06 MB to 12.03 MB**
    (4.76 MB after gzip) for all of Phase 2. Most of that is hayro's built-in fonts and CJK character maps (`embed-fonts`,
    `embed-cmaps`), which make PDFs without embedded fonts render correctly. Turning `embed-cmaps` off later would shrink
    it, but Chinese/Japanese/Korean PDFs might then render wrongly.
  - Big, image-heavy lecture decks may render slower than in PDFium, and in the browser a slow page freezes the UI briefly.
    Mitigation: render only the visible page at a fixed size, and cache it.
  - Escape hatch: if real-world decks fail, swap the renderer *inside* `media/slides.rs` (e.g. PDFium on native, pdf.js on the
    web) without touching the rest of the app.

## ADR-011: Large files (PDFs, photos, sketches) are blobs stored outside the library
- **Date:** 2026-11-19
- **Status:** accepted. Extends ADR-003 (storage) and ADR-009 (platform boundary).
- **Context:** Phase 2 brings files that are 1,000–100,000× bigger than a note: a lecture PDF is often 5–60 MB, a phone photo
  3–6 MB. ADR-003 rewrites `library.json` after *every* change, so putting bytes inside it (e.g. base64) would make every
  keystroke rewrite tens of megabytes, and in the browser the whole library is read before the app can start.
- **Options considered:**
  - **Bytes inside `library.json` (base64).** Simplest, but saves get slow and the file 33% bigger than the data. Rejected.
  - **SQLite for everything.** Handles blobs well on native, but it's a new storage engine for one feature, and not on the web.
  - **Separate blob store keyed by a random id (chosen).** The library stores a `BlobKey` (`<uuid>.pdf|png|jpg`) plus small
    facts (page count, image width/height); the bytes go to `<data>/blobs/<key>` on native (`lectio_core::storage::FileBlobStore`,
    atomic write like ADR-003) and to a new IndexedDB table `blobs` on the web (database version 1 → 2).
- **Decision:** the third option. Blob keys are *validated types*: `BlobKey::parse` only accepts a canonical UUID and one of
  three extensions, both when a key is created and when `library.json` is read back. Because the key becomes a file name, a
  hand-edited key like `../../secret.pdf` can't reach outside the folder (*path traversal*: using `../` in a name to escape a
  directory). Imported files are checked by their first bytes (*magic numbers*), not their names, with limits of 100 MB per
  PDF and 25 MB per image (`material::check_file`). When notes or decks are deleted, `material::orphaned_blobs(before, after)`
  lists blobs nothing refers to any more, and the app deletes them.
- **Consequences:**
  - `library.json` stays small (tested: a 400-page deck + a 4000×3000 photo add < 3 KB of JSON), so saving stays instant.
  - Blobs are written *before* the library refers to them, so a crash can leave an unused blob but never a note pointing at
    nothing. Orphan clean-up only runs on delete, so an interrupted import may leave a blob behind (small, rare; accepted).
  - Backups must copy the `blobs/` folder too (native), and the browser's storage quota now matters more (ADR-009).
  - Schema version 2: Phase 1 files load (new fields default to empty), but a Phase 1 app refuses a Phase 2 file instead of
    silently dropping slides and photos (ADR-003's rule).

## ADR-012: Getting material in — file picker (`rfd`), browser camera, and sketches drawn with `tiny-skia`
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** Phase 2 needs to (1) pick a PDF or photo file on all four targets, (2) take a photo "where the platform allows
  it", and (3) save a freehand sketch as an image.
- **File picking. Options:** `rfd` (Rusty File Dialog) / hand-written per-OS dialogs / egui's drag-and-drop only.
  **Chosen: `rfd` 0.18.** It opens the native dialog on Windows and macOS, the desktop portal on Linux (it loads D-Bus at
  run time, so CI needs no new system packages), and an `<input type="file">` in the browser. Behind
  `Platform::pick_file`, which returns a `Pending` result, because in the browser reading a file is asynchronous.
- **Camera. Options:** (a) native webcam with `nokhwa` on desktop + browser camera on the web; (b) **browser camera only**;
  desktop apps use the file picker (chosen); (c) only the file picker everywhere, using the HTML `capture` attribute so phones
  open their camera.
  **Chosen: (b).** In the browser, `getUserMedia({video})` gives a live preview inside the app and a snapshot button. That
  works with laptop webcams and on phones and tablets (rear camera preferred). Native webcam capture (a) would add a C-backed
  camera library with three OS backends (AVFoundation, Media Foundation, V4L2), a macOS camera permission for an unsigned
  binary, and more CI risk. And a laptop webcam faces the student, not the board, while phone photos reach the desktop as
  files anyway. `Platform::open_camera()` returns `None` on native, so the "Take photo" button simply isn't shown there.
- **Sketches. Options:** save the strokes as vector data and redraw them / rasterize with egui's own tessellator / rasterize
  with `tiny-skia` (chosen). The spec says sketches are *saved as images*, so the strokes are drawn into a PNG with
  `tiny-skia` (a pure-Rust port of a subset of Skia, Chrome's 2D graphics library): anti-aliased lines, same output on every
  target, and unit-testable without a window. Photos are stored exactly as picked (no re-compression); only the camera
  snapshot is encoded, as JPEG.
- **Consequences:** two new pure-Rust dependencies (`rfd`, `tiny-skia`) and `image` now decodes JPEG too; no native camera.
  Phone photos are often stored sideways with an EXIF "rotate me" tag, so the decoder applies the EXIF orientation before
  showing a photo.
