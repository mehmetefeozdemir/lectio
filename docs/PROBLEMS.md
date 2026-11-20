# Problems & Fixes

Every bug, dead end and wrong turn while building Lectio, with the root cause, what I tried and the fix.

## P-001: The setup script failed on a clean Mac (no Homebrew)
- **Phase:** 0 (environment setup)
- **Symptom:** `./scripts/setup-mac.sh` stopped right away with `Install Homebrew first`.
- **Root cause:** The first version of the script assumed Homebrew was installed, which isn't true on a fresh macOS install.
- **Tried:** Ran the script as written. It exited at the Homebrew check.
- **Fix:** Rewrote `scripts/setup-mac.sh` to use the official installers instead (`sh.rustup.rs` for Rust) and to check for the
  Xcode Command Line Tools first. Homebrew is no longer needed.
- **Lesson:** Setup scripts should depend on as little as possible and check each prerequisite with a clear message, not assume the
  developer's machine looks like mine.

## P-002: `cargo add` turned off eframe's default features by accident
- **Phase:** 1
- **Symptom:** After `cargo add eframe image --no-default-features`, `Cargo.toml` had `eframe = { version = "0.37.0", default-features = false }`. A UI framework with no window backend can't open a window.
- **Root cause:** `--no-default-features` applies to **every** crate in that `cargo add` command, not only the last one. I meant it for `image` (to pull in only the PNG encoder).
- **Tried:** Caught it by reading `Cargo.toml` before the first build, so nothing failed at runtime.
- **Fix:** Rewrote `Cargo.toml` by hand: `eframe = "0.37"` (defaults on: wgpu renderer, AccessKit screen-reader support) and `image = { version = "0.25", default-features = false, features = ["png"] }`.
- **Lesson:** Add crates with different feature needs in separate commands, and read the manifest diff after tools edit it.

## P-003: Borrow-checker error E0502 when drawing the UI
- **Phase:** 1
- **Symptom:** `cargo build` failed in `src/app.rs`:
  `error[E0502]: cannot borrow self.buffers as mutable because it is also borrowed as immutable`
  on `ui::draw(ui, &self.view(), &mut self.buffers, ...)`.
- **Root cause:** `self.view()` builds a read-only snapshot that borrows **all** of `self`. In the same call I also asked for `&mut self.buffers` (the text the user is typing). Rust forbids a mutable borrow of a field while the whole struct is borrowed read-only, because the reader could see the data change underneath it.
- **Tried:** 1) Considered putting the text buffers inside `View` as `&mut`. That makes the "read-only snapshot" not read-only, which goes against ADR-007. 2) Considered splitting `view()` into a function that borrows each field separately: correct, but a long parameter list.
- **Fix:** In `LectioApp::ui`, move the buffers out for the frame with `std::mem::take(&mut self.buffers)`, draw, then put them back. `mem::take` swaps in an empty default value, so there is never a second reference.
- **Lesson:** In Rust, a method that borrows `&self` borrows every field. When a frame needs one field mutable and the rest read-only, take that field out temporarily or borrow fields separately.

## P-004: `f32::from_sample` not found (E0599) in the recorder
- **Phase:** 1
- **Symptom:** `error[E0599]: no associated function or constant named from_sample found for type f32` in `src/audio/recorder.rs`.
- **Root cause:** `from_sample` comes from cpal's `Sample` trait. In Rust a trait's methods can only be called when the trait is imported (`use`), even though `f32` implements it.
- **Fix:** `use cpal::{FromSample, Sample, SampleFormat, SizedSample};`.
- **Lesson:** "Method not found" on a type that should have it usually means a missing trait import; the compiler's `help:` line names the trait.

## P-005: Can't fully verify microphone recording from an automated run
- **Phase:** 1
- **Symptom:** The automated check can build, run the tests, and take a screenshot, but it cannot click "Start recording" and speak. On macOS the first microphone use also shows a permission prompt that a person has to answer.
- **Root cause:** Real audio capture depends on hardware and an OS permission. Neither exists in a headless check.
- **Tried / mitigation:** Moved everything that *can* be tested without hardware into `lectio-core` (state machine, sample-based clock, ms↔sample conversion, downsampler, storage) and tested it: 47 tests. The `--screenshot` mode uses built-in sample data (`src/demo.rs`) and never opens the microphone, so checks never trigger a permission prompt.
- **Still open:** A manual test on the real Mac: record ~30 s with a pause in the middle, add notes, stop, click a note, and confirm the audio jumps to it. If Lectio shows "Could not open the microphone", allow microphone access for Terminal in System Settings > Privacy & Security > Microphone.
- **Update (P-007 work):** a manual test found P-007. Live capture can now be checked with one command,
  `lectio --self-test-audio <seconds>`; a 5 s run on the MacBook Air microphone recorded real sound with 0 dropped frames.
- **Lesson:** Keep hardware-dependent code thin, and test the logic around it as plain functions.

## P-006: `timeout` command doesn't exist on macOS
- **Phase:** 1
- **Symptom:** Running the screenshot test as `timeout 90 ./target/debug/lectio --screenshot shots/now.png` gave `zsh: command not found: timeout`.
- **Root cause:** `timeout` is part of GNU coreutils (Linux). macOS ships BSD tools without it.
- **Fix:** Started the app in the background and polled it with `kill -0 $PID` once a second for up to 90 s, then killed it if it was still running. The app also guards itself: `src/screenshot.rs` exits with an error if no picture arrives within 1 200 frames.
- **Lesson:** Don't assume Linux tools in scripts meant for a Mac; build the safety limit into the program itself where possible.

## P-007: "Microphone problem: A buffer underrun or overrun occurred" on a real recording
- **Phase:** 1 (found in the manual mic test on the MacBook Air, `cargo run` = debug build)
- **Symptom:** During a real recording, Lectio showed `Microphone problem: A buffer underrun or overrun occurred`.
- **Suspected root cause (to confirm):** the input callback in `src/audio/recorder.rs` runs on the OS real-time audio thread but does heap work for every chunk. It allocates a `Vec<f32>` *per audio frame* (`frame.iter()...collect()`), another `Vec` for the resampled output, and sends that `Vec` through an `mpsc` channel. A debug build makes this much slower. If the callback doesn't return in time, CoreAudio's input buffer overflows (an *overrun*).
- **Verdict on the suspected cause: partly confirmed, partly rejected.**
  - *What the error means (confirmed from cpal's source):* on macOS, cpal 0.18 raises `ErrorKind::Xrun`
    ("A buffer underrun or overrun occurred") from a listener on CoreAudio's `kAudioDeviceProcessorOverload` property
    (`cpal-0.18.2/src/host/coreaudio/macos/mod.rs`). That property fires when the device's real-time I/O thread **missed its
    deadline**. So the error is about timing on the audio thread. It doesn't mean our buffer filled up.
  - *Confirmed: the callback broke all three real-time rules.* The self-test showed the MacBook Air mic delivers 512 mono
    frames per callback at 48 kHz, so the old callback did **513 heap allocations per call** (one 1-element `Vec` per frame,
    plus the output `Vec`). It also sent through an unbounded `std::sync::mpsc` channel, which allocates a new internal block
    every few dozen messages. And the error callback locked a `Mutex` to store the message, even though cpal notes that
    overload notifications fire on the real-time thread.
  - *Rejected: "the debug build makes it too slow".* I measured the old callback body in a throwaway benchmark (20 000 calls,
    512 frames each, with a second thread allocating like the UI does). **Debug: median 79 µs, p99.9 114 µs, max 134 µs.
    Release: median 10 µs, max 43 µs.** The deadline for 512 frames at 48 kHz is **10 667 µs**. Debug is 8× slower, but still
    only ~1% of the budget, so the *average* cost can't explain a missed deadline.
  - *Most likely trigger (inferred, not proven):* a rare long stall *inside* an allocation, for example `malloc` waiting on a
    lock held by the UI thread, or a page fault when the allocator asks the OS for fresh memory. These are unbounded and rare,
    which is why a 20 000-call benchmark doesn't reproduce them and why real-time code must not allocate at all. An overload
    caused by another app on the same device is also possible; the new counters tell these cases apart.
- **Tried:**
  1. Benchmarked the old callback (above). It ruled out "it's just slow" and pointed at worst-case behaviour instead.
  2. Considered `opt-level = 1` for debug builds. Rejected: it makes the average faster but leaves the unbounded allocation stalls.
  3. Considered a bounded `mpsc::sync_channel` + `try_send`. Rejected: no blocking, but still one `Vec` allocation per callback.
- **Fix (ADR-009):** `src/audio/recorder.rs` now uses a **pre-allocated lock-free single-producer/single-consumer ring buffer**
  (`rtrb` 0.4, 2 s of raw audio). The callback only (a) copies raw interleaved `f32` samples into the ring with
  `write_chunk_uninit` + `fill_from_iter`, whole frames only (`lectio_core::realtime::whole_frame_samples`), and (b) updates
  atomic counters: frames captured, samples dropped when the ring is full, callback count, slowest callback time. The error
  callback only increments an atomic overrun counter for `Xrun`; other errors use `try_lock`, which never waits. Downmix,
  downsampling and WAV writing moved to the writer thread through `lectio_core::resample::SpeechConverter`. That function
  carries a half-finished frame over to the next chunk, because a ring read can end in the middle of a stereo pair. 9 new
  tests in `tests/capture_pipeline_tests.rs` check that chunked conversion is **bit-identical** to one big call, including a
  split at every possible index. The capture view now shows lost audio only if it is above 0. A new
  `--self-test-audio <seconds>` flag records with no window and prints the counters.
- **Result (5 s on the MacBook Air microphone, 48 kHz mono f32):**
  `release`: 240 640 frames captured, **0 dropped, 0 overruns**, 470 callbacks, **slowest callback 29 µs** (deadline ~10 666 µs),
  peak 0.211 (real sound, not permission-denied silence). `debug`: 0 dropped, 0 overruns, slowest callback 116 µs.
- **Follow-up:** a later `cargo run --release -- --self-test-audio 120` passed: 0 dropped, 0 overruns, slowest callback 45 µs.
  Still worth doing: a real
  10-minute recording in the app while using other apps, and confirm 0 overruns. If overruns still appear with 0 dropped
  frames, the overload comes from outside Lectio.
- **Lesson:** On a real-time audio thread, the rule is *no allocation, no locks, no blocking*, because what matters is the
  **worst case**, not the average. A benchmark that shows "fast on average" doesn't prove a callback is safe. Also, add
  counters (dropped frames, overruns, slowest callback) so the next report can be diagnosed from data instead of guesses.

## P-008: `uuid` refuses to compile for the browser (`wasm32-unknown-unknown`)
- **Phase:** 1 (platform targets, ADR-010)
- **Symptom:** `cargo check -p lectio-core --target wasm32-unknown-unknown` failed:
  `error: to use uuid on wasm32-unknown-unknown, specify a source of randomness using one of the js, rng-getrandom, or rng-rand features`
  (from `uuid-1.29.3/src/rng.rs:104`).
- **Root cause:** `NoteId::new_random()` uses random (v4) UUIDs. On Windows, macOS and Linux the randomness comes from the OS.
  `wasm32-unknown-unknown` has no OS, so the crate can't guess where randomness comes from and stops the build on purpose.
- **Fix:** In `crates/lectio-core/Cargo.toml`, a wasm-only dependency section turns on uuid's `js` feature, which uses the
  browser's `crypto.getRandomValues`. No Rust code changed, so the core crate is still platform-free.
- **Lesson:** "Compiles for wasm" isn't automatic even for pure-Rust crates: anything that needs randomness, time, files or
  threads has to be told what the browser provides. Check the core crate for wasm early, before the app depends on it.

## P-009: New warnings once the web code was linted (`-D warnings`)
- **Phase:** 1 (platform targets)
- **Symptom:** 1) `cargo check --target wasm32-unknown-unknown`: `warning: use of deprecated method
  web_sys::AudioBufferSourceNode::stop` in `src/platform/web/player.rs`. 2) `cargo clippy -- -D warnings` on both targets:
  `error: this if statement can be collapsed` in `LectioApp::collect_background_errors` (`src/app.rs`). 3) Before that, the
  native build warned `method duration_ms is never used` and `function encode_mono_i16 is never used`.
- **Root cause:** 1) web-sys moved `stop()` to the parent class `AudioScheduledSourceNode`. 2) A nested `if let` that clippy
  wants as one condition. 3) The new `AudioPlayback` trait had a method the app never calls (it gets the duration from the
  model), and the WAV encoder is only *called* by the web recorder, so on native it looked like dead code.
- **Fix:** 1) Call `web_sys::AudioScheduledSourceNode::stop(&source)`. 2) Rewrote it as one `if let`. 3) Removed `duration_ms`
  from the trait (YAGNI); marked `encode_mono_i16` with `#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]`. It is
  still unit-tested on every target.
- **Lesson:** Lint *every* target. Code behind `#[cfg]` is only checked when you build for that target, so CI runs clippy for
  native and for wasm.

## P-010: Headless-Chrome screenshot of the web app stuck on "Loading Lectio…"
- **Phase:** 1 (platform targets, local verification)
- **Symptom:** `chrome --headless --screenshot --virtual-time-budget=15000 http://127.0.0.1:8765/index.html` saved a page that
  only said "Loading Lectio…". The same command with `#sample` showed the full app.
- **Root cause:** Not an app bug. `#sample` doesn't touch IndexedDB; the normal start waits for IndexedDB to open. Chrome's
  `--virtual-time-budget` fast-forwards *virtual* time, so the "15 s" passed almost instantly in real time, before the real
  database work finished. Without the budget, `--screenshot` fires right after the page's load event (≈1 s), also too early.
- **Tried:** 1) `--virtual-time-budget`: too early, as above. 2) `--timeout=12000`: the screenshot was still taken at load.
- **Fix:** Wrote `tools/web-smoke.mjs`, a ~200-line Node script with no npm packages. It drives Chrome through the DevTools
  Protocol (the same API browser automation tools use) and waits in **real time** until the page's status line is removed,
  which `src/entry_web.rs` does once the app has started. It also clicks, types, reloads and saves screenshots. Result:
  `start: started`, no console errors.
- **Lesson:** When a check fails, first ask whether the *check* is right. A screenshot tool that guesses when the page is ready
  will give false failures; wait for a signal the app sets itself.

## P-011: My browser test clicked the wrong button after a reload
- **Phase:** 1 (platform targets, local verification)
- **Symptom:** The scripted web test (record → reload → open "Lecture 1" → Play) ended on an empty "Lecture 2" capture view.
- **Root cause:** The test clicks fixed coordinates. While recording, the sidebar shows an extra line ("Stop the recording to
  switch sessions"), which pushes "Lecture 1" down. I measured its position in that state, but after the reload the line is
  gone, so my click at (69, 166) hit "+ New session" and created Lecture 2. The app did exactly what was clicked.
- **Fix:** Clicked (69, 143), which I measured from a screenshot of the reloaded page. The full flow then passed.
- **Lesson:** Coordinate-based UI tests are brittle: they break when the layout changes. My first idea was to find buttons
  *by name* through the accessibility tree instead, but that turned up a bigger problem (P-013).

## P-012: Linux can't be type-checked from a Mac
- **Phase:** 1 (platform targets, local verification)
- **Symptom:** `cargo check --target x86_64-unknown-linux-gnu` failed in `alsa-sys v0.4.0`'s build script:
  `pkg-config has not been configured to support cross-compilation.` (The Windows check,
  `cargo check --workspace --all-targets --target x86_64-pc-windows-msvc`, passed.)
- **Root cause:** cpal's Linux backend links to the system's ALSA sound library, and its build script asks `pkg-config` where
  ALSA is installed. A Mac has no Linux ALSA to find. The Windows backend talks to WASAPI through pure-Rust bindings, so
  checking it needs no system libraries.
- **Fix:** None needed locally. The Linux build is verified only by the `ubuntu-latest` CI job, which installs
  `libasound2-dev` and the X11/Wayland/xkbcommon packages first.
- **Lesson:** "Cross-platform Rust" stops at C libraries. Know which targets you can check locally and which only CI can prove,
  and say so.

## P-013: In the browser, screen readers can't see Lectio's buttons (accessibility gap)
- **Phase:** 1 (platform targets): found while making the web test sturdier (P-011)
- **Symptom:** A test step that reads the page's accessibility tree through Chrome DevTools
  (`Accessibility.getFullAXTree`) and looks for "Lecture 3" on `index.html#sample` failed:
  `FAIL: expected "Lecture 3" on the page`, although the screenshot clearly shows it.
- **Root cause:** egui draws everything itself onto one `<canvas>`. On desktop, eframe passes every widget to the OS screen
  reader through AccessKit (ADR-003). In the browser, eframe 0.37 has no AccessKit bridge to the DOM: the canvas is the only
  element, so the browser's accessibility tree is empty. eframe's `web_screen_reader` feature only *speaks* text with the
  browser's speech synthesis, which a real screen reader user can't navigate.
- **Tried:** The DevTools accessibility query above (that's how it was found). Removed the `--expect-text` step from
  `tools/web-smoke.mjs` again, since it can't work.
- **Fix:** Not fixed yet. Logged as a known gap in ADR-010 and ARCHITECTURE. It goes against SPEC's "screen-reader
  labels" requirement for the **web** build only; desktop is fine. Options for later: (1) a hidden DOM mirror of the main
  controls (session list, record/stop, notes) kept in sync with the app; (2) an AccessKit web adapter, if egui gains one;
  (3) tell screen-reader users to use the desktop app until then.
- **Fixed in Phase 4b** (ADR-027): option (1), but driven from egui's whole AccessKit tree rather than a hand-picked list,
  through egui's plugin hooks. The check that found this problem is back in the browser test (`--flow a11y`,
  `--expect-ax`) and passes: 448 names in Chrome's accessibility tree on the sample lecture, against none before.
- **Lesson:** "One codebase" doesn't mean "same behaviour everywhere". A non-functional requirement (accessibility) can hold
  on one platform and fail on another, so it needs a check *per platform*.

## P-014: `lifetime may not live long enough` when caching hayro's renderer next to the PDF
- **Phase:** 2 (slides, ADR-011)
- **Symptom:** First build of `src/media/slides.rs`:
  `error: lifetime may not live long enough … has type &hayro::RenderCache<'2> … let's call the lifetime of this reference '1`.
  The browser version of `DeckRenderer` stored `pdf: Pdf` and `cache: RenderCache<'static>` in one struct.
- **Root cause:** hayro's `RenderCache<'a>` *borrows* from the parsed PDF (`'a` is the PDF's lifetime). A struct that owns a
  value and also a borrow of that same value is a *self-referential struct*. Rust forbids it, because moving the struct would
  leave the borrow pointing at the old location. `RenderCache<'static>` claimed the cache borrows nothing, which isn't true.
- **Tried:** Nothing else; the compiler's note named the two lifetimes, which made the cause clear.
- **Fix:** `render_page<'a>(pdf: &'a Pdf, cache: &RenderCache<'a>, …)` states the real relationship. On native the cache is a
  local variable inside the render thread, created after the PDF, so the borrow is fine. In the browser a fresh cache is made
  per page. That loses glyph caching between pages, which is acceptable because the app keeps rendered pages as textures.
- **Lesson:** When a library type has a lifetime parameter, read it as "this borrows from something". Then decide who owns
  that something and keep the borrower in a smaller scope, instead of storing both side by side.

## P-015: Label chips wrapped letter by letter once the slide panel took space
- **Phase:** 2 (UI)
- **Symptom:** The first `--screenshot` with the slide panel showed the "Definition" chip broken into "De / fini / tio / n",
  and each note row was about four times taller than before.
- **Root cause:** The slide panel made the notes column narrower. Each note row was one `horizontal_wrapped` line holding the
  time, the text, the chips and a right-aligned button group. The right-aligned group claimed the rest of the line, so the
  last chip was squeezed into a few pixels and its text wrapped inside the chip.
- **Fix:** `src/ui/notes.rs`: each row is now three columns (time | text, chips and image underneath | buttons), with the
  middle column given a fixed width. `labels::chip` uses `Label::extend()`, so a chip never wraps inside itself; the whole chip
  moves to the next line instead.
- **Lesson:** Adding a panel changes the width every other view gets. Check screenshots after layout changes, because unit
  tests can't see this kind of bug.

## P-016: The sketch canvas opened as a tiny stamp
- **Phase:** 2 (sketches)
- **Symptom:** In the browser test, "Sketch…" opened a window whose white drawing area was about 325×200 points: too
  small to draw a diagram on.
- **Root cause:** `ui/sketch.rs` sized the canvas from `ui.available_width()` *inside* the new `egui::Window`. A window that
  has never been shown starts at egui's narrow default width, so the canvas asked the window how wide to be, and the window
  was waiting for its content to tell it. Neither side had a real width.
- **Fix:** The canvas width now comes from the screen (`ctx.content_rect().width()` minus a margin, clamped to 240–720
  points), and the window is centred with `.anchor(CENTER_CENTER)`. The camera window got a minimum width the same way.
  Strokes are stored in fixed 960×600 canvas pixels, so the saved PNG doesn't depend on the window size.
- **Lesson:** In immediate-mode UIs, sizes flow from the outside in. A new container has no size until something outside
  gives it one, so content shouldn't ask a brand-new container how big it is.

## P-017: The Phase 2 browser test clicked "Review" instead of the note box, and its scroll did nothing
- **Phase:** 2 (verification, `tools/web-smoke.mjs`)
- **Symptom:** 1) A click meant for the note field created a "Review" marked moment instead. 2) `--scroll` left the reading
  view where it was.
- **Root cause:** 1) The same issue as P-011: while recording, the recorder panel is shorter, so everything below it moves up
  23 px; I had measured the field before recording started. (It was a happy accident: it showed that one-tap labels are
  auto-linked to the slide on screen.) 2) egui scrolls the area *under the pointer*, and the script sent a wheel event without
  moving the pointer there first.
- **Fix:** Re-measured the positions in the recording state from a screenshot; `--scroll` now sends `mouseMoved` before the
  wheel. Because fixed positions can't prove much on their own, the flow now ends with `--expect-stored`, which reads
  IndexedDB and checks the counts (3 notes, 3 slide links, 2 images, 1 deck, 3 blobs). I also ran it once with a wrong
  expectation to confirm it fails: `FAIL: stored notes is 3, expected 4`.
- **Lesson:** A UI test that only clicks proves nothing unless it also *checks* something. When the screen can't be read
  (P-013), check the stored data instead.

## P-018: Icons showed up as "M", "y" and "å", and then the text vanished
- **Phase:** UI redesign (ADR-014)
- **Symptom:** First redesign screenshot: the "⋯" menu showed **M**, settings showed **y**, the Review chip showed **å**, the
  slide arrows were wrong arrows. After my first fix, the icons were right but most lowercase letters disappeared
  ("Graph traversal" → "G", "Lecture 1" → "L 1").
- **Root cause:** egui looks up each character in a family's fonts in order. I first listed Inter, then the Phosphor icon
  font. Inter has its own glyphs in the Unicode *private-use area* (codepoints reserved for app-specific symbols), at some of
  the same codepoints Phosphor uses for icons, so Inter's glyphs won. Swapping the order exposed the opposite overlap: the
  Phosphor font file also has glyphs for some Latin letters, so it now hid Inter's letters.
- **Tried:** 1) Inter → Phosphor: wrong icons. 2) Phosphor → Inter: icons right, letters gone.
- **Fix:** Icons get their own font family (`theme::icon_font`, Phosphor only), and text families contain no icon font.
  Mixed "icon + text" strings are built from two parts with different fonts (`widgets::icon_text`, an egui `LayoutJob`).
- **Lesson:** A font fallback list is only safe when the fonts don't overlap. Icon fonts and text fonts can collide in both
  directions, so keep them in separate families and combine them per piece of text.

## P-019: egui 0.37 renamed APIs I used from memory
- **Phase:** UI redesign
- **Symptom:** `error[E0308]: mismatched types … expected Frame, found bool` on `TextEdit::frame(false)`, plus deprecation
  warnings for `egui::WidgetType` ("Renamed to `egui::Role`"), `Ui::id` ("Renamed to `Ui::scope_id`") and `Id::new`.
- **Root cause:** egui changes its API between minor versions (0.x); my habits were from older releases.
- **Fix:** `TextEdit::frame(Frame::NONE)`; accessible roles now use AccessKit's `Role` (`TextInput`, `CheckBox`, `ListItem`,
  `Meter`, `Canvas`); `scope_id()`; `Id::unique()`. CI runs clippy with `-D warnings`, so deprecations can't pile up.
- **Lesson:** With 0.x libraries, read the installed version's source or changelog instead of relying on memory.

## P-020: The redesign broke the browser test's fixed click positions (again)
- **Phase:** UI redesign
- **Symptom:** Every coordinate in `tools/web-smoke.mjs --flow phase2` pointed at the old layout (the same brittleness as
  P-011 and P-017).
- **Root cause:** Clicking by position ties a test to one layout. Finding controls by name the usual way doesn't work here,
  because egui draws on a canvas with no accessibility tree in the browser (P-013).
- **Fix:** Every named control (icon buttons, inputs, session rows, the sketch canvas) records its accessible name and
  rectangle during the frame (`widgets::name`). With `#test` in the URL, the web build publishes that list as
  `window.__lectioWidgets` (`entry_web::publish_named_widgets`). The script's new `--click-name` and `--drag-in` steps use it,
  and the Phase 2 flow passed on its first run with names instead of coordinates.
- **Lesson:** When a UI can't be inspected from outside, have it describe itself. The same names serve screen readers on
  desktop and the test in the browser.

## P-021: The composer's attach icons overlapped the label chips, then the send button was cut off
- **Phase:** UI redesign
- **Symptom:** In the first screenshot the "Attach" icons were drawn on top of the "Definition" chip. After moving them
  into the input row, the browser version (one extra camera icon) clipped the send button.
- **Root cause:** A right-to-left group inside a horizontal row takes whatever width is left, which can be less than its
  content; the second time, I had reserved room for three icons but the web build shows four.
- **Fix:** Label chips sit on their own wrapping row, the attach icons live in the input row, and the text field reserves
  `ATTACH_WIDTH` for all four icons plus spacing (`src/ui/composer.rs`).
- **Lesson:** Platform-dependent controls change widths, so check the layout on every target (here: native and web screenshots).


## P-022: The feed didn't follow playback to the playing note
- **Phase:** Session workflow
- **Symptom:** In the first playback screenshot the playing note (17:05) was highlighted but sat below the visible part of the
  feed, while the audio stream next to it had scrolled correctly.
- **Root cause:** Two bugs on top of each other. (1) The feed scrolled to the item only once, in the first frame, before the
  slide thumbnails and the sketch had loaded; when they arrived, the content grew and pushed the note out of view. (2) My first
  fix stored `(index, height)` and scrolled again "when it changed", but I compared the stored pair with a value built from
  the same stored pair, so the condition was never true.
- **Fix:** A small `Follow` state in `src/ui/notes.rs` keeps the index scrolled to, the content height at that time, and the
  height of the last frame. The feed scrolls again when the playing item changes *or* the height changed since the last
  scroll, and otherwise leaves the user free to scroll during playback.
- **Lesson:** "Scroll to it once" isn't enough when the content loads after the first frame. Also, a change check needs two
  different values to compare; reading back what I just stored only ever compares a value with itself.

## P-023: Adding a fifth label touched more code than the label list
- **Phase:** Session workflow
- **Symptom:** After adding `LabelKind::Heading`: compile errors in `ui/icons.rs`, `ui/theme.rs` and `ui/widgets.rs`
  (`non-exhaustive patterns: LabelKind::Heading not covered`), then a failing test
  (`new_library_has_four_default_labels_and_no_courses`), then later clippy's `unnecessary_sort_by` in the new trash code.
- **Root cause:** The label kind is matched in every place that gives it a look (icon, colours, marker shape). That is what I
  want: Rust's exhaustive `match` lists every place that has to change. The test encoded "four labels" as a fact. The clippy
  lint is newer than the code style I used for sorting newest-first.
- **Fix:** Added Heading's icon (text-H), colours (neutral greys that pass the contrast test in both themes) and marker
  (a flat bar); renamed the test to five labels; sorted with `sort_by_key(|(_, s)| Reverse(s.created_unix_ms))`.
- **Lesson:** Exhaustive matches turn "did I update every place?" into a compiler error. Add new enum variants at the end
  so saved data and shortcuts that depend on order keep their meaning.

## P-024: The speech engine wouldn't build for the browser (`getrandom`)
- **Phase:** 3
- **Symptom:** `cargo check -p lectio-speech --target wasm32-unknown-unknown` stopped with
  `The wasm32-unknown-unknown targets are not supported by default; you may need to enable the "wasm_js" configuration flag`
  from `getrandom 0.3.4`.
- **Root cause:** candle pulls in `rand` 0.9 → `getrandom` 0.3, which refuses to guess where randomness comes from in a
  browser. (Phase 1 hit the same with `uuid` and `getrandom` 0.2, P-008.)
- **Tried:** the error suggests a `--cfg getrandom_backend="wasm_js"` compiler flag as well; I prepared a
  `.cargo/config.toml`, but checked first with the feature alone.
- **Fix:** `getrandom = { version = "0.3", features = ["wasm_js"] }` for the wasm32 target in `crates/lectio-speech/Cargo.toml`.
  The feature alone was enough for this version, so no config file was needed.
- **Lesson:** when a transitive dependency breaks the browser build, `cargo tree -i <crate> --target wasm32-unknown-unknown`
  shows who pulls it in; then fix it once, in the crate that owns that dependency.

## P-025: The top card in the feed looked half hidden under the panel header
- **Phase:** 3 (reported after the session-workflow screens)
- **Symptom:** when the feed was scrolled, the first visible card looked tucked under the "Feed · 12" header and faded out.
- **Root cause:** egui 0.37 paints a *scroll fade*, a 20 px gradient in the panel's background colour, over the top and
  bottom of a scroll area that has more content. The feed's cards are white on a grey panel, so the grey gradient over a
  white card read as "covered", and there was no line between header and list.
- **Fix:** in `ui/notes.rs` the feed turns the fade off (`ui.spacing_mut().scroll.fade.strength = 0.0`), draws a 1 px divider
  under the header, and adds a small top margin inside the list.
- **Lesson:** a library's default visual effect can clash with a design system; check edge states (scrolled, empty, long)
  in screenshots, not only the first view.

## P-026: The transcript panel grew and crushed the feed into a thin column
- **Phase:** 3
- **Symptom:** in the browser flow screenshot (1400 px wide) the right panel took about 1000 px and the feed showed its hint
  text one letter per line.
- **Root cause:** egui side panels grow to fit their content's width. Something in the Transcript tab (the settings row and
  long lines before wrapping settled) asked for more width on the first frames, and a resizable panel keeps the size it grew to.
- **Fix:** `ui/workspace.rs` caps each side panel at 30% of the window (`SIDE_PANEL_MAX_SHARE`), so the feed always keeps
  at least 40%.
- **Lesson:** give every resizable panel a maximum, not just a minimum and a default.

## P-027: candle's mel spectrogram starts threads, which the browser build can't
- **Phase:** 3
- **Symptom:** none at run time: I found it while reading `candle-transformers`' `whisper/audio.rs` before using it. It calls
  `std::thread::scope` with up to 12 threads; on `wasm32-unknown-unknown` creating a thread panics.
- **Root cause:** candle's helper is written for desktops.
- **Fix:** my own single-threaded spectrogram in `crates/lectio-speech/src/mel.rs` (Hann window, `rustfft`, Slaney mel
  filters, Whisper's log scaling), with tests for the mel scale, the filter shapes, and that a 1 kHz tone lights up the right
  band. It takes milliseconds per 30-second window, so threads weren't needed anyway.
- **Lesson:** "it compiles for wasm" doesn't mean "it runs in the browser"; read what a helper does before relying on it.

## P-028: The browser test's fake microphone recorded a beep instead of my speech file
- **Phase:** 3
- **Symptom:** the Phase 3 browser flow recorded 20 s with Chrome's fake microphone set to a speech WAV
  (`--use-file-for-fake-audio-capture`), but the transcript was only "you", which is what Whisper typically says about
  silence or a tone.
- **Root cause:** the WAV, made with macOS `afconvert`, had an `FLLR` padding chunk between the format and data chunks
  (`xxd` showed `FLLR` at byte 36). Chrome's fake-audio reader expects the data right after the format, rejected the file,
  and fell back to its default beep.
- **Tried:** a plain rewrite of the WAV (Python's `wave` module) — still the beep in headless Chrome.
- **Fix:** the flow now records (testing capture), then puts a known 15 s speech clip in place of that recording in
  IndexedDB (`--replace-recording`), reloads, and transcribes: 5 segments came back in the worker, with the right words.
- **Lesson:** separate what a test is checking: microphone capture and transcription are now tested by different steps, so a
  quirk of the fake microphone can't hide a transcription bug (or fake a pass).

## P-029: trunk names the worker's files with content hashes
- **Phase:** 3
- **Symptom:** the app has to start the worker with `new Worker(url)`, but trunk writes `lectio-worker-<hash>_loader.js`,
  and the hash changes with every build.
- **Root cause:** trunk's default `filehash = true` (good for browser caching of the main app, whose script tag trunk writes
  itself); for a worker the app starts from Rust, nothing rewrites the URL.
- **Fix:** `filehash = false` in `Trunk.toml`, and `data-loader-shim` on the worker's `<link>`, so trunk writes a small
  `lectio-worker_loader.js` that loads the worker's WebAssembly. The app uses that fixed name (`platform/web/speech.rs`).
- **Lesson:** a build tool's "rename for caching" must be matched by every place that refers to the file by name.

## P-030: Search snippets started in the middle of a word ("…oday we look at…")
- **Phase:** 3
- **Symptom:** in the browser flow, searching "graph" showed "…oday we look at GraphTraversal, a graph is…".
- **Root cause:** the snippet started a fixed 30 characters before the match, wherever that landed.
- **Fix:** test first (`snippets_start_at_the_beginning_of_a_word`, failed), then `search.rs` steps the start back (at most
  15 characters) to the beginning of the word; reaching the start of the text means no "…".

## P-031: New lints on new code (clippy `-D warnings`)
- **Phase:** 3
- **Symptom:** `manual implementation of .is_multiple_of()` in the speed label, `large size difference between variants` for
  the browser player's state enum, and `unused dependency lectio-speech` for the worker crate on the desktop target.
- **Fix:** `percent.is_multiple_of(100)`; `State::Ready(Box<Loaded>)`; the worker's dependencies moved under
  `[target.'cfg(target_arch = "wasm32")'.dependencies]`, since on the desktop the worker binary is empty.
- **Lesson:** run clippy for both targets (`--workspace` and `--target wasm32-unknown-unknown`): each target sees different code.

## P-032: The 0.5B study model copied the prompt's template and echoed my notes
- **Phase:** 4a
- **Symptom:** the first quiz from the sample lecture had the choices `A) choice`, `B) choice`…, and outline headings were
  whole transcript lines, word for word, with their `(17:05)` time in front (`lectio --ai-benchmark small`).
- **Root cause:** a 0.5B model imitates the *shape and words* of what it is shown. A template with placeholder words is,
  to it, an example to copy, and a numbered list of source lines invites it to repeat them.
- **Tried:** stronger wording ("do not copy the template"). No change: small models follow examples better than rules.
- **Fix:** tests first (`questions_with_placeholder_or_repeated_choices_are_dropped`,
  `outline_lines_that_echo_the_material_with_its_time_are_dropped`), then: every prompt ends with a real worked example
  on another topic (the solar system), and the parsers drop placeholder answers, questions whose choices repeat, and
  outline titles over six words that copy a source line. With the same sample, 6 outline items and 3 of 3 questions were kept.
- **Lesson:** with small models, show one good answer instead of describing the format, and check the output in code.

## P-033: Generated items lost their citations, and acronyms like "DFS" were not matched
- **Phase:** 4a
- **Symptom:** many 0.5B outline items and questions had no `[S12]` marker, so the rule "drop items without a source"
  dropped good items. When I matched items to sources by shared words instead, "Which structure does DFS use?" matched
  nothing.
- **Root cause:** the model often leaves the marker out. My first word matcher ignored words shorter than four letters,
  which removed exactly the acronyms that carry the meaning in a CS lecture.
- **Fix:** `sources::best_match` cites the source that shares the most distinctive words (at least two), counting words of
  three letters or more but skipping a short list of common ones ("the", "and", "for"…). Test:
  `an_item_without_source_numbers_is_cited_to_the_line_it_matches`.
- **Lesson:** test data should look like the real data, and lecture notes are full of three-letter acronyms.

## P-034: The schema v5 change broke the build and one old test
- **Phase:** 4a
- **Symptom:** (1) `field cards of struct Library is private` where the study module built a new library with
  `..lib.clone()`; (2) `the trait Eq is not implemented for f32` once flashcards stored their SM-2 ease in the library;
  (3) the old test that checked "version 4 is written" failed (`left: 5, right: 4`).
- **Root cause:** (1) struct-update syntax needs every field visible in that module; (2) `f32` has no total equality
  (`NaN != NaN`), so a struct holding one can't derive `Eq`; (3) the test pinned the old version number on purpose.
- **Fix:** small `with_card_list` / `with_focus_log` constructors in `model.rs`; `Library` derives `PartialEq` only (nothing
  needed `Eq`); the old test was replaced by `a_version_4_library_loads_with_no_study_data`, which reads a v4 file and
  checks that version 5 is written.
- **Lesson:** a test that pins a schema version is meant to fail when the version changes. Replace it with one for the new
  version *and* one that reads the old files.

## P-035: The study model never finished an outline in the browser
- **Phase:** 4a
- **Symptom:** in headless Chrome the outline stayed at "Writing the outline: part 1 of 1…" for 25 minutes, with no error;
  the same prompt takes about 10 s on the desktop.
- **Root cause:** candle has fast WebAssembly kernels for the quantized formats the model uses (Q4_K, Q6_K), but they are
  compiled only when the `simd128` target feature is on (`#[cfg(target_feature = "simd128")]` in
  `candle-core/src/quantized/k_quants.rs`). My browser build didn't turn it on, so every matrix product ran in plain scalar
  code.
- **Fix:** `.cargo/config.toml` adds `-C target-feature=+simd128` for `wasm32-unknown-unknown`. The outline then came back
  in headless Chrome: 780 tokens (prompt and reply) in 93 s, about 8 tokens/s, against roughly 60 on the desktop.
  WebAssembly SIMD needs Chrome 91, Firefox 89 or Safari 16.4 (all from 2021–2023).
- **Lesson:** when a library has several code paths per CPU, check which one my target actually compiles. A missing
  target feature doesn't fail; it is just slow.

## P-036: Outlines of a short session were empty, and headings pointed at the wrong moment
- **Phase:** 4a
- **Symptom:** (1) for a session with six short notes, "Write an outline" ended with "The model's outline had no lines
  Lectio could check". (2) With fourteen notes it wrote eight good titles, but "Depth-first search details" started at
  0:11 (the Queue definition) instead of 0:18.
- **Root cause:** (1) with so little material the 0.5B model copied each note back as a "heading", and the parser drops
  copied lines (P-032), so nothing was left. (2) The model numbered its headings `[S1]`, `[S2]`, `[S3]`… in order instead of
  citing, so the numbers pointed at unrelated lines. I reproduced both on the desktop with
  `lectio --ai-benchmark small --saved` on a copy of the browser test's notes.
- **Fix:** tests first, then: (1) `prompts::enough_for_outline` asks for at least 150 tokens (about 600 characters) of
  material and otherwise says so before calling any model. My first limit, 250, was a guess and was wrong: the browser
  test's 13 notes (232 tokens) were refused, and even the 14 notes that gave eight good headings were only 247. 150 sits
  between the measured cases (6 notes ≈ 95 tokens only echoed; 13–14 notes worked). (2) `sources::checked_ids` treats a cited line that shares fewer
  than two distinctive words with the item as a wrong number, and uses the best-matching line instead (if there is one).
  Tests: `a_handful_of_short_notes_is_too_little_for_an_outline`,
  `a_cited_line_that_shares_too_little_with_the_item_is_taken_as_a_wrong_number`. One older test had to change: it cited an
  unrelated slide line on purpose, which the new rule now corrects.
- **Lesson:** "it has a citation" is not the same as "the citation is right"; a cheap check against the item's own words
  catches most wrong numbers from a small model.

## P-037: Outline titles in the browser started with "- "
- **Phase:** 4a
- **Symptom:** the first outline written in the browser showed titles like "- Breadth-first search (BFS) is a useful…".
- **Root cause:** the model wrote two list markers (`- - Title`); the parser removed only the first. The browser's answer
  differed from the desktop's for the same prompt, most likely because the maths runs on different vector instructions
  (WebAssembly SIMD vs ARM NEON) and tiny rounding differences change which word greedy decoding picks.
- **Fix:** test first (`doubled_list_markers_are_not_part_of_a_title`), then `outline::without_marks` strips repeated
  `-`, `*`, `•` and `1.` markers.
- **Lesson:** the same model and prompt can answer differently on another platform, so the parser has to accept every
  shape seen on any of them.

## P-038: Linux CI failed to link: `unable to find library -lgbm`
- **Phase:** 3 (found on the first CI run after Phase 3)
- **Symptom:** The `native (ubuntu-latest)` job compiled everything and then failed at the final link step: `rust-lld: error: unable to find library -lgbm`. Windows, macOS and the web build passed.
- **Root cause:** The screenshot library (`xcap`) captures Wayland screens through `libwayshot`, which links to the system's GBM library (Mesa's buffer manager). The CI image had PipeWire and DBus installed for system audio but not GBM. I couldn't see this on a Mac because Linux can't be linked from macOS (see the earlier Linux type-check problem).
- **Fix:** Added `libgbm-dev` to the CI's Linux packages, plus `libegl1-mesa-dev` and `libdrm-dev`, which the same capture path uses.
- **Verified:** The next CI run (44633484797) passed on all four jobs, Linux included.
- **Lesson:** Adding a dependency with native code means checking which system libraries it links on every OS, not only whether it compiles. The CI matrix is what caught it.

## P-039: The app crashed at the end of playback at a non-1x speed
- **Phase:** 3 (found in manual testing after Phase 4a)
- **Symptom:** The desktop app aborted with `panicked at crates/lectio-core/src/stretch.rs:70:19: index out of bounds: the len is 352768 but the index is 352768`.
- **Root cause:** At speeds other than 1x, `TimeStretcher::best_match` searches ±10 ms around the next piece for the best waveform match. Near the end of a recording the window's lower bound (`nominal - search`) could be larger than its upper bound (`len - frame`). The loop ran `lowest..=highest.max(lowest)`, so it still tried `candidate = lowest`, and `candidate + i` read exactly one sample past the end. The existing tests played to the end only at lengths where this window never inverted.
- **Fix:** If the window is empty (`lowest > highest`), `best_match` returns `nominal`, and `next_piece` already stops cleanly when a full piece no longer fits. The loop now only runs over valid candidates.
- **Test:** `every_speed_plays_to_the_end_of_any_length_without_panicking` plays 10 recording lengths (0 to 352,768 samples, including the crashing one) at all 7 speeds to the end. It fails with the same panic on the old code and passes on the new code.
- **Lesson:** Boundary bugs hide at the end of buffers. A test that sweeps lengths and parameters together catches cases a few hand-picked examples miss.

## P-040: A session listed twice on the Collections page shared its widget ids
- **Phase:** 4b
- **Symptom:** on the new Collections page, the session cards of "Graph traversal" were outlined in red with egui's debug
  warning "First use of widget ID 9194" / "Second use of widget ID 9194". Clicks on the second card could reach the first.
- **Root cause:** the page lists a session under each collection it is in *and* under its course. Each card's click area
  has the id `scope_id().with(("open", session.id))`; both cards were in the same scroll area, so the ids were equal.
- **Fix:** each collection and each course section draws inside `ui.push_id(collection.id / course.id, …)`, so the same
  session gets a different id in each section. The warning is gone in the screenshot.
- **Lesson:** an id made from the *data* (a session id) is only unique if each item is shown once; when the same item can
  appear in several places, the place has to be part of the id.

## P-041: At 200% zoom, "Post next slide" covered the last label chip
- **Phase:** 4b (accessibility pass)
- **Symptom:** with `LECTIO_ZOOM=2`, the input bar's "Definition" chip was half hidden under "Post next slide".
- **Root cause:** the chips and the button shared one wrapping row, with the button laid out right-to-left at the end; when
  the row was too narrow, the right-aligned layout started on top of chips that were already placed.
- **Fix:** below 820 points the chips get their own wrapping row and "Post next slide" goes on the next line.
- **Lesson:** checking 200% zoom with a real screenshot finds layout overlaps that look fine at 100%.

## P-042: Operators inside fractions had no space around them
- **Phase:** 4b
- **Symptom:** in the quadratic formula's sample picture, the numerator read "−b±√b²−4ac" with no space around ± and −.
- **Root cause:** my layout laid out a fraction's numerator and denominator as *scripts*, and scripts skip the spacing
  around binary operators and relations (as in TeX, where subscripts are tight). Fractions in TeX keep that spacing.
- **Fix:** a fraction's parts keep the script state of the fraction itself; only superscripts and subscripts are tight.
- **Lesson:** I found this by looking at rendered samples (`cargo test write_formula_samples -- --ignored`), not from a
  unit test: for layout code, a picture of a few real formulas is a cheap, effective check.

## P-043: Lints after Phase 4b's new modules (clippy `-D warnings`)
- **Phase:** 4b
- **Symptom:** (1) `large size difference between variants` for `SavedData` once `Library` got a collections list;
  (2) `function ... is never used` in a release build for the screen-reader mapping and the icon's PNG writer; (3) `use of
  deprecated type alias accesskit::Tree: Use TreeInfo instead`; (4) `field assignment outside of initializer for an
  instance created with Default::default()` for a tiny-skia `Paint`.
- **Root cause:** (1) one variant now holds a much bigger value; (2) the mapping is only used by the browser build (and by
  the desktop's tests), the PNG writer only by a test, so the desktop's non-test build saw dead code; (3) and (4) newer
  library and lint versions.
- **Fix:** `Valid(Box<Library>)`; `#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]` on the mapping module and
  `#[cfg(test)]` on the PNG writer; `TreeInfo`; a struct literal for `Paint`.
- **Lesson:** code used by only one target, or only by tests, is dead code everywhere else. The packaging script's
  release build showed it first; every build (each target, with and without tests) is worth running before calling a
  change done.


## P-044: The Linux desktop screenshots panicked: `libxkbcommon-x11.so could not be loaded`
- **Phase:** 4b (found in the CI run after Phase 4b)
- **Symptom:** The CI run was green, but the `native (ubuntu-latest)` job carried an `exit code 101` annotation. The advisory "Desktop screenshots" step had panicked at `xkbcommon-dl-0.4.2/src/x11.rs:59:28: Library libxkbcommon-x11.so could not be loaded.` The step is `continue-on-error`, so the job still passed and `gh run view --log-failed` showed nothing.
- **Root cause:** On X11 (which Xvfb provides), winit loads the keyboard library `libxkbcommon-x11` with `dlopen` at runtime instead of linking it. The CI installed `libxkbcommon-dev`, which has the headers and the base library but not the X11 part, and the build doesn't notice because nothing links against it.
- **Fix:** Added `libxkbcommon-x11-0` to the CI's Linux packages.
- **Lesson:** Libraries opened with `dlopen` don't show up at link time, so only running the app finds them missing. An advisory step that fails still needs reading: a green run with a red annotation is not a clean run.

## P-045: The Linux release build failed on Ubuntu 22.04: `cannot find function spa_meta_region_is_valid`
- **Phase:** 4b (first release, tag v0.1.0)
- **Symptom:** The release workflow's `linux` job failed while compiling `libspa 0.10.1`: missing functions (`spa_meta_region_is_valid`, `spa_meta_first`), a missing `flags` field on `spa_video_info_raw`, and `i64`/`u64` mismatches. CI on `ubuntu-latest` (24.04) built the same code fine.
- **Root cause:** The release job ran on Ubuntu 22.04 so the AppImage would work on older systems. The screen-capture library (`xcap`) reaches PipeWire through `libspa`, whose bindings are generated from the system's headers. 22.04 ships PipeWire 0.3.48, which predates those functions and fields. Newer headers alone wouldn't help either: the AppImage leaves out `libpipewire` on purpose (it has to match the system's sound server), so on 22.04 it would load the old library at run time anyway.
- **Fix:** The Linux release builds on Ubuntu 24.04, and the install guide now says glibc 2.39+ (Ubuntu 24.04+, Fedora 40+, Debian 13+).
- **Lesson:** "Build on the oldest system you support" also fixes the oldest version of every system library you bind to, not only glibc. The CI matrix and the release job ran on different Ubuntu versions, so CI being green didn't cover the release. Release builds should be tried before tagging (`workflow_dispatch`) rather than discovered on the tag.

## P-046: The macOS release failed at the last step: `hdiutil: create failed - No space left on device`
- **Phase:** 4b (first release, tag v0.1.0)
- **Symptom:** On the release workflow's `macos` job, both architectures built, `lipo` joined them, the app was signed and verified, and then the `.dmg` step failed with "No space left on device".
- **Root cause:** The disk wasn't full. `hdiutil create -srcfolder` estimates the image size from the folder on its own, and on GitHub's macOS runners that estimate is sometimes too small for the contents. This is a known, intermittent problem. The app is about 3× larger than a single-architecture build because it holds both Apple Silicon and Intel code, which makes a tight estimate more likely to miss.
- **Fix:** `macos.sh` measures the staged folder with `du`, passes `-size` (folder plus 64 MB) and `-fs HFS+` itself, and retries up to three times. A retry also covers hdiutil's other intermittent error, "Resource busy".
- **Lesson:** A packaging tool on CI can fail for reasons unrelated to the code. Read the last line before the error rather than assuming the build broke. Once the cause is known, make the step state its inputs explicitly instead of relying on the tool's guesses.
