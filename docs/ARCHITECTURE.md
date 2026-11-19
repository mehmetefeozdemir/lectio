# Architecture

> Updated after each phase. Current state: **end of Phase 2 (Material) + UI redesign (ADR-014)**.

## Stack
Rust 2024 edition, **one codebase for Windows, macOS, Linux and the web** (ADR-010). UI with **eframe/egui 0.37** (ADR-003),
native in a window and WebAssembly on a `<canvas>` in the browser. Everything platform-specific (storage, recording, playback,
file picking, blobs, camera) sits behind traits in `src/platform/mod.rs`. Domain logic lives in `lectio-core`, a crate with no
UI, audio or platform code that CI also compiles for `wasm32-unknown-unknown`. Phase 2 media code is shared by all targets:
PDF slides with **hayro** (ADR-011), photos with **image**, sketches with **tiny-skia**, dialogs with **rfd** (ADR-013).

## Platform boundary
```mermaid
flowchart TB
  subgraph shared["Shared by every target"]
    UI["ui/* views"] -->|Action| APP["app.rs: LectioApp"]
    APP --> CORE["lectio-core: model, clock, seek, SpeechConverter, interpret_saved"]
    APP --> MAT["material.rs: imports, slide viewer, texture caches"]
    MAT --> MEDIA["media/*: hayro slides, image decode, tiny-skia sketches (same on every target)"]
    APP -->|"Box&lt;dyn Platform&gt;"| P{{"trait Platform: store, audio, pick_file, put/get/delete_blob, open_camera"}}
  end
  P -->|"cfg(not wasm32)"| NATIVE
  P -->|"cfg(wasm32)"| WEB
  subgraph NATIVE["platform/native: Windows, macOS, Linux"]
    NS["JsonFileStore: library.json + recordings/*.wav"]
    NB["FileBlobStore: blobs/&lt;uuid&gt;.pdf|png|jpg"]
    NR["Recorder: cpal input + rtrb ring + writer thread (ADR-009)"]
    NP["Player: cpal output stream"]
    NF["rfd native dialog (blocking); no camera"]
  end
  subgraph WEB["platform/web: browser (WebAssembly)"]
    WS["IdbLibraryStore: IndexedDB 'lectio' v2 (library, recordings, blobs)"]
    WR["WebRecorder: getUserMedia + AudioWorklet"]
    WP["WebPlayer: Web Audio AudioBufferSourceNode"]
    WF["rfd input type=file (async) + WebCamera: getUserMedia video"]
  end
```

| Trait (`src/platform/mod.rs`) | Native (`platform/native/`) | Web (`platform/web/`) |
|---|---|---|
| `LibraryStore` (in `lectio-core`) | `JsonFileStore`: atomic file save | `IdbLibraryStore`: read before start, saves queued in the background |
| `AudioCapture` | `Recorder` (cpal, lock-free ring) | `WebRecorder` (`AudioWorklet` batches → `SpeechConverter`) |
| `AudioPlayback` | `Player` (cpal output, sample position) | `WebPlayer` (`AudioBufferSourceNode`, `AudioContext.currentTime`) |
| time "now" (`now_unix_ms`) | `SystemTime` | JavaScript `Date.now()` |
| `Platform::pick_file` (Phase 2) | `rfd::FileDialog`, blocks while open (audio threads keep recording) | `rfd::AsyncFileDialog` → `Pending` |
| `put_blob` / `get_blob` / `delete_blob` | `FileBlobStore` (`lectio-core`), atomic write | IndexedDB table `blobs`, queued writes |
| `open_camera` → `CameraSession` | `None` (file picker instead, ADR-013) | `WebCamera`: `<video>` + off-screen canvas, JPEG snapshot |
| slide rendering (not a trait: same code) | `DeckRenderer` on a background thread | `DeckRenderer`: one page per frame on the main thread |

Browser APIs are asynchronous; the traits are synchronous because egui redraws every frame. So the web side starts work with
`wasm_bindgen_futures::spawn_local` and the app checks results each frame: `is_ready()`, `take_error()`,
`take_background_error()`, and `Pending::take()` for file picks and blob reads (native returns `Pending::ready` at once).

## Folder structure
```
lectio/
├── Cargo.toml                 # workspace root + the `lectio` app; dependencies split by target
├── index.html, Trunk.toml     # web build (trunk build --release → dist/)
├── web/lectio-capture-worklet.js  # browser audio-thread code for recording
├── .github/workflows/ci.yml   # CI: ubuntu/windows/macos + web (+ advisory browser flow)
├── tools/web-smoke.mjs        # headless-Chrome test driver; `--flow phase2` (CI) and `--flow phase3` (local, downloads a model)
├── assets/sample-slides.pdf   # 2-page deck for sample data, tests and the browser flow
├── assets/fonts/              # Inter, JetBrains Mono (OFL), Phosphor icons (MIT); sources in README.md
├── crates/lectio-core/        # pure domain logic: no UI, no audio I/O, no platform code
│   ├── src/
│   │   ├── lib.rs             # public API, MAX_NAME_CHARS
│   │   ├── model.rs           # Library → Course → Session → Note; immutable `with_*` changes
│   │   ├── labels.rs          # LabelKind (fixed identity) + LabelSet (renameable names)
│   │   ├── ids.rs             # CourseId / SessionId / NoteId (typed UUIDs)
│   │   ├── error.rs           # ModelError with plain-English messages
│   │   ├── timeline.rs        # seek maths: ms ↔ samples ↔ timeline fraction, active note
│   │   ├── clock.rs           # RecordingClock state machine + NoteDraft (anchor = start of typing)
│   │   ├── resample.rs        # downmix, Downsampler (→16 kHz), SpeechConverter (chunk-safe), float→i16
│   │   ├── realtime.rs        # integer maths used on the audio thread: ring size, whole-frame writes
│   │   ├── material.rs        # Phase 2: BlobKey, MediaKind/check_file, SlideDeck, SlideLink, Attachment, orphaned_blobs,
│   │   │                      #   with_slide_item / with_note_text (slides posted to the feed)
│   │   ├── stream.rs          # audio stream: 10 s sections, section_at, section_labels (tint per section)
│   │   ├── trash.rs           # live/trashed sessions, trash + restore, 30-day expiry
│   │   ├── prefs.rs           # Preferences: panels, theme, speed, filters, captions, speech model + language
│   │   ├── export.rs          # session_markdown: headings, notes, slides, labels → Markdown
│   │   ├── transcript.rs      # Phase 3: Segment/Transcript, window alignment, next window, merge, segment_at
│   │   ├── search.rs          # Phase 3: fold, word/prefix matching, ranking, snippets, search_deck, with_slide_text
│   │   ├── dsp.rs             # Phase 3: analyse, biquads, expander, normalise gain, FilterChain, with_cleaned_copy
│   │   ├── stretch.rs         # Phase 3: speed steps, skip, time left, TimeStretcher (WSOLA)
│   │   └── storage.rs         # LibraryStore, interpret_saved, JsonFileStore, FileBlobStore (atomic writes)
│   └── tests/                 # 143 tests: model, material, workflow, transcript, search, audio DSP, seek, clock,
│                              #   storage, resample, capture pipeline
├── crates/lectio-speech/      # Phase 3 (ADR-016): Whisper on candle, no platform code
│   └── src/                   # mel.rs (spectrogram), tokenizer.rs (tokens → text, languages), models.rs (files,
│                              #   pinned revisions, SHA-256), engine.rs (one 30 s window), job.rs (window stepper,
│                              #   captions), protocol.rs (page ↔ worker messages); 13 tests
├── crates/lectio-worker/      # Phase 3: the browser's speech Web Worker (wasm only; empty on desktop)
├── src/
│   ├── main.rs                # module list + two `main`s, picked by target
│   ├── entry_native.rs        # native: args (--screenshot, --self-test-audio, --transcribe), window options
│   ├── entry_web.rs           # web: open IndexedDB, start egui on the canvas, #sample
│   ├── app.rs                 # LectioApp: owns state, applies Actions, saves through `dyn Platform`
│   ├── app/material_actions.rs # Phase 2 actions: imports, camera, sketch, viewer, reading view
│   ├── app/ui_state.rs        # undo for deletes (8 s), level-meter history, silence timer
│   ├── app/workflow.rs        # screens, trash, posting slides, saved layout/theme, Copy all / Export Markdown
│   ├── app/speech_actions.rs  # Phase 3: transcription, model download, live captions, search, opening a result
│   ├── app/audio_actions.rs   # Phase 3: speed, filters, cleaned copy, recording source, screenshots
│   ├── action.rs              # Action enum: everything the UI can ask for
│   ├── material.rs            # Phase 2 runtime state: pending imports, Viewer, image/slide texture caches, camera
│   ├── media/                 # slides.rs (hayro, DeckRenderer: pages + text), slide_text.rs (text device),
│   │                          #   images.rs (EXIF-upright decode, JPEG), sketch.rs (PNG)
│   ├── platform/
│   │   ├── mod.rs             # traits Platform / AudioCapture / AudioPlayback / CameraSession, Pending, FileKind,
│   │   │                      #   CaptureSource
│   │   ├── speech.rs          # SpeechService trait, SpeechRequest / SpeechEvent
│   │   ├── wav.rs             # WAV bytes ↔ samples (shared; 2 unit tests)
│   │   ├── native/            # mod.rs (NativePlatform: data folder, rfd, FileBlobStore, xcap), recorder.rs (mic +
│   │   │                      #   loopback, mixing), player.rs (stretcher + filters), speech.rs (thread, download, CLI)
│   │   └── web/               # mod.rs (WebPlatform), idb.rs, store.rs, recorder.rs, player.rs (<audio>), files.rs,
│   │                          #   camera.rs, display.rs (getDisplayMedia), speech.rs (model download, worker)
│   ├── ui/                    # views (see "Screens and workspace"): theme (tokens), widgets, icons, format,
│   │                          #   nav (left column), home (Home/Collections/Trash/Tasks), search (Search page),
│   │                          #   workspace (panels, Layout menu, tabs, caption strip), slides, notes (feed),
│   │                          #   stream (Audio | Transcript tabs), transcript, capture + playback + composer (input
│   │                          #   bar), timeline, reading, sketch, camera, screen_crop, labels (rename window)
│   ├── demo.rs                # sample data with slides + a sketch (--screenshot, web #sample)
│   ├── self_test.rs           # native only: headless mic test
│   └── screenshot.rs          # native only: draws 120 frames, saves a PNG, exits
├── docs/                      # design docs: architecture, decisions, problems
└── shots/                     # screenshots
```

## Data model (implemented; schema version 4)
```mermaid
erDiagram
  LIBRARY ||--o{ COURSE : contains
  LIBRARY ||--|| LABEL_SET : has
  LIBRARY ||--|| PREFERENCES : "has (v3)"
  COURSE ||--o{ SESSION : contains
  SESSION ||--o| RECORDING : has
  SESSION ||--o{ NOTE : "has (sorted by offset_ms)"
  SESSION ||--o{ SLIDE_DECK : "has (Phase 2)"
  NOTE }o--o{ LABEL_KIND : tagged
  NOTE }o--o| SLIDE_LINK : "links to (Phase 2)"
  SLIDE_LINK }o--|| SLIDE_DECK : "page of"
  NOTE ||--o| ATTACHMENT : "photo or sketch (Phase 2)"
  SLIDE_DECK ||--|| BLOB : "PDF bytes, stored separately"
  SESSION ||--o| TRANSCRIPT : "has (v4)"
  TRANSCRIPT ||--o{ SEGMENT : "sorted, inside the recording"
  ATTACHMENT ||--|| BLOB : "PNG/JPEG bytes, stored separately"
  LIBRARY {
    u32 schema_version "4 (1, 2 and 3 still load)"
  }
  TRANSCRIPT {
    string language "Whisper code, e.g. en"
    string model "tiny|base|small"
    bool complete "false while running or stopped"
  }
  SEGMENT {
    u64 start_ms
    u64 end_ms
    string text
  }
  PREFERENCES {
    bool show_slides "default true"
    bool show_feed "default true"
    bool show_audio "default true"
    enum theme "system|light|dark"
    u16 speed_percent "50-200, v4"
    FilterSettings filters "noise_reduction, voice_boost, normalise (v4)"
    bool play_cleaned "v4"
    bool captions "v4"
    enum speech_model "tiny|base|small (v4)"
    string language "none = detect (v4)"
    bool show_transcript_tab "v4"
  }
  SLIDE_DECK {
    uuid id
    string title "from the file name"
    BlobKey blob "uuid.pdf"
    u32 page_count "read at import"
    string[] page_text "one per page, for search (v4)"
  }
  SLIDE_LINK {
    uuid deck
    u32 page "1..=page_count"
  }
  ATTACHMENT {
    enum kind "photo|sketch"
    BlobKey blob "uuid.png or uuid.jpg"
    u32 width "upright (EXIF applied)"
    u32 height
  }
  BLOB {
    bytes data "never inside library.json (ADR-012)"
  }
  COURSE {
    uuid id
    string name "1-80 chars, trimmed"
  }
  SESSION {
    uuid id
    string title
    u64 created_unix_ms
    u64 deleted_unix_ms "set while in the trash (v3)"
  }
  RECORDING {
    string file_name "recordings/<session-id>.wav"
    u64 duration_ms
    u32 sample_rate "16000"
    string cleaned_file "session-clean.wav, if made (v4)"
  }
  NOTE {
    uuid id
    enum kind "text (default) | slide (v3)"
    u64 offset_ms "position in the recording"
    string body "may be empty if it has a label; a slide item's title"
    LabelKind[] labels "important|review|task|definition|heading"
  }
  LABEL_SET {
    LabelKind kind
    string name "renameable display name"
  }
```
Rules enforced by `Library` (tested in `tests/model_tests.rs` and `tests/material_tests.rs`): names can't be blank or longer
than 80 characters; a note needs text, a label or an image; notes of every kind stay sorted by `offset_ms` (linking a slide
never moves a note); a note can't be placed after the end of an existing recording; a session gets one recording; a slide link
must point at an existing page of a deck *in the same session*; removing a deck removes the links to it, not the notes;
`BlobKey`s are only `<canonical uuid>.<pdf|png|jpg>`, checked again when `library.json` is read (no `../` path tricks);
imported files are recognised by their first bytes, with 100 MB (PDF) and 25 MB (image) limits; a file with a newer
`schema_version` is refused. Since v3 (tested in `tests/workflow_tests.rs`): a slide item must link to an existing page and
its title falls back to "Slide N" when cleared; a trashed session is hidden from every list except Trash and is removed for
good 30 days after `deleted_unix_ms`; every new field has a default, so v2 files load unchanged. Since v4 (tested in
`tests/transcript_tests.rs` and `tests/search_tests.rs`): a transcript needs a recording and its segments must be in order
and inside the recording; `page_text` has exactly one entry per page; a cleaned copy's name is letters, digits, `-`, `_`
and `.wav` only; v3 files load with no transcripts, normal speed and no filters.

Where the data lives (same JSON format and same WAV format on every platform):
```
Native: the OS app-data folder (macOS ~/Library/Application Support/org.Lectio.Lectio,
        Windows %APPDATA%\Lectio\Lectio\data, Linux ~/.local/share/lectio), or $LECTIO_DATA_DIR
  library.json                  # the whole Library, pretty-printed JSON (small: no file bytes)
  recordings/<session-id>.wav   # 16 kHz, mono, 16-bit PCM
  blobs/<uuid>.pdf|png|jpg      # Phase 2: slide decks, photos (as picked), sketches (PNG)
  recordings/<id>-clean.wav     # Phase 3: a cleaned copy, if the user made one
  models/whisper-<size>/        # Phase 3: config.json, tokenizer.json, model.safetensors (checked by SHA-256)

Web: IndexedDB database "lectio" (version 3) in the browser, per site
  table "library":    key "library.json"      → the same JSON text
  table "recordings": key "<session-id>.wav"  → the same WAV bytes
  table "blobs":      key "<uuid>.pdf|png|jpg" → the file's bytes (added by the v1→v2 upgrade; v1 data is kept)
  table "models":     key "whisper-<size>/<file>" → a speech-model file (added by the v2→v3 upgrade)
```

## Phase 2 flows

### Import slides → see them beside the notes → link notes
```mermaid
sequenceDiagram
  actor S as Student
  participant App as LectioApp
  participant P as Platform
  participant M as MaterialState
  participant R as DeckRenderer (thread / per-frame)
  S->>App: Import PDF…
  App->>P: pick_file(Slides) → Pending
  P-->>App: (later frame on web) PickedFile{name, bytes}
  App->>App: check_file (magic bytes, ≤100 MB), slides::page_count (hayro parse)
  App->>P: put_blob(uuid.pdf, bytes)   (blob first …)
  App->>App: library.with_deck(...)     (… then the reference, ADR-012), save
  App->>M: seed_deck(bytes), show_deck(page 1)
  M->>R: request(page) → ColorImage → texture
  S->>App: types a note while slide 4 is shown
  App->>App: with_note + with_slide_link(deck, 4)  (if "Link new notes" is on)
  S->>App: clicks the note's "Slide 4" chip
  App->>M: show_deck(deck, 4)
```

### Photo / camera / sketch → image note on the timeline
```mermaid
flowchart LR
  A[Photo from file…] -->|offset taken at click| B[pick_file Photo]
  C[Take photo… web only] --> D[WebCamera snapshot → JPEG]
  E[Sketch…] -->|offset taken when opened| F[strokes → tiny-skia PNG 960×600]
  B --> G[check_file + upright_size EXIF]
  G --> H[put_blob uuid.jpg/png]
  D --> H
  F --> H
  H --> I[with_attachment_note at offset + auto slide link]
  I --> J[MaterialState.seed_image → texture, max 1600 px]
```
Deleting a note or deck → `orphaned_blobs(before, after)` → `delete_blob` for each, after the library is saved.

### Reading view
The toolbar's **Reading view** toggle replaces the workspace with `ui/reading.rs`: the session as a document in timeline
order, with larger text, headings as section titles, labels, "Slide N · deck" for linked notes, posted slides at column
width, and photos and sketches inline. There is no audio. "Copy all" and "Export Markdown" produce the text from
`lectio_core::export::session_markdown`.

## Main flows

### Record → note → stop
```mermaid
sequenceDiagram
  actor S as Student
  participant UI as Views (egui)
  participant App as LectioApp
  participant Rec as Recorder
  participant CB as Audio callback (real-time OS thread)
  participant Ring as rtrb ring (2 s, pre-allocated)
  participant W as Writer thread
  participant Store as JsonFileStore
  S->>UI: Start recording (Cmd+R)
  UI->>App: Action::Recorder(Start)
  App->>Rec: Recorder::start(path) allocates the ring once
  loop every callback (~10.7 ms for 512 frames at 48 kHz)
    CB->>Ring: copy raw interleaved f32, whole frames only (no alloc, no lock)
    CB->>CB: atomics: frames captured, samples dropped, slowest callback
  end
  loop every 10 ms
    W->>Ring: read everything available
    W->>W: SpeechConverter: downmix + downsample to 16 kHz
    W->>W: append to WAV, samples_written += n
  end
  App->>App: each frame: clock.with_total_samples(samples_written)
  S->>UI: types a note, presses Enter
  UI->>App: DraftChanged / SubmitNote
  App->>App: library.with_note(session, draft.anchor_ms, text)
  App->>Store: save (temp file + rename)
  S->>UI: Stop and save
  App->>Rec: finish() drops the stream, writer finalizes WAV
  App->>App: library.with_recording(duration_ms)
  App->>Store: save
```
Pause sets a flag the audio callback checks; paused audio is dropped, so pauses add no samples and no time (ADR-006).
Closing the window mid-recording runs the same `finish_recording` path, so the audio and duration are kept.

### Record → stop in the browser (same app code, `platform/web`)
```mermaid
sequenceDiagram
  actor S as Student
  participant App as LectioApp
  participant WR as WebRecorder (Rust, main thread)
  participant B as Browser (getUserMedia)
  participant WK as lectio-capture worklet (audio thread)
  participant DB as IndexedDB
  S->>App: Start recording
  App->>WR: platform.start_capture(file) returns at once
  WR->>B: getUserMedia({audio, no echo/noise/gain processing})
  B-->>S: "Allow microphone?" (first time)
  B-->>WR: MediaStream
  WR->>WK: addModule(lectio-capture-worklet.js), connect source → worklet
  loop every 4096 samples (~85 ms at 48 kHz)
    WK->>WR: postMessage(Float32Array, transferred)
    WR->>WR: SpeechConverter → 16 kHz i16 in memory, samples_written += n
  end
  App->>App: each frame: clock.with_total_samples(samples_written)
  S->>App: Stop and save
  App->>WR: finish(): stop tracks, close AudioContext, encode WAV
  WR->>DB: put recordings[file] (background, spawn_local)
  App->>DB: save library.json (background, separate write)
  App->>DB: WebPlayer::open reads recordings[file]
```
IndexedDB runs read-write work on the *same* table in the order it was started, so the player's read of `recordings[file]`
always sees the audio that was just written. The library and the audio are different tables, so their two writes are not
ordered against each other: if the tab is closed in the split second between them, the session can say "recorded" while the
audio is missing, and the player then reports "The audio for this session is not in this browser's storage."

### Playback → click a note → seek
```mermaid
flowchart LR
  A[Open a recorded session] --> B[Player::open loads WAV into memory]
  B --> C[Timeline: markers at offset_to_fraction of each note]
  C --> D{Student clicks}
  D -->|note time or text| E[Action::PlayFrom offset_ms]
  D -->|timeline bar| F[fraction_to_offset, snap to a marker within 6 px]
  E --> G[AudioPlayback::seek_ms]
  F --> G
  G -->|native| H1[Player: ms_to_samples sets the read position of the cpal output callback]
  G -->|web| H2[WebPlayer: new AudioBufferSourceNode started at offset]
  H1 --> I[active_note_index highlights the current note]
  H2 --> I
```

### Planned (later phases)
```mermaid
flowchart LR
  C[Recording saved as 16 kHz WAV] --> T[Phase 3: transcribe with a local model] --> F[Phase 3-4: search, study tools]
```

## Threads (native)
In the browser, all Rust code runs on the page's main thread; only the recording worklet (JavaScript) runs on the browser's
audio thread and talks to Rust through `postMessage`.

| Thread | Owns | Talks to others via |
|---|---|---|
| UI (main) | `LectioApp`, `Library`, cpal `Stream` handles | reads `samples_written` and `Recorder::stats()` atomics; `Mutex<PlayState>` |
| Audio input callback (**real-time**) | `rtrb::Producer<f32>` | writes raw samples into the ring; only atomics otherwise. No allocation, no locks, no blocking (ADR-009) |
| WAV writer | `rtrb::Consumer<f32>`, `SpeechConverter`, `hound::WavWriter` | polls the ring every 10 ms; bumps `samples_written`, `peak` |
| Audio output callback (**real-time**) | read-only `Arc<[i16]>` samples | `try_lock` on `PlayState` (plays silence instead of waiting) |

*Real-time thread:* a thread the OS runs at high priority with a hard deadline per audio buffer; if the code on it waits for
anything (memory allocator, a lock, the disk), the deadline can be missed and audio glitches (PROBLEMS P-007).

## Accessibility (Phases 1–2)
- Phase 2: slide images and photos carry alt text ("Week 3, slide 4 of 32", "Photo", "Sketch"), the sketch canvas reports
  itself as a canvas, slides turn with Page Up / Page Down, and every Phase 2 action is a labelled button. The reading view
  uses larger text (18 pt). Drawing a sketch needs a pointer (mouse, trackpad, pen or touch), so it has no keyboard equivalent.
- **Desktop:** every control is a labelled egui widget; eframe's AccessKit integration hands them to the OS screen reader
  (AccessKit supports macOS, Windows and Linux). Not yet tried with a real screen reader. The timeline reports itself as a
  slider ("Recording position 17:40 of 50:00").
- **Web: known gap (P-013).** egui draws on one `<canvas>`, and eframe 0.37 has no AccessKit bridge to the browser's
  accessibility tree, so screen readers can't read the web version yet. Typing and Enter were verified in the browser test;
  Tab navigation and browser zoom were not tested there.
- Keyboard: Tab moves focus; Cmd/Ctrl+R start/pause/resume recording or play/pause; Cmd/Ctrl+Shift+R stop; Cmd/Ctrl+1–4 label the
  moment (Important, Review, Task, Definition) and Cmd/Ctrl+5 saves the typed text as a Heading; Cmd/Ctrl+Shift+N posts the
  next slide; Esc leaves full-screen slides; Cmd/Ctrl+[ and ] skip ±10 s; Cmd/Ctrl+Shift+, and . change the playback
  speed; with the timeline focused, ←/→ skip ±10 s and Home/End jump to the ends.
- Navigation items report themselves as tabs with a selected state; audio sections are buttons named "Play from 0:10".
- Zoom: Cmd/Ctrl + and − (eframe built-in); panels scroll and wrap, so 200% stays usable.
- Label chips use near-black text on light fills (contrast above 7:1), always show the label name, and labelled timeline markers have
  a flag shape, so colour is never the only signal.

## Known limitations (Phases 1–2)
- Phase 2: no camera in the desktop apps (ADR-013); photos come from the file picker.
- Phase 2: PDFs that are password-protected can't be opened (hayro limit, ADR-011); very large, image-heavy decks are untested
  and may render slowly, especially in the browser, where a page renders on the main thread.
- Phase 2: if an import is interrupted after its blob is stored but before the library saves, an unused blob stays behind;
  orphan clean-up only runs on delete (ADR-012).
- The audio stream uses fixed 10-second sections, not sections split at pauses (ADR-015).
- Page search in the slides panel and the "Computer audio" / "Both" sources are placeholders until Phase 3; Tasks is a
  placeholder until Phase 4; there is no print yet (Markdown export only).
- If the app crashes (not a normal close) during recording, the WAV header isn't finalized; the notes are safe (saved on every
  change) but the audio file needs repair. A recovery step is a candidate for a later phase.
- One recording per session; no "record more" after Stop.
- The whole recording is loaded into memory for playback (~115 MB per hour).
- Live microphone capture is checked with `lectio --self-test-audio <seconds>` on a real machine, not in automated tests
  (PROBLEMS P-005, P-007).
- If the writer thread stalls for more than 2 s (the ring size), samples are dropped; the capture view then shows how much.
- Web: the recording is kept in memory until Stop (≈115 MB per hour) and then stored in IndexedDB; a tab crash mid-lecture
  loses that audio (notes are safe). Up to one worklet batch (≈85 ms) at the very end is not saved. Browsers may clear a
  site's storage under disk pressure unless it is marked persistent (`navigator.storage.persist()`, not requested yet).
- Web: tested in headless Chrome only (fake microphone); Safari and Firefox are untested (ADR-010).

## Design system (ADR-014)
The layout is described in "Screens and workspace" below; this section covers the look.

- **Tokens:** every colour, size, spacing and radius comes from `ui/theme.rs` (light + dark `Palette`, `label_colors`, type
  scale 12/13/15/18/24, spacing 4–32, radius 6/10). A unit test checks WCAG AA contrast for all text and marker pairs.
- **Fonts:** Inter (text), JetBrains Mono (timestamps only), Phosphor (icons) in **separate families**; mixed icon + text is
  built with `widgets::icon_text` (PROBLEMS P-018).
- **Controls:** `ui/widgets.rs`: icon buttons (tooltip + accessible name), primary/subtle buttons, time pills, label and neutral
  chips, a switch, cards, empty states, and label marker shapes (diamond/circle/square/triangle) so labels never differ by
  colour alone.
- **Names for tests:** `widgets::name` gives a control its accessible name and records `(name, rect)` for the frame; the web
  build publishes them as `window.__lectioWidgets` when the URL has `#test` (P-020).
- **Safety:** deleting a note or slide deck shows an **Undo** toast for 8 s (`ui_state.rs`). The library value from before
  the delete is kept (it's immutable, ADR-007), and orphaned files are deleted only after the undo window closes.
- **Empty states:** no course → "Add a course"; no sessions → a hint to press "New session"; no slides → "Import PDF";
  all panels hidden → "Show the feed". One primary action each.
- **Reading view:** a centred 720 px column, 17 px text with 27 px line height; headings as section titles and posted
  slides at column width; "Copy all" and "Export Markdown" at the top.

## Screens and workspace (ADR-015)
```mermaid
flowchart LR
  subgraph List["List screens (nav column: Home · Collections · Tasks · Search · Trash)"]
    HOME["Home: every session, newest first<br/>title · course · date · length · count per label · ⋯ Move to trash<br/>New session (asks which course when there are several)"]
    COLL["Collections: courses and their sessions"]
    TRASH["Trash: Restore · days left (30)"]
  end
  HOME -- "open a session" --> WS
  COLL -- "open a session" --> WS
  WS -- "back arrow (not while recording)" --> HOME
  subgraph WS["Workspace"]
    direction TB
    TOP["Toolbar: ← · title (rename) · course · date · Reading view · Layout ▾ · theme · settings"]
    subgraph Panels
      direction LR
      SL["Slides<br/>deck · page search (Phase 3) · page strip<br/>page (zoom, full screen) · ‹ n/N › · Post this slide"]
      FD["Feed<br/>cards in time order: notes, photos, sketches, slide items<br/>Heading notes as section titles"]
      AU["Audio stream<br/>10 s sections, tinted by label<br/>live level · silence hint"]
    end
    IB["Input bar: record/pause · time · source (Microphone) · level · stop — or play · ±5 s · timeline<br/>note input · photo/camera/sketch · send · Heading Important Review Task Definition · Post next slide"]
    TOP --> Panels --> IB
  end
```
- **Layout:** the Layout menu hides Slides, Feed or Audio (saved in `Library.preferences`). With the feed hidden, the slides take
  the middle. Below 900 px the workspace shows one panel at a time, picked with tabs; the input bar stays.
- **Feed:** cards are at most 760 px wide. During playback the item playing is highlighted and scrolled into view when it
  changes (PROBLEMS P-022); during recording the feed sticks to the newest item.
- **Audio stream:** `audio_sections(duration)` gives the sections; `section_labels` gives each one's tint; `section_at` the
  playing one. Clicking a section plays from its start. While recording, the column grows and a level bar shows sound
  arriving; after 10 s below the silence level a hint suggests checking the microphone.

### Record with slides → the feed fills in
```mermaid
sequenceDiagram
  actor User
  participant UI as Input bar / Slides panel
  participant App as LectioApp (workflow.rs)
  participant Lib as Library (immutable)
  User->>UI: Start recording (deck loaded)
  UI->>App: Action::Recorder(Start)
  App->>Lib: with_slide_item(session, 0, shown slide)
  Note over Lib: "Slide 1" appears in the feed at 0:00
  User->>UI: Post next slide (Cmd/Ctrl+Shift+N)
  UI->>App: Action::PostSlide { next: true }
  App->>App: viewer.page += 1
  App->>Lib: with_slide_item(session, now, page)
  User->>UI: types "Overview", presses Heading
  UI->>App: Action::LabelMoment(Heading)
  App->>Lib: with_note(session, typing start, "Overview", [Heading])
```

### Trash → restore or expire
```mermaid
flowchart LR
  A["⋯ Move to trash"] --> B["with_session_trashed: deleted_unix_ms = now"]
  B --> C{"Within 30 days?"}
  C -- "Restore" --> D["with_session_restored: back on Home"]
  C -- "no, at the next start" --> E["without_expired_trash: session removed;<br/>its blobs and recording deleted"]
```

## Phase 3: text and audio

### Transcribe a recording (ADR-016)
```mermaid
sequenceDiagram
  actor U as User
  participant App as LectioApp (speech_actions.rs)
  participant S as SpeechService
  participant W as Desktop thread / browser Web Worker
  participant E as lectio_speech (Engine, Transcription)
  participant L as Library (immutable)
  U->>App: Transcript tab › Transcribe (model, language)
  App->>L: with_transcript(empty, complete=false)
  App->>S: Transcribe{session, file, model, language, start_ms}
  S->>S: model files present? else download (progress, SHA-256 check)
  S->>W: load the model (once), send the 16 kHz samples
  loop each 30 s window
    W->>E: Transcription::next → mel → encoder → greedy decoding with timestamps
    E-->>W: segments aligned to the recording (lectio_core::transcript)
    W-->>App: Segments + Progress events (polled each frame)
    App->>L: with_transcript(merged segments) and save
  end
  W-->>App: Finished{complete}
```
- **Stop** sets a flag the loop checks between windows; what is done stays, and "Continue transcribing" starts after the
  last finished segment.
- **Live captions:** every 2 s while recording, `AudioCapture::recent_audio(8 s)` → `Caption` request → the same engine →
  the caption strip above the input bar. Desktop recorders keep the last 30 s for this.
- **Desktop CLI:** `lectio --transcribe file.wav --model tiny|base|small [--language xx]` prints segments and the real-time
  factor (the numbers in ADR-016).

### Playback: speed and filters (ADR-017)
```mermaid
flowchart LR
  subgraph Desktop["Desktop (cpal output callback, no allocation)"]
    A1["16-bit samples in memory"] --> B1["TimeStretcher (WSOLA, 50-200%)"] --> C1["FilterChain"] --> D1["linear resample to the speaker rate"]
  end
  subgraph Browser["Browser"]
    A2["WAV from IndexedDB"] --> C2["FilterChain, a slice per frame (if filters are on)"] --> D2["audio element: playbackRate + preservesPitch"]
  end
```
"Save cleaned copy" runs the same `FilterChain` over the whole recording and stores `<session>-clean.wav`; playback then
uses it, with the live filters off.

### Recording sources and screenshots (ADR-019)
```mermaid
flowchart LR
  M["Microphone (cpal input / getUserMedia)"] --> R1["ring"] --> X["writer: each to 16 kHz, mix, WAV + last 30 s"]
  C["Computer audio: macOS process tap / WASAPI loopback / Pulse-PipeWire monitor / getDisplayMedia"] --> R2["ring"] --> X
  SC["Screenshot: xcap main display / getDisplayMedia frame"] --> CR["drag to choose the part"] --> PN["photo note at the moment pressed"]
```

### Search (ADR-018)
The Search page calls `lectio_core::search::search(library, query, 60)` when the text changes and when the page opens. Each
hit carries the session, the moment and the slide; opening it selects the session, seeks, and shows the slide. Slide text is
read once per deck (`media/slide_text.rs`) into `SlideDeck.page_text`; the slides panel's box lists matching pages
(`search_deck`).

### Phase 3 accessibility
New controls have accessible names: "Speech model", "Spoken language", "Transcription progress", "Play from 0:12" (each
segment), "Search everything", "Search slides", "Audio source", "Live captions", "Sound filters", "Playback speed",
"Screenshot area". The caption strip uses large text (18 px).

### Phase 3 known limitations
- No speaker separation (ADR-020). Live captions need the model downloaded first and lag about 2 s behind.
- The tiny model mishears technical words; base and small are better but slower (numbers in ADR-016).
- Small is desktop-only (browser tab memory). The browser build has no WebAssembly SIMD yet, so it is slower there.
- Computer audio: macOS 14.2+ and permission; Firefox and Safari can't share tab sound; if the default output device also
  has a microphone, macOS records that microphone instead.
- Slide text: decks from before Phase 3 are read the first time their session opens; scanned (image-only) PDFs have none.
- Search scans everything each time: fine for a few terms of lectures, would need an index for much more.
