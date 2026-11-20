# Lectio: install and run

Lectio is a lecture note-taking app: record a class, take notes stamped with the moment in the recording, add slides,
photos, sketches and formulas, transcribe the audio on your own computer, and study from it. It runs on macOS, Windows,
Linux and in a web browser, from one Rust codebase. Your notes stay on your device; there is no account.

Every release on the project's GitHub **Releases** page has these files (`<v>` is the version, e.g. `0.4.0`):

| Platform | File | Notes |
|---|---|---|
| macOS 11 or newer (Apple Silicon and Intel) | `Lectio-<v>-macos-universal.dmg` | one app for both kinds of Mac |
| Windows 10 or 11 (64-bit) | `Lectio-<v>-windows-x64.msi` (installer) or `…-windows-x64.zip` (no install) | |
| Linux (x86-64, glibc 2.39 or newer: Ubuntu 24.04+, Fedora 40+, Debian 13+) | `Lectio-<v>-linux-x86_64.AppImage` | one file, no install |
| Web | open **https://mehmetefeozdemir.github.io/lectio/**, or `Lectio-<v>-web.zip` to host it yourself | nothing to install |
| — | `SHA256SUMS.txt` | checksums of all of the above |

To check a download: `shasum -a 256 <file>` (macOS, Linux) or `Get-FileHash <file>` (Windows PowerShell), and compare
with the line in `SHA256SUMS.txt`.

## macOS
1. Open the `.dmg` and drag **Lectio** onto **Applications**.
2. Open Lectio from Applications. If the release was signed and notarised, it opens straight away. If it wasn't (the
   release notes say), macOS says it "can't check it for malicious software": right-click (or Control-click) Lectio in
   Applications, choose **Open**, then **Open** again. You only do this once. On macOS 15 and newer, if there is no
   Open button: System Settings › Privacy & Security › scroll down › **Open Anyway**.
3. The first time you record, macOS asks for the microphone; recording computer sound asks for "Screen & System Audio
   Recording". Both can be changed later in System Settings › Privacy & Security.

Uninstall: drag Lectio from Applications to the Bin. Your notes are in `~/Library/Application Support/org.Lectio.Lectio`
(delete that folder to remove them too).

## Windows
- **Installer (.msi):** double-click it and follow the steps. Lectio appears in the Start menu. Uninstall it from
  Settings › Apps.
- **Zip:** unzip it anywhere and run `Lectio.exe`. Nothing is installed; delete the folder to remove it.

If the release wasn't signed, Windows SmartScreen shows "Windows protected your PC": choose **More info**, then
**Run anyway**. Windows asks for the microphone the first time you record (Settings › Privacy & security › Microphone).
Your notes are in `%APPDATA%\Lectio\Lectio\data`.

## Linux
```sh
chmod +x Lectio-<v>-linux-x86_64.AppImage
./Lectio-<v>-linux-x86_64.AppImage
```
If it says FUSE is missing, install `libfuse2` (Ubuntu 24.04: `sudo apt install libfuse2t64`; older: `libfuse2`), or run
it with `--appimage-extract-and-run`. Sound comes from PulseAudio or PipeWire, which desktop Linux already has. Your notes
are in `~/.local/share/lectio`.

## Web

Open https://mehmetefeozdemir.github.io/lectio/ in Chrome, Edge, Firefox or Safari 16.4+. The site is updated with each release.

To host your own copy, unzip the web zip as below.
Unzip `Lectio-<v>-web.zip` into a folder on any static web host (GitHub Pages, Netlify, a university web space, or
`python3 -m http.server` for a quick local try). Two rules:
- Serve it over **https** (or from `localhost`), or the browser won't allow the microphone.
- `.wasm` files must be sent as `application/wasm` (most hosts already do).

Notes are stored in that browser, for that web address only (IndexedDB). Clearing the site's data deletes them, so use
**Export** in the paper view to keep copies. Chrome, Edge and Firefox work; Safari 16.4 or newer.

## Build from source
Needs Rust (stable, via rustup). From the `` folder:
```sh
cargo run --release                     # the desktop app
cargo test --workspace                  # the tests
rustup target add wasm32-unknown-unknown && cargo install trunk
trunk build --release                   # the web app, into dist/
tools/package/macos.sh 0.4.0            # a .dmg (macOS); tools/package/ has the others
```
On Linux, first install the packages listed in `.github/workflows/ci.yml` (ALSA, PulseAudio, X11/Wayland, PipeWire
headers).

## Screen readers and keyboard
- Desktop: VoiceOver (macOS), Narrator and NVDA (Windows) and Orca (Linux) read Lectio's controls through the system
  accessibility interface.
- Browser: Lectio keeps a hidden copy of its controls in the page for screen readers. Move through it with your screen
  reader's normal reading keys (VoiceOver: VO + arrows); VO + Space (or Enter) presses the control. Tab and Shift-Tab move
  between controls, and the screen reader announces each one.
- 200% zoom: Ctrl/⌘ + plus on the desktop, or the browser's zoom; the layout switches to tabs when space runs out.
- Drawing on slides needs a mouse, pen or touch; everything else works from the keyboard.
