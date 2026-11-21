#!/usr/bin/env bash
# Store screenshots for both stores (ADR-039), made by the UI tests on the
# sample lecture, light mode, with a clean status bar:
#
#   App Store  → mobile/ios/fastlane/screenshots/en-US/
#                6.9" iPhone (1320 × 2868) and 13" iPad (2064 × 2752)
#   Google Play→ mobile/android/fastlane/metadata/android/en-US/images/
#                phoneScreenshots (1080 × 1920), sevenInchScreenshots
#                (1200 × 1920), tenInchScreenshots (1600 × 2560)
#
# Usage: tools/store/screenshots.sh [ios|android|all]   (default: all)
#
# iOS needs the Simulators named below (Xcode › Settings › Components); set
# LECTIO_IOS_PHONE / LECTIO_IOS_TABLET to use others of the same screen size.
# Android uses a running emulator or device if there is one, otherwise starts
# the Android Studio virtual device named in LECTIO_AVD (or the first one).
set -euo pipefail

APP_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$APP_DIR"

IOS_PHONE="${LECTIO_IOS_PHONE:-iPhone 18 Pro Max}"
IOS_TABLET="${LECTIO_IOS_TABLET:-iPad Pro 13-inch (M5)}"
IOS_OUT="mobile/ios/fastlane/screenshots/en-US"
PLAY_OUT="mobile/android/fastlane/metadata/android/en-US/images"
WORK="target/store"

step() { printf '\n==> %s\n' "$*"; }
fail() { printf 'screenshots: %s\n' "$*" >&2; exit 1; }

# ---------- iOS ----------

simulator_id() {
    xcrun simctl list devices available --json | python3 -c '
import json, sys
name = sys.argv[1]
devices = json.load(sys.stdin)["devices"]
matches = [d["udid"] for runtime, list_ in sorted(devices.items()) if "iOS" in runtime for d in list_ if d["name"] == name]
print(matches[-1] if matches else "")' "$1"
}

ios_device() {
    local kind="$1" name="$2" variable="$3" udid result
    udid="$(simulator_id "$name")"
    [ -n "$udid" ] || fail "no '$name' Simulator; install it in Xcode, or set $variable."
    step "iOS $kind: $name"
    xcrun simctl boot "$udid" 2>/dev/null || true
    xcrun simctl bootstatus "$udid" -b > /dev/null
    xcrun simctl ui "$udid" appearance light
    xcrun simctl status_bar "$udid" override --time "9:41" --dataNetwork wifi --wifiMode active --wifiBars 3 \
        --cellularMode active --cellularBars 4 --batteryState charged --batteryLevel 100
    result="$WORK/ios-$kind-$(date +%s).xcresult"
    TEST_RUNNER_LECTIO_STORE_SCREENSHOTS=1 xcodebuild test \
        -project mobile/ios/Lectio.xcodeproj -scheme Lectio -destination "id=$udid" \
        -derivedDataPath mobile/ios/build/DerivedData -resultBundlePath "$result" \
        -only-testing:LectioUITests/StoreScreenshotUITests > "$WORK/ios-$kind.log" 2>&1 \
        || { xcrun simctl status_bar "$udid" clear; fail "the iOS screenshot test failed; see $WORK/ios-$kind.log"; }
    xcrun simctl status_bar "$udid" clear
    local attachments="$result-attachments"
    mkdir -p "$attachments"
    xcrun xcresulttool export attachments --path "$result" --output-path "$attachments" > /dev/null
    # The manifest maps each exported file to the name the test gave it.
    python3 - "$attachments" "$IOS_OUT" "$kind" <<'EOF'
import json, shutil, sys, pathlib
folder, out, kind = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
out.mkdir(parents=True, exist_ok=True)
for test in json.loads((folder / "manifest.json").read_text()):
    for item in test["attachments"]:
        name = item["suggestedHumanReadableName"].split("_0_")[0]
        shutil.copy(folder / item["exportedFileName"], out / f"{kind}-{name}.png")
        print(f"  {kind}-{name}.png")
EOF
}

ios() {
    tools/mobile/build-ios.sh
    mkdir -p "$WORK"
    rm -f "$IOS_OUT"/*.png
    ios_device phone "$IOS_PHONE" LECTIO_IOS_PHONE
    ios_device tablet "$IOS_TABLET" LECTIO_IOS_TABLET
}

# ---------- Android ----------

ADB=""
STARTED_EMULATOR=""

android_sdk() {
    for candidate in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}" "$HOME/Library/Android/sdk" "$HOME/Android/Sdk"; do
        if [ -n "$candidate" ] && [ -d "$candidate" ]; then echo "$candidate"; return; fi
    done
    fail "the Android SDK wasn't found; set ANDROID_HOME."
}

ensure_device() {
    local sdk avd
    sdk="$(android_sdk)"
    ADB="$sdk/platform-tools/adb"
    if "$ADB" devices | grep -q "device$"; then return; fi
    avd="${LECTIO_AVD:-$("$sdk/emulator/emulator" -list-avds | head -n 1)}"
    [ -n "$avd" ] || fail "no emulator running and no virtual device to start (Android Studio › Device Manager)."
    step "Starting the emulator $avd"
    "$sdk/emulator/emulator" -avd "$avd" -no-window -no-audio -no-boot-anim -no-snapshot-save > "$WORK/emulator.log" 2>&1 &
    STARTED_EMULATOR=yes
    "$ADB" wait-for-device
    until [ "$("$ADB" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; do sleep 2; done
}

demo_mode() {
    "$ADB" shell settings put global sysui_demo_allowed 1
    local demo=(shell am broadcast -a com.android.systemui.demo -e command)
    "$ADB" "${demo[@]}" enter > /dev/null
    "$ADB" "${demo[@]}" clock -e hhmm 0941 > /dev/null
    "$ADB" "${demo[@]}" battery -e level 100 -e plugged false > /dev/null
    "$ADB" "${demo[@]}" network -e wifi show -e level 4 -e fully true > /dev/null
    # Recent emulators ignore "datatype none" and show "3G", so the mobile
    # icon is hidden instead, with the other system icons (VPN, alarm...).
    "$ADB" "${demo[@]}" network -e mobile hide > /dev/null
    "$ADB" "${demo[@]}" status -e volume hide -e bluetooth hide -e location hide -e alarm hide \
        -e sync hide -e tty hide -e eri hide -e mute hide -e speakerphone hide -e zen hide > /dev/null
    "$ADB" "${demo[@]}" notifications -e visible false > /dev/null
}

restore_device() {
    "$ADB" shell wm size reset || true
    "$ADB" shell wm density reset || true
    "$ADB" shell am broadcast -a com.android.systemui.demo -e command exit > /dev/null || true
    if [ -n "$STARTED_EMULATOR" ]; then "$ADB" emu kill > /dev/null || true; fi
}

android_form() {
    local form="$1" size="$2" density="$3" folder="$4"
    step "Android $form: $size at $density dpi"
    "$ADB" shell wm size "$size"
    "$ADB" shell wm density "$density"
    sleep 2
    (cd mobile/android && ./gradlew --quiet connectedDebugAndroidTest \
        -Pandroid.testInstrumentationRunnerArguments.class=org.lectio.android.StoreScreenshotTest \
        -Pandroid.testInstrumentationRunnerArguments.storeScreenshots=true \
        -Pandroid.testInstrumentationRunnerArguments.formFactor="$form") > "$WORK/android-$form.log" 2>&1 \
        || fail "the Android screenshot test failed; see $WORK/android-$form.log"
    mkdir -p "$PLAY_OUT/$folder"
    rm -f "$PLAY_OUT/$folder"/*.png
    find mobile/android/app/build/outputs/connected_android_test_additional_output -path "*/store/$form-*.png" \
        -exec cp {} "$PLAY_OUT/$folder/" \;
    ls "$PLAY_OUT/$folder"
}

android() {
    tools/mobile/build-android.sh
    mkdir -p "$WORK"
    ensure_device
    trap restore_device EXIT
    "$ADB" shell cmd uimode night no > /dev/null
    demo_mode
    # Play: phones up to 2:1, so 1080 × 1920 (9:16); tablets by smallest width.
    android_form phone 1080x1920 420 phoneScreenshots
    android_form tablet7 1200x1920 320 sevenInchScreenshots
    android_form tablet10 1600x2560 320 tenInchScreenshots
}

case "${1:-all}" in
    ios) ios ;;
    android) android ;;
    all) ios; android ;;
    *) fail "unknown target '$1' (use: ios, android, all)" ;;
esac
step "Done."
