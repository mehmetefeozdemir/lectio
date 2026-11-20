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

## ADR-002: Documentation as a first-class output
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** I want to ship a full app this term and be able to explain every part of it: why each technology was
  chosen, what broke, and how the pieces fit together.
- **Options considered:**
  - Write the docs at the end: fast while building, but the reasons behind decisions are forgotten by then.
  - Keep three living documents next to the code (chosen): `DECISIONS.md` (one ADR per real choice, with the alternatives),
    `PROBLEMS.md` (every failure: symptom, exact error, root cause, fix, lesson) and `ARCHITECTURE.md` (structure, data model
    and main flows, with Mermaid diagrams), updated while I work.
- **Decision:** The second option. Each phase ends only when the app builds, the tests pass and these documents are current.
- **Consequences:** Slower phases, but every decision and bug can be traced and explained later.

## ADR-003: Tech stack — Rust, with egui (eframe) for the UI and a UI-free core crate
- **Date:** 2026-11-19
- **Status:** accepted; **extended by ADR-010** (platform targets). ADR-010 keeps the language, UI library and core crate, and
  replaces the "desktop first" trade-off and the mobile notes below.
- **Context:** Phase 1 needs a window, microphone recording, audio playback with seeking, and storage that works with no account and no network. The spec asks for a stack that can later reach web and mobile and is local-first. Rust is already installed on my machine, and my automated checks test a desktop build by asking it for a screenshot.
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

## ADR-004: Local-first storage — one JSON file plus a folder of WAV files, behind a `LibraryStore` trait
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** The spec says data must survive a restart with no account. Phase 1 data is small (courses, sessions, notes) except for the audio, which is large.
- **Options considered:**
  - **SQLite (via `rusqlite`).** Pros: queries, transactions, scales to many sessions. Cons: a schema and migrations from day one; doesn't compile to the browser without extra work; overkill for a few hundred notes.
  - **eframe's built-in key-value persistence.** Pros: zero code, and it maps to Local Storage on the web. Cons: it saves on a timer (every 30 s) and on exit, so a crash can lose the last notes; not meant for large data; hard to inspect.
  - **A JSON file for the library + WAV files for audio (chosen).** Pros: human-readable (`library.json` opens in any editor), trivial to back up, serde does the work, easy to test with a temp folder. Cons: the whole file is rewritten on each change (fine at this size); no queries (search arrives in Phase 3 and may need an index).
- **Decision:** `lectio_core::storage::JsonFileStore` writes `library.json` and keeps audio in `recordings/<session-id>.wav` inside the OS app-data folder (`~/Library/Application Support/org.Lectio.Lectio` on macOS; override with the `LECTIO_DATA_DIR` environment variable). It saves after **every** change, using write-to-temp-then-rename so a crash never leaves a half-written file (a rename on the same disk is atomic: it either fully happens or doesn't). The file carries `schema_version: 1`. A file from a newer version is refused and left untouched; a corrupt file is moved to `library.json.broken-<time>` and the app starts fresh and says where the old file is.
- **Consequences:** The app only talks to the `LibraryStore` trait, so a web build can add an IndexedDB store without touching the views. If Phase 3 search gets slow, SQLite can replace the JSON file behind the same trait.

## ADR-005: Audio — `cpal` for microphone and speaker, stored as 16 kHz mono 16-bit WAV (`hound`)
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** Phase 1 needs record, pause, resume, stop and playback with exact seeking. Phase 3 will transcribe the audio.
- **Options considered:**
  - **`rodio` for playback.** Pros: high-level and easy. Cons: seeking support depends on the decoder, and I'd still need `cpal` for recording, so two audio stacks.
  - **Compressed format (Opus/MP3).** Pros: files ~10× smaller. Cons: needs an encoder dependency, and seeking inside compressed audio is approximate. That is risky for "click a note, hear that exact moment".
  - **`cpal` for both directions + uncompressed WAV (chosen).** Pros: one low-level library; seeking is exact because the position is just a sample index. Cons: more code (resampling, sample formats); bigger files.
- **Decision:** Record from the default microphone with `cpal`, mix down to mono, downsample to **16 kHz**, and write 16-bit WAV with `hound`. 16 kHz is enough for speech and is the input rate open-source speech-to-text models like Whisper expect, so Phase 3 needs no conversion. Size: ~115 MB per hour (48 kHz stereo float would be ~1.4 GB). Playback loads the file into memory and plays it through `cpal`, using linear interpolation to match the speaker's rate.
- **Consequences:** Exact seeks and Whisper-ready files. Storage size may need compression later. The downsampler is a simple box filter (it averages each group of input samples); fine for speech, not hi-fi.

## ADR-006: The recording clock is the audio itself (sample count), not a timer
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** Every note stores `offset_ms`, its position in the recording. If that number drifts from the audio, "click a note to hear that moment" breaks.
- **Options considered:**
  - **Wall-clock timer (`Instant::now()` minus start, minus pauses).** Pros: simple. Cons: drifts from the audio when the sound card delivers samples late, when the laptop sleeps, or when pause/resume timing is slightly off; errors add up over a 2-hour lecture.
  - **Count the samples written to the file (chosen).** Pros: the offset *is* the position in the file by definition; pauses add no samples, so they add no time. Cons: the clock only moves when audio arrives (a dead microphone shows 0:00, which is honest).
- **Decision:** `RecordingClock` (`crates/lectio-core/src/clock.rs`) converts the writer thread's sample count to ms with `timeline::samples_to_ms`, rounding *down* so a note never points past the recorded audio. The count only grows (`with_total_samples` ignores stale lower readings). Also, `NoteDraft` stamps a note with the moment the student **started typing**, not when they pressed Enter, because that is when they heard the thing worth writing down.
- **Consequences:** Seek accuracy is one sample (1/16 000 s). Covered by `tests/clock_tests.rs` and `tests/seek_tests.rs`.

## ADR-007: State management — immutable `Library` values + an `Action` enum applied in one place
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** egui is immediate-mode: views are functions that run every frame. If views changed data directly, a bug could leave a half-applied change, and the code would be hard to test.
- **Options considered:**
  - **Views mutate a shared `&mut Library`.** Pros: least code. Cons: side effects spread across every view; hard to know when to save.
  - **Unidirectional data flow (chosen), the same idea as Redux/Elm.** Views read a read-only `View` snapshot and push `Action`s (`src/action.rs`); `LectioApp::apply` (`src/app.rs`) performs them. Every model change is a method like `Library::with_note(...)` that returns a **new** `Library` and leaves the old one unchanged.
- **Decision:** The second option. After applying a frame's actions, the app compares the new library with the old one and saves only if it changed.
- **Consequences:** The model is testable without a UI (the 20 model tests use plain values); undo later is "keep the previous `Library`". Cost: each change clones the library (microseconds at this size).

## ADR-008: Labels have a fixed identity and a renameable display name
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** The spec has four one-tap labels that the student can rename, and Phase 4 must collect every `Task` label.
- **Options considered:** store the label's *name* on each note (renaming breaks old notes and the Phase 4 task list) / store a **kind** (`LabelKind::Task`) on notes and keep the names in one `LabelSet` (chosen).
- **Decision:** Notes store `LabelKind`s; `Library::labels()` maps each kind to its current name. A one-tap label with no text typed creates a "marked moment" note; tapping a label while typing saves the typed note with that label.
- **Consequences:** Renaming is one change in one place; the Phase 4 task list can filter by `LabelKind::Task` whatever the student calls it. Fully custom labels would later need a new id type.

## ADR-009: Real-time-safe capture — lock-free SPSC ring buffer (`rtrb`), conversion on the writer thread
- **Date:** 2026-11-19
- **Status:** accepted (changes the recording pipeline from ADR-005; the stored format is unchanged)
- **Context:** P-007. The OS calls the microphone callback on a *real-time* thread with a hard deadline (512 frames at 48 kHz =
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
    0 overruns over 5 s (PROBLEMS P-007).
  - The 2 s ring costs 384 KB for 48 kHz mono f32 (768 KB for stereo), allocated once at start.
  - The writer thread now does more CPU work, but it isn't real-time, so a slow moment there only uses ring space.
  - The same pattern (real-time thread ↔ lock-free ring ↔ normal thread) is what a future web recorder (an `AudioWorklet`)
    would need too.

## ADR-010: Platform targets — one Rust codebase for Windows, macOS, Linux and the web
> Numbering note: the request called this "ADR-009", but ADR-009 was already used for the real-time capture pipeline (P-007),
> and ADR numbers are never reused, so this is ADR-010.
- **Date:** 2026-11-19
- **Status:** accepted. **Extends ADR-003** (same language, UI library and core crate) and **supersedes** ADR-003's
  "desktop first" trade-off and its mobile notes. **Extends ADR-004** (the same `LibraryStore` trait now has an IndexedDB
  implementation) and **ADR-005** (same 16 kHz WAV format everywhere).
- **Context:** My decision: ship Lectio on Windows, macOS, Linux and the web from **one codebase**,
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
  - **Storage** (behind `LibraryStore`, ADR-004)
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
      the same `SpeechConverter` as native (ADR-009). Gives raw PCM, so the 16 kHz WAV and the sample-based clock (ADR-006) work
      exactly as on native.
    - *`AudioWorklet` + `SharedArrayBuffer` ring.* The browser version of ADR-009's lock-free ring: no allocation on the audio
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
  The only wasm-specific line is a Cargo setting that gives `uuid` the browser's random numbers (P-008). Rules that both
  stores need moved into the core (`storage::interpret_saved`, tested). The web build uses **trunk** (`index.html`,
  `Trunk.toml`), and `index.html#sample` opens sample data. GitHub Actions builds, tests and lints on ubuntu/windows/macos and
  builds the web version (P-013 for where the file lives).
- **Consequences / trade-offs:**
  - Easy: one language, one UI, one set of rules on all four targets; the core's 60 tests cover all of them.
  - The browser keeps a recording **in memory until Stop** (≈115 MB per hour), then stores it in IndexedDB. If the tab
    crashes mid-lecture, the notes are safe (saved on every change) but that audio is lost. Native writes to disk as it
    records. Fix later: store chunks every few seconds.
  - The worklet still allocates a small `Float32Array` per batch on the browser's audio thread (12 per second; JavaScript's
    garbage collector handles it). The `SharedArrayBuffer` option above would remove that; not needed until a measurement
    says so (P-007's lesson).
  - Web download: 7.1 MB of WebAssembly for the release build (before the web server's gzip/brotli compression).
  - Async browser APIs behind sync traits: the web side starts work with `spawn_local` and the app polls `is_ready()` /
    `take_error()` each frame. That's simple, but errors show up a frame later, not at the call.
  - Verified locally on this Mac: native build/tests, Windows type-check, wasm build, and a scripted Chrome run. Linux is proven
    only by CI (P-012). Safari and Firefox are untested.
  - Mobile is out of scope; eframe's Android support and the core crate keep the door open.
  - **Accessibility gap on the web (P-013):** on desktop, egui exposes every widget to screen readers through AccessKit; in
    the browser, eframe 0.37 draws on a single `<canvas>` with no DOM accessibility tree, so screen readers can't navigate
    Lectio there. This is the biggest cost of choosing egui for the web, and it matters for the secondary user (SPEC). It
    must be addressed before the web version is called accessible (options in P-013).

## ADR-011: PDF slide rendering — `hayro` (pure Rust) on all four targets
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** Phase 2 shows imported PDF slides next to the notes, on Windows, macOS, Linux **and** in the browser
  (ADR-010). Showing a slide means *rasterizing* a page: turning the PDF's vector drawing commands into pixels for a texture.
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

## ADR-012: Large files (PDFs, photos, sketches) are blobs stored outside the library
- **Date:** 2026-11-19
- **Status:** accepted. Extends ADR-004 (storage) and ADR-010 (platform boundary).
- **Context:** Phase 2 brings files that are 1,000–100,000× bigger than a note: a lecture PDF is often 5–60 MB, a phone photo
  3–6 MB. ADR-004 rewrites `library.json` after *every* change, so putting bytes inside it (e.g. base64) would make every
  keystroke rewrite tens of megabytes, and in the browser the whole library is read before the app can start.
- **Options considered:**
  - **Bytes inside `library.json` (base64).** Simplest, but saves get slow and the file 33% bigger than the data. Rejected.
  - **SQLite for everything.** Handles blobs well on native, but it's a new storage engine for one feature, and not on the web.
  - **Separate blob store keyed by a random id (chosen).** The library stores a `BlobKey` (`<uuid>.pdf|png|jpg`) plus small
    facts (page count, image width/height); the bytes go to `<data>/blobs/<key>` on native (`lectio_core::storage::FileBlobStore`,
    atomic write like ADR-004) and to a new IndexedDB table `blobs` on the web (database version 1 → 2).
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
  - Backups must copy the `blobs/` folder too (native), and the browser's storage quota now matters more (ADR-010).
  - Schema version 2: Phase 1 files load (new fields default to empty), but a Phase 1 app refuses a Phase 2 file instead of
    silently dropping slides and photos (ADR-004's rule).

## ADR-013: Getting material in — file picker (`rfd`), browser camera, and sketches drawn with `tiny-skia`
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

## ADR-014: UI design system — tokens in one module, bundled fonts and icons, light and dark themes
- **Date:** 2026-11-19
- **Status:** accepted. Replaces the ad-hoc colours and sizes from Phase 1 (`ui/theme.rs` constants).
- **Context:** The Phase 1–2 screens worked but looked like a default egui demo: grey buttons everywhere, weak hierarchy,
  "Labels / Delete" repeated on every note, no icons. A study app is used for hours, often in a dim lecture hall, so it needs
  a calm, readable look and a dark theme. egui has no design system of its own, just a `Style` with hundreds of knobs, so
  without shared tokens every view picks its own colours.
- **Options considered:**
  - **Tweak egui's default style per view.** Cheapest, but it's how the current inconsistency happened.
  - **A third-party egui theme crate.** Fast, but it would give Lectio someone else's look, and these crates trail egui
    releases (several still target older egui versions).
  - **My own token module, `src/ui/theme.rs` (chosen).** A *design token* is a named design value (a colour, size or
    spacing) that every view reads instead of hard-coding numbers. Everything lives in one file and is unit-tested.
- **Decision:**
  - **Type:** Inter (Regular + SemiBold) for the UI and JetBrains Mono only for timestamps, both embedded with
    `include_bytes!` (SIL OFL licences in `assets/fonts/`). Scale **12 / 13 / 15 / 18 / 24** px; two weights.
  - **Space and shape:** spacing 4/8/12/16/24/32, corner radius 6 (controls, chips) and 10 (cards, panels), 1 px borders, and
    one soft shadow used only for popovers and menus (windows and panels are flat).
  - **Colour:** neutral greys plus one accent (indigo), and a desaturated pair per label (soft fill + dark text in light mode,
    dark fill + light text in dark mode). A unit test computes the **WCAG contrast ratio** (relative luminance (L1 + 0.05) /
    (L2 + 0.05); AA needs 4.5:1 for normal text, 3:1 for graphics) for every text/background pair in both themes.
  - **Themes:** light and dark; the app follows the OS setting (`ThemePreference::System`) and a toolbar button cycles
    System → Light → Dark.
  - **Icons:** the Phosphor icon font (MIT). I bundle the official font file and define the ~35 codepoints I use in
    `src/ui/icons.rs` rather than depend on an egui wrapper crate, because the wrapper's current release targets an older
    egui. Icons are drawn at 16 px (toolbar 18 px). Every icon-only button gets a tooltip and an explicit accessible name
    (`WidgetInfo`), so a screen reader says "Import PDF", not a private-use character.
  - **Shapes, not just colours:** each label has its own icon (star, arrow, check box, book) on chips, and its own marker
    shape on the timeline (diamond, circle, square, triangle), so labels can be told apart without colour.
- **Consequences:** about 1.6 MB of fonts embedded in every build (desktop and web); one place to change the look; a test
  fails if a colour change breaks contrast. Views must use the tokens and the helpers in `src/ui/widgets.rs`. The theme
  choice is not saved yet; each start follows the OS. (Since ADR-015 the choice is saved in the library.)


## ADR-015: Session workflow: a home page, a four-panel workspace, and saved layout (schema v3)
- **Date:** 2026-11-19
- **Status:** accepted. Builds on ADR-014 (same design system); replaces the course sidebar as the way into a session.
- **Context:** Students who already take lecture notes on a computer expect a familiar flow: a list of all their sessions
  first, then one session with the slides, the notes and the audio side by side. In Lectio they also want to place slides
  *into* the notes as they come up in the lecture, and to see where in the audio the important parts are. The Phase 1–2
  screen opened straight into one session with a course tree on the left, and slides were only a side panel.
- **Options considered:**
  - **Keep the single screen and add more side panels.** The least work, but there is still no place to see every session at
    once, and no place for trash or (later) tasks and search.
  - **A home page plus a fixed workspace.** Better, but a student on a laptop at a small desk can't hide what they don't need.
  - **Home page, a workspace of hideable panels, and tabs on narrow windows (chosen).**
- **Decision:**
  - **Screens** (`action::Screen`): Home (all sessions, newest first: title, course, date, length, a count per label),
    Collections (courses), Tasks and Search (placeholders until Phases 4 and 3), Trash, and the Workspace. A back arrow
    returns home; leaving a session is refused while it is recording.
  - **Workspace:** Slides (left), Feed (centre), Audio stream (right), and the input bar along the bottom. The Layout menu
    hides any panel. Below 900 px one panel shows at a time, chosen by tabs.
  - **Feed items have a kind** (`NoteKind::Text` or `Slide`). A slide item is a note whose link points at a page, with an
    editable title ("Slide 4" by default). Starting a recording with a deck loaded posts the shown slide at 0:00; "Post next
    slide" (Cmd/Ctrl+Shift+N) turns the page and posts it at the current time. A slide item is only a reference to the deck
    already stored, so it costs a few bytes, not a copy of the image.
  - **Heading** is a fifth label. A note labelled Heading is drawn as a section title in the feed and the reading view, and
    becomes a `##` line in the Markdown export. It is added at the *end* of `LabelKind::ALL`, so Cmd/Ctrl+1–4 keep their
    meaning and Heading is Cmd/Ctrl+5.
  - **Audio stream:** the recording is cut into fixed 10-second sections (`lectio_core::stream`), each tinted with the first
    non-Heading label used inside it. I chose fixed sections over splitting at pauses because pause detection needs the
    audio decoded and analysed (slow for an hour of audio in the browser, and harder to test); fixed sections are pure
    arithmetic on the duration, so they are unit-tested and drawn instantly.
  - **Trash:** "Move to trash" stamps `deleted_unix_ms` on the session instead of deleting it. Trashed sessions are hidden
    everywhere except Trash, can be restored, and are removed for good (with their audio and files) at start-up once 30 days
    have passed (`without_expired_trash`).
  - **Saved layout and theme:** `Library.preferences` (`show_slides`, `show_feed`, `show_audio`, `theme`). I store them in the
    library rather than in a separate settings file because the library is already saved atomically on every platform (a file
    on desktop, IndexedDB in the browser); a second store would need a second save path and its own error handling.
  - **Schema v3.** All new fields have serde defaults, so a v2 file loads unchanged (text notes, nothing trashed, every panel
    shown, theme from the OS) and is written back as v3. A v4 file is refused, as before.
  - **Reading view and export:** headings, notes, images and posted slides as one document; "Copy all" puts Markdown on the
    clipboard and "Export Markdown" saves a `.md` file (a save dialog on desktop, a download in the browser).
- **Consequences:** the course sidebar is gone (Collections replaces it). An older Lectio can't open a v3 library, as with
  every schema bump. Computer-audio capture is shown in the source picker but disabled until Phase 3. Printing is not done yet;
  Markdown is the export format for now.

## ADR-016: Speech-to-text: Whisper on candle (pure Rust), on a thread on desktop and in a Web Worker in the browser
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** Phase 3 adds transcripts after class and live captions while recording. The spec asks for local, offline
  speech-to-text by default, on all four targets (Windows, macOS, Linux, web). An hour of lecture must not take hours.
- **Options considered:**
  - **whisper.cpp through `whisper-rs`.** Fast and widely used, but it compiles C++ with CMake and generates bindings with
    bindgen (libclang). This Mac has no CMake, every CI runner would need a C++ toolchain, and it doesn't build for the
    browser, so the web would need a second engine.
  - **ONNX Runtime (`ort`).** Good speed, but ships large native libraries per platform; the browser needs the separate
    JavaScript `onnxruntime-web`, so again two code paths.
  - **The browser's Web Speech API.** No download, but in Chrome it sends the audio to a cloud service (not local), it only
    works on a live microphone (not a saved recording), and it gives no timestamps per phrase.
  - **Transcribe on the desktop only.** Simplest, but the web version would lose a core Phase 3 feature.
  - **OpenAI's Whisper model run with `candle`, a machine-learning library written in Rust (chosen).** Pure Rust, so the same
    code builds for the desktop and for WebAssembly with no C/C++ toolchain. candle includes the Whisper network; I wrote the
    rest in a new crate, `crates/lectio-speech`.
- **Decision:**
  - **Engine (`lectio-speech`):** my own log-mel spectrogram (`mel.rs`; candle's version starts threads, which a browser
    build can't, P-027), a decoder that turns tokens back into text from `tokenizer.json` without a tokenizer library
    (`tokenizer.rs`), greedy decoding with Whisper's timestamp rules (`engine.rs`), and a window-by-window stepper
    (`job.rs`). The alignment rules (window offset, clamping, dropping "[Music]", where the next window starts, merging)
    are pure functions in `lectio_core::transcript`, unit-tested without a model.
  - **Models:** tiny (146 MB), base (279 MB), small (925 MB), chosen in the Transcript tab. Files come from a *pinned
    revision* of the public model repositories, so they can't change, and each file's size and SHA-256 checksum are stored
    in `models.rs` and checked after every download (desktop: also before every load). Downloads report progress.
    Desktop: `<data>/models/whisper-<size>/`; browser: IndexedDB table `models`.
  - **Where it runs:** desktop: one background thread owns the loaded model (`platform/native/speech.rs`). Browser: a second
    WebAssembly program, `crates/lectio-worker`, runs as a *Web Worker* (a separate JavaScript thread), so the page stays
    responsive; the page downloads the model and sends files and audio as *transferred* buffers (moved, not copied). The
    worker yields between 30-second windows so "Stop" gets through.
  - **Small is desktop-only:** its weights plus working memory don't fit comfortably in a browser tab's 4 GB
    WebAssembly limit; the browser shows it disabled with that reason.
  - **Language:** "Detect" (the first window decides, and the rest keeps it) or one of Whisper's 99 languages.
  - **Live captions:** every 2 s while recording, the last 8 s of audio go through the same engine; the strip above the
    input bar shows the text. They need the model to be downloaded already, and pause if the engine is busy.
  - **Measured on an M2 MacBook Air** (release build, Apple's Accelerate framework for the matrix maths), 54 s of speech:
    tiny 0.072, base 0.132, small 0.448 times real time (an hour of lecture: about 4, 8 and 27 minutes).
- **Consequences:** about 1.7 MB more WebAssembly for the worker; the first transcription downloads the model. Accuracy is
  Whisper's: tiny mishears technical words ("Q" for "queue"). There is no speaker separation yet (ADR-020).

## ADR-017: Playback speed without pitch change, and audio filters
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** Students replay lectures at 0.5×–2× and skip ±10 s; lecture halls are noisy and recordings often quiet. The
  filters must also be usable to make a cleaned copy.
- **Options considered:**
  - **Resampling (play samples faster).** Trivial, but the voice goes up in pitch ("chipmunk").
  - **A phase vocoder (FFT-based time-stretching).** High quality for music, but more code, phasey on speech, and needs FFTs
    on the audio thread.
  - **WSOLA (chosen for the desktop).** *Waveform-similarity overlap-add*: copy 40 ms pieces of the recording, overlapping
    by half, stepping through the source faster or slower than through the output, and nudge each piece (±10 ms) so its
    waveform lines up with the previous one. Cheap, made for speech, and allocation-free after start, so it runs in the
    real-time playback callback (`lectio_core::stretch::TimeStretcher`).
  - **Browser:** an `<audio>` element's `playbackRate` with `preservesPitch`, which the browser runs on its own audio thread
    (chosen for the web; doing WSOLA on the page's main thread would stutter when the UI is busy).
- **Decision (filters, `lectio_core::dsp`, the same on every target):**
  - **Noise reduction** is a *downward expander*: below a threshold just above the measured noise floor (the quietest 10% of
    20 ms frames), audio is turned down further (1:4, at most −30 dB). It quietens hiss between words; it can't remove
    noise *under* speech, which would need spectral processing.
  - **Voice boost:** a high-pass biquad at 80 Hz (rumble) and a +6 dB peak at 2.5 kHz (consonants). A *biquad* is a
    standard second-order filter; the coefficients are the "Audio EQ Cookbook" formulas.
  - **Normalise:** one gain bringing speech to about −20 dBFS, limited so peaks stay under 0.95 and the gain under +20 dB.
  - Filters run live while playing (desktop: in the callback; browser: applied to the samples in slices across frames,
    then played). **"Save cleaned copy"** writes `<session>-clean.wav` next to the original (kept), and playback switches
    to it (the live filters then stay off, so they aren't applied twice).
  - Speed steps 0.5–2× (Cmd/Ctrl+Shift+, and .), skip ±10 s (Cmd/Ctrl+[ and ]); speed and filters are saved in
    `Library.preferences`.
- **Consequences:** filter maths and the stretcher are unit-tested (gains in dB, pitch by zero-crossings, timing). The
  browser needs the whole recording decoded in memory to filter it (as before for playback).

## ADR-018: Search by scanning the library, and slide text from the PDF renderer
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** Search must cover notes, headings, transcripts, slide titles and the text on slides, and jump to the moment.
- **Options considered:**
  - **A full-text index (e.g. the `tantivy` search engine).** Fast at scale, but a large dependency, a second store to keep in
    step with the library on every platform, and more than a term of notes needs.
  - **Scan the library on each search (chosen).** A term of lectures is a few MB of text; scanning takes milliseconds and
    can never be out of date.
  - **Slide text with another PDF library.** Unnecessary: the renderer already interprets every page (ADR-011).
- **Decision:** `lectio_core::search`: text is folded (lower case, accents removed, one char per char so positions stay
  valid); every query word must match a whole word, except the last, which may be a prefix (results while typing).
  Score = kind weight (heading 3, slide title 2.5, note 2, slide text 1.5, transcript 1) × word matches (1 whole, 0.5
  prefix), plus one weight if the whole query is a phrase; ties go to the newer session, then the earlier moment. Snippets
  show about 80 characters around the match, starting at a word. Slide text is read once per deck with a small hayro
  *device* that keeps only the Unicode of each glyph run (`media/slide_text.rs`) and stored in `SlideDeck.page_text`
  (on the renderer's thread on desktop, one page per frame in the browser). The slides panel's search box lists matching
  pages.
- **Consequences:** decks imported before Phase 3 get their text the first time their session is opened. Text in scanned
  (image-only) PDFs isn't found; that would need OCR.

## ADR-019: Recording computer audio, and screenshots into the feed
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** Online classes need "Computer audio" or "Microphone + computer", and a quick screenshot of what is shown.
- **Options considered:**
  - **macOS ScreenCaptureKit through a binding crate.** Works from macOS 12.3, but it's a second audio path next to cpal
    and needs a screen-recording session just for sound.
  - **Each OS's loopback through cpal (chosen).** cpal 0.18 records an *output* device as input: on macOS through a Core
    Audio process tap (macOS 14.2+), on Windows through WASAPI loopback, and on Linux I pick the default output's
    PulseAudio/PipeWire *monitor* source through cpal's PulseAudio host (pure Rust, no system library).
  - **Browser:** `getDisplayMedia` with audio (the user picks a tab or screen and ticks "Share audio"; Chrome and Edge).
- **Decision:** the recorder opens one cpal stream per source; each has its own lock-free ring; the writer thread converts
  each to 16 kHz and *mixes* them (the first input sets the pace, a second one is added sample by sample and trimmed if it
  runs over 0.5 s ahead). In the browser both streams feed the same AudioWorklet, whose input adds them. Sources that can't
  work are shown disabled with the reason. **Screenshots:** desktop captures the main display with `xcap`, browser grabs one
  frame of a `getDisplayMedia` video; the user drags over the part to keep, and it becomes a photo note (JPEG) at the moment
  the button was pressed.
- **Consequences:** permissions: macOS asks for "Screen & System Audio Recording" (audio) and "Screen Recording"
  (screenshots); without them the start fails with a message saying where to allow it. If the default output device also has
  an input (some USB headsets), macOS records that input instead of the tap. Firefox and Safari can't share tab audio. On
  Linux, `xcap` builds against PipeWire, so CI installs `libpipewire-0.3-dev` and clang.

## ADR-020: Speaker separation: not yet
- **Date:** 2026-11-19
- **Status:** accepted (to revisit)
- **Context:** The feature list asks for speaker separation ("who said what") if a practical local method exists.
- **Options considered:**
  - **A neural diarisation pipeline** (a voice-activity model, a speaker-embedding model, then clustering): the accurate way,
    but it means two more models to download and port to candle, and clustering whose right settings depend on the room.
  - **Clustering simple features** (pitch, MFCC averages) per segment: cheap, but unreliable for a lecturer plus students in
    a reverberant hall; wrong labels are worse than none.
  - **Not yet (chosen).**
- **Decision:** transcripts have no speaker labels in Phase 3. The `Segment` type can gain an optional speaker field with a
  serde default later without breaking saved files.
- **Consequences:** a lecture is treated as one voice; questions from students appear in the same transcript.

## ADR-021: AI study tools behind a `StudyModel` provider layer: a local model, Ollama, or an OpenAI-compatible endpoint
- **Date:** 2026-11-19
- **Status:** accepted (written before the code)
- **Context:** Phase 4 adds an outline of a session, quizzes and flashcards made from the user's own lecture material. The
  spec asks for local-first behaviour, works with no AI at all (features hidden, never broken), and a pluggable provider.
  Answers must be checkable: every generated item cites the moments and slides it came from.
- **Options considered (the local model):**

  | candidate | size (download) | licence | notes |
  |---|---|---|---|
  | **Qwen2.5-0.5B-Instruct**, GGUF Q4_K_M | 469 MB | Apache-2.0 | best instruction following at this size; supported by candle's `quantized_qwen2`; small enough for a browser tab |
  | SmolLM2-360M-Instruct, GGUF Q8_0 | 369 MB | Apache-2.0 | smaller, but noticeably weaker at following an output format |
  | Llama-3.2-1B-Instruct | ≈ 0.8 GB (Q4) | Llama 3.2 Community Licence, gated | download requires an account and accepting terms, so it can't be fetched by the app |
  | Phi-3.5-mini-instruct | ≈ 2.3 GB (Q4) | MIT | good quality, but too large for a browser tab and slow on a laptop CPU |

  I chose **Qwen2.5-0.5B-Instruct** as the default on every target, and offer **Qwen2.5-1.5B-Instruct** (1.1 GB, Apache-2.0,
  same tokenizer) on the desktop for better answers. Both use the same pattern as Whisper (ADR-016): files from a pinned
  revision, sizes and SHA-256 in the code, download on first use with progress, run on a desktop thread or in the browser's
  Web Worker.
- **Options considered (the layer):** one hard-wired provider (simplest, but no choice and no offline/online trade-off); a
  plug-in system loading code at run time (overkill); **a small trait with three implementations (chosen)**.
- **Decision:**
  - **`StudyModel`** (in `platform/ai.rs`): "complete this chat (system + user text, at most N tokens)" plus "is it ready"
    and progress events. Implementations: **Local** (Qwen via candle, `crates/lectio-llm`), **Ollama** (any model the user
    has pulled, at `http://localhost:11434`, detected with `/api/tags`), **OpenAI-compatible** (any `/v1/chat/completions`
    endpoint: a base URL, a model name, and the user's key).
  - **No provider = AI hidden.** The Outline tab and the "with AI" buttons only appear when a provider is set and ready.
    Offline fallbacks don't need AI: a cloze quiz from Definition notes and flashcards from Definition notes.
  - **Keys** are stored apart from the library (a `secrets.json` readable only by the user on the desktop, the browser's own
    storage on the web), never put in `library.json` (which a user may share), never shown in errors or logs (the `Debug`
    output of the settings is redacted).
  - **Citations:** the material sent to a model is a numbered list, one line per source, e.g.
    `[S12] (17:05) BFS visits vertices in order of distance` or `[S30] (slide 2) Queue, distance order`. The model must
    end each item with the source numbers it used; Lectio maps them back to moments and slides (`lectio_core::sources`).
    Items with no valid source are dropped, so nothing appears without a place to check it.
  - **Long sessions are chunked:** sources are packed in time order into chunks under a token budget (4 characters ≈ 1
    token); a single over-long source is cut at a word with "…"; a whole collection keeps an even sample of chunks within a
    limit. Each chunk gets its own request; results are merged in time order.
  - **Output format:** small models follow simple line formats far better than JSON, so prompts ask for lines like
    `[S12] Title` (outline) or `Q:` / `A)`…`D)` / `Answer:` / `Why:` / `Sources:` blocks (quiz); parsers ignore anything else.
- **Consequences:** the first local use downloads 469 MB; quality is that of a 0.5B model (good enough for outlines and
  simple questions, weaker on subtle ones), so the user can switch to the 1.5B model, Ollama, or an endpoint. In the browser
  Ollama only works when the page is served from `localhost` (its default allowed origins), and an endpoint must allow CORS.
- **Changes after building it (2026-11-19):**
  - **Prompts show a worked example instead of a template.** With a template (`A) choice`) the 0.5B model copied the
    placeholders back (P-032). Each prompt now ends with a short, real example on a different topic (the solar system), and
    the parsers drop placeholder answers, duplicate choices, and items that copy a source line word for word.
  - **Citations can be inferred.** The 0.5B model often leaves out `[S12]` even when the example shows it. Rather than drop
    a good item, Lectio cites the source that shares the most distinctive words with it (at least two words of three or more
    letters, not in a short list of common words; `sources::best_match`). If nothing matches well enough, the item is still
    dropped, so the rule "nothing without a place to check it" holds. I preferred this to asking again, which doubles the
    wait and usually gives the same answer. The same check also catches *wrong* numbers: a cited line sharing fewer than
    two such words with the item is replaced by the best match, when there is one (`sources::checked_ids`, P-036).
  - **Too little material is refused up front.** Under about 600 characters (`MIN_OUTLINE_TOKENS` = 150) an outline can
    only repeat the notes, so Lectio says so instead of calling a model.
  - **Browser builds use WebAssembly SIMD** (`.cargo/config.toml`), without which candle's quantized kernels are not
    compiled and the model is far too slow (P-035). This raises the minimum browsers to Chrome 91, Firefox 89 and
    Safari 16.4.
  - **In the browser the study model runs in its own worker.** The page starts a second copy of the worker program for it
    (`lectio_llm::protocol`), because each worker has its own WebAssembly memory (at most 4 GB): Whisper and Qwen never have
    to fit in one. Downloads are assembled in a JavaScript buffer, not WebAssembly memory, which can grow but never shrinks.
  - **Measured on an M2 MacBook (desktop, release build):** 0.5B reads the prompt at about 80 tokens/s and writes about
    46 tokens/s; 1.5B about 140 and 32 tokens/s (`lectio --ai-benchmark small|large` reproduces this).

## ADR-022: Definitions: Open English WordNet, downloaded on demand; an online dictionary behind the same trait
- **Date:** 2026-11-19
- **Status:** accepted
- **Context:** Looking up a word in a note or the transcript should work offline, with an optional online source.
- **Options considered:** bundling a dictionary in the app (a useful English dictionary is tens of MB, too much for every web
  page load); WordNet 3.0 (old); **Open English WordNet** (CC BY 4.0, maintained; the 2023 edition as one JSON Lines file on
  Hugging Face, which a browser may download); Wiktionary extracts (CC BY-SA, much larger, messy markup);
  dictionaryapi.dev (free, no key, online only).
- **Decision:** a `DictionaryProvider` trait with two providers. **Offline (default):** Open English WordNet 2023, downloaded
  on first use from a pinned revision (49 MB, SHA-256 checked), then converted once into a compact tab-separated index
  (word → part of speech, definition, example) kept on the device. Lookups try simple base forms (plurals, -ing, -ed).
  **Online (off by default):** dictionaryapi.dev. A definition can be added to the feed as a Definition note.
- **Consequences:** English only; the attribution "Open English WordNet (CC BY 4.0)" is shown with every offline result.
  In the browser the index is built by a short-lived copy of the worker (so the page doesn't freeze while 49 MB of JSON is
  read) and kept in IndexedDB; it is read into memory on the first look-up.

## ADR-023: Math notes: my own layout engine for a LaTeX subset, drawn with an open math font
- **Date:** 2026-11-20
- **Status:** accepted (written before the code)
- **Context:** Students need formulas in their notes (`\frac{n(n+1)}{2}`). The spec asks for LaTeX input with a live preview,
  rendered in the feed, the paper view and every export, on all four targets, preferably in pure Rust.
- **Options considered:**

  | option | pure Rust / browser | size | fits egui + PDF + DOCX | notes |
  |---|---|---|---|---|
  | KaTeX or MathJax through an embedded JavaScript engine | no (C engine or the page's JS) | +1–3 MB | needs a second renderer for egui | best coverage, but two code paths and a JS engine on the desktop |
  | Typst's math engine | yes | +10 MB, large API | yes, but brings a whole typesetting system | far more than notes need |
  | `latex2mathml` / `pulldown-latex` | yes | small | only produce MathML; egui can't draw MathML | good for HTML only |
  | **own parser + box layout for a LaTeX subset (chosen)** | yes | ~1 k lines + a 1 MB font | one layout, drawn by every output | limited to what students write in notes |
- **Decision:** `lectio_core::math` parses the common subset (letters, numbers, operators, Greek letters and about 110
  symbols, `^`, `_`, `\frac`, `\sqrt[n]{}`, `\text{}`, `\mathbf{}`, `\left( … \right)`, `\overline`, accents, `\sum`/`\prod`/
  `\int`/`\lim` with limits, `\binom`) into a tree and lays it out as boxes (TeX's idea: every piece has a width, a height above
  the baseline and a depth below). The layout only needs glyph widths, so it is tested in `lectio-core` with fake metrics. The
  app turns the laid-out glyphs and rules into an image with the bundled **Noto Sans Math** (SIL OFL), for the feed, paper
  view, PDF and DOCX; Markdown keeps the LaTeX source between `$$`. Anything outside the subset is an error shown in the live
  preview, never a wrong picture.
- **Consequences:** no macros, matrices or alignment environments yet (`\begin{…}` is reported as not supported). The
  formula is a picture in exports (the LaTeX source is its alt text), so it isn't searchable inside a PDF; it is searchable in
  Lectio because the note stores the source.

## ADR-024: Slide annotations: vector strokes per page, normalised to the page
- **Date:** 2026-11-20
- **Status:** accepted
- **Context:** Draw, highlight and erase on a slide page, shown in the slides panel, the paper view and exports.
- **Options considered:** (1) save a PNG per annotated page (simple, but loses quality when zoomed, makes erasing a pixel
  operation, and adds a blob per page); (2) edit the PDF (complex, and the original file should stay as imported); (3)
  **strokes as point lists in `library.json` (chosen)**.
- **Decision:** each `SlideDeck` gets `ink: Vec<PageInk>`; a `PageInk` is a page number and its strokes. A stroke is a tool
  (pen or highlighter), a colour from a fixed palette, a width, and points with coordinates from 0 to 1 across the page, so
  the same strokes fit any zoom or export size. Points are rounded to 1/10 000 (finer than a pixel on a 4K screen) to keep
  the file small. The eraser removes whole strokes that pass within its radius (like a whiteboard eraser, and easy to undo).
  Limits: 2 000 points per stroke, 500 strokes per page.
- **Consequences:** schema version 6. Drawing is drawn by egui on top of the page; exports draw the same strokes with
  tiny-skia onto the rendered page.

## ADR-025: Paper view and exports: one document model, three writers (Markdown, PDF with krilla, DOCX by hand)
- **Date:** 2026-11-20
- **Status:** accepted
- **Context:** A document-style view of a session with slides inline, plus Copy, Print, and export to Markdown, PDF and DOCX
  on every target.
- **Options considered (PDF):** `printpdf` (big API changes between versions), `pdf-writer` (low level: fonts and text
  layout by hand), **krilla** (high-level, pure Rust, embeds and subsets fonts, images and vector paths; chosen).
  **(DOCX):** `docx-rs` (many dependencies), **writing the Office Open XML by hand into a zip (chosen)**: a .docx is a zip of
  a few XML files, and the part I need (headings, paragraphs, images) is about 200 lines that I can test.
  **(Print):** native print dialogs differ on every OS; instead the desktop writes the PDF and opens it in the system viewer,
  and the browser prints an HTML version of the document from a hidden frame.
- **Decision:** `lectio_core::paper` turns a session into a list of blocks (title, heading, note, formula, slide, image,
  transcript excerpt), with each slide shown once, before the first note about it. A new crate `lectio-export` (pure Rust,
  no UI) writes the blocks as Markdown, HTML, PDF and DOCX, given the pictures (slide pages with their ink, photos, sketches,
  formulas) as PNG bytes, which the app renders first, one per frame so the window stays responsive.
- **Consequences:** exports are the same on all targets and testable without a window. PDF text is real text (selectable,
  searchable) in Inter; formulas and slides are images.

## ADR-026: Collections across courses; filter and sort on Home
- **Date:** 2026-11-20
- **Status:** accepted
- **Context:** Group sessions across courses (e.g. "Midterm 1"), and filter and sort the Home list.
- **Decision:** `Library.collections`: a name and an ordered list of session ids. A session can be in many collections;
  removing a session for good removes it from them; trashed sessions are hidden but kept. Home has a filter (all, one
  course, one collection, or one label) and a sort (newest, oldest, title, longest), saved in the preferences.
  "Quiz me" can now use a whole collection.
- **Options considered:** tags on sessions (simpler, but a collection's order and name couldn't be edited in one place);
  nested folders (more than the spec asks).

## ADR-027: Screen readers on the web: a DOM mirror of egui's AccessKit tree
- **Date:** 2026-11-20
- **Status:** accepted (fixes P-013)
- **Context:** egui draws into one `<canvas>`; on the desktop eframe hands its AccessKit tree to the OS, but in the browser
  eframe 0.37 throws that tree away ("not currently implemented"), so screen readers find nothing.
- **Options considered:** eframe's `web_screen_reader` feature (it only speaks text through speech synthesis; a screen reader
  user can't navigate it); rewriting the web UI in HTML (a second UI); **an egui plugin that mirrors the tree into the DOM
  (chosen)**.
- **Decision:** a plugin (`platform/web/a11y.rs`) turns on AccessKit and, in egui's `output_hook`, receives the tree every
  frame. It keeps one element per node in an invisible overlay above the canvas (`opacity: 0`, `pointer-events: none`, so
  the mouse still reaches egui), positioned like the widget so a screen reader's highlight lands in the right place, with
  the matching role (`button`, `checkbox`, `heading`, `textbox`…), name, value and state, and only touches elements that
  changed. (My first plan put the elements inside the `<canvas>` as "fallback content"; fallback content gets no layout
  boxes, so a screen reader's cursor would have no position to show.) Activating a mirrored control (for example VoiceOver's VO-Space) becomes an AccessKit
  `Click` request that the plugin's `input_hook` gives back to egui; typing into a mirrored text field becomes `SetValue`.
  Focus changes are announced in a polite live region.
- **Consequences:** the browser test can check the accessibility tree again (`--expect-ax`). The mirror follows egui's
  frames, so very fast changes (a timer) are announced only when the user moves to them.

## ADR-028: Proof on platforms I can't test by hand, and releases from tags
- **Date:** 2026-11-20
- **Status:** accepted
- **Decision:** CI runs the desktop app's `--screenshot` mode on Linux (under `xvfb`, Mesa's software renderer) and Windows
  (the WARP software adapter) and uploads the pictures; on Linux it also runs `--self-test-audio` against a PulseAudio null
  sink. A `release.yml` workflow on `v*` tags builds a universal macOS `.dmg`, a Windows `.zip` and `.msi`, a Linux AppImage
  and the web build as a zip, and attaches them to a GitHub release. Signing happens only when the signing secrets exist
  (Apple Developer ID / notarisation; a Windows code-signing certificate); otherwise the artifacts are unsigned and the
  install guide says how to open them.
- **Consequences:** the new CI steps start as advisory (they don't fail the build) until they have been green a few times.
