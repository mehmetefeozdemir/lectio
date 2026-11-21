# Lectio: install and run

Lectio is a lecture note-taking app: record a class, take notes stamped with the moment in the recording, add slides,
photos, sketches and formulas, transcribe the audio on your own computer, and study from it. It runs on macOS, Windows,
Linux and in a web browser from one Rust codebase, and as native apps over the same Rust core on Android phones and
tablets (an APK with each release) and on iPhone and iPad (built from source, see below). Your notes stay on your
device; there is no account.

Every release on the project's GitHub **Releases** page has these files (`<v>` is the version, e.g. `0.4.0`):

| Platform | File | Notes |
|---|---|---|
| macOS 11 or newer (Apple Silicon and Intel) | `Lectio-<v>-macos-universal.dmg` | one app for both kinds of Mac |
| Windows 10 or 11 (64-bit) | `Lectio-<v>-windows-x64.msi` (installer) or `…-windows-x64.zip` (no install) | |
| Linux (x86-64, glibc 2.39 or newer: Ubuntu 24.04+, Fedora 40+, Debian 13+) | `Lectio-<v>-linux-x86_64.AppImage` | one file, no install |
| Web | open **https://mehmetefeozdemir.github.io/lectio/**, or `Lectio-<v>-web.zip` to host it yourself | nothing to install |
| Android 8.0 or newer (phones and tablets) | `Lectio-<v>-android.apk` (or `…-android-debug-signed.apk`) | install by hand, see "Android" |
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

## Android
### Install the APK on your phone (sideload)
1. On the phone, open the release page in the browser and download `Lectio-<v>-android.apk` (or the
   `-debug-signed` one), or copy it over from a computer.
2. Open the downloaded file. Android asks whether the browser (or Files app) may install apps: **Settings** › allow
   **Install unknown apps** for it, then go back and tap **Install**.
3. Open Lectio. The first time you tap **Record**, Lectio explains why it needs the microphone, then Android asks; on
   Android 13 and newer it also asks for notifications, so a "Recording" notification (with a Stop button) can show
   while it records with the screen off.

From a computer with the Android platform tools: turn on **Developer options › USB debugging** on the phone, connect it,
and run `adb install Lectio-<v>-android.apk`.

**Updating:** a newer APK signed with the same key installs over the old one and keeps your notes. A
`-debug-signed` APK is signed by whichever machine built it, so if Android says the update "conflicts with an existing
package", export your lectures first (Export › PDF, Word or Markdown), then uninstall and install the new one;
uninstalling removes the notes stored on the phone.

### Run it in the Android Studio emulator on a Mac
**You need:** Android Studio (it brings the SDK, the emulator and a Java runtime), Rust (https://rustup.rs), and in
Android Studio › **Settings › Languages & Frameworks › Android SDK › SDK Tools**: **NDK (Side by side)** version
`30.0.16248370` (tick "Show Package Details" to pick it).
1. Build the Rust core for Android, from the `` folder:
   ```sh
   tools/mobile/build-android.sh
   ```
   The first run takes 10–20 minutes (three Android builds and one for the Mac); later runs only rebuild what changed.
2. In Android Studio, **File › Open** `mobile/android`. Let Gradle sync (it downloads the Android libraries and,
   the first time, the Android 37.1 platform).
3. **Tools › Device Manager › +**: create a virtual device, e.g. a **Medium Phone** with a recent system image. On an
   Apple Silicon Mac pick an **arm64** image. For the tablet layout, also make a tablet (e.g. **Medium Tablet**).
4. Pick the device at the top and press **Run ▶**. The emulator records from the Mac's microphone if you allow it in
   the emulator's settings (Extended controls › Microphone › "Virtual microphone uses host audio input").

Without Android Studio's buttons, from `mobile/android` (with `JAVA_HOME` set, e.g. to Android Studio's runtime
`/Applications/Android Studio.app/Contents/jbr/Contents/Home`):
```sh
./gradlew assembleDebug                 # the debug APK, in app/build/outputs/apk/debug/
./gradlew installDebug                  # install it on the running emulator or a connected phone
./gradlew testDebugUnitTest             # the JVM tests (they call the Rust core built for the Mac)
./gradlew connectedDebugAndroidTest     # the instrumented tests and the UI test, on the emulator
```
The UI test's screenshots are then in `build/outputs/connected_android_test_additional_output/`.

**Speed on a real phone:** record a lecture, open **Transcript**, choose a model and tap **Transcribe**; the line at
the end shows how long it took and the speed against real time. The first run downloads the model (tiny 151 MB,
base 290 MB).

## iPhone and iPad (build it yourself)
The iOS app is not on the App Store; you build it on a Mac and run it in the Simulator or on your own iPhone or iPad
(iOS 16 or newer). It keeps its notes in the same layout as the desktop app.

**You need:** a Mac with Xcode (16 or newer, from the App Store; open it once to install its components) and Rust
(https://rustup.rs). From the `` folder:
```sh
tools/mobile/build-ios.sh
```
This builds the shared Rust core for iPhone and the Simulator, writes the Swift bindings, downloads a checked copy of
XcodeGen into `target/tools/`, and generates `mobile/ios/Lectio.xcodeproj`. The first run takes 15–25 minutes (it
compiles the speech and study models' code three times); later runs only rebuild what changed. After editing only
`mobile/ios/project.yml`, `tools/mobile/build-ios.sh project` regenerates the project in seconds.

### In the Simulator
1. Open `mobile/ios/Lectio.xcodeproj` in Xcode.
2. At the top of the window choose the **Lectio** scheme and a Simulator, e.g. **iPhone 17** or an iPad.
3. Press **⌘R**. The Simulator records from your Mac's microphone (macOS asks for permission the first time).

Tests, from `mobile/ios`:
```sh
xcodebuild test -project Lectio.xcodeproj -scheme Lectio -destination 'platform=iOS Simulator,name=iPhone 17'
```
This runs the view-model tests and the UI test (course → lecture → record → note → play → export). The UI test's
screenshots are in the result bundle: `xcrun xcresulttool export attachments --path <bundle>.xcresult --output-path shots`.

### On your iPhone with a free Apple ID ("Personal Team")
No paid developer account is needed. No team id or certificate is stored in the repository; you choose your own team
in Xcode.
1. Xcode › **Settings** › **Accounts** › **+** › **Apple ID**, and sign in. Xcode creates "*Your Name* (Personal Team)".
2. Connect the iPhone with a cable, unlock it, and tap **Trust** on the phone.
3. Turn on Developer Mode on the phone: **Settings › Privacy & Security › Developer Mode** › on, then restart the phone
   when asked. (The switch appears after the phone has been connected to Xcode once.)
4. In Xcode, click the **Lectio** project › target **Lectio** › **Signing & Capabilities**: keep **Automatically manage
   signing** on and pick your **Personal Team**.
5. If Xcode says the bundle identifier `org.lectio.Lectio` "is not available", someone else's team already uses it:
   change it there to something unique, e.g. `org.lectio.Lectio.yourname`. This change lives in the generated project
   only, so it isn't committed (run step 4 again after regenerating the project).
6. Choose your iPhone at the top of the window and press **⌘R**.
7. The first time, the phone refuses to open an app from an "Untrusted Developer": **Settings › General › VPN & Device
   Management** › your Apple ID › **Trust**. Then open Lectio.

With a free team the app stops opening after **7 days**; run it from Xcode again to renew it (your notes stay). A free
team can have up to 3 such apps on a device at once.

**Speed on a real device:** record a lecture, open **Transcript**, choose a model and tap **Transcribe**; when it is done
the screen shows how long it took and the speed against real time. The first run downloads the model (tiny 151 MB,
base 290 MB) over Wi-Fi.

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
- iPhone and iPad: VoiceOver reads every control (a note's time button says "Play from 1 minute, 5 seconds"; formulas
  are read as their LaTeX), text follows the Larger Text setting, and the app follows dark mode.
- Android: TalkBack reads every control the same way (long-press menus are offered as "More actions"), text follows
  the system font size up to 200 %, and the app follows dark theme and your wallpaper colours.
