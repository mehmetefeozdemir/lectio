#!/usr/bin/env bash
# Builds the shared Rust core for the Android app (ADR-033):
#
#   1. lectio-ffi as a shared library (liblectio_ffi.so) for arm64-v8a,
#      armeabi-v7a and x86_64 with the Android NDK (through a pinned
#      cargo-ndk), stripped, into the app's jniLibs/<abi>/;
#   2. the same crate for this computer, and the Kotlin bindings UniFFI writes
#      from it, into app/src/generated/kotlin (the JVM unit tests load this
#      computer's library through JNA, so they run the real Rust code);
#   3. the launcher icon layers, copied from assets/icon.
#
# Usage (from anywhere):  tools/mobile/build-android.sh               all three ABIs
#                         tools/mobile/build-android.sh x86_64        only the ABIs named
#
# Needs the Android SDK (ANDROID_HOME, or Android Studio's default folder) with
# NDK $NDK_VERSION:  sdkmanager "ndk;30.0.16248370"
set -euo pipefail

APP_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$APP_DIR"

MIN_SDK=26
NDK_VERSION="30.0.16248370"
CARGO_NDK_VERSION="4.1.2"
ALL_ABIS=(arm64-v8a armeabi-v7a x86_64)
ANDROID_DIR="mobile/android"
JNI_LIBS="$ANDROID_DIR/app/src/main/jniLibs"
GENERATED="$ANDROID_DIR/app/src/generated/kotlin"
RES="$ANDROID_DIR/app/src/main/res"
CARGO_NDK_ROOT="target/tools/cargo-ndk-$CARGO_NDK_VERSION"

step() { printf '\n==> %s\n' "$*"; }
fail() { printf 'build-android: %s\n' "$*" >&2; exit 1; }

triple_for() {
    case "$1" in
        arm64-v8a) echo aarch64-linux-android ;;
        armeabi-v7a) echo armv7-linux-androideabi ;;
        x86_64) echo x86_64-linux-android ;;
        *) fail "unknown ABI '$1' (use: ${ALL_ABIS[*]})" ;;
    esac
}

find_sdk() {
    for candidate in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}" "$HOME/Library/Android/sdk" "$HOME/Android/Sdk"; do
        if [ -n "$candidate" ] && [ -d "$candidate" ]; then
            echo "$candidate"
            return
        fi
    done
    fail "the Android SDK wasn't found. Install Android Studio, or set ANDROID_HOME."
}

# cargo-ndk only runs as a cargo subcommand (`cargo ndk`), so its folder goes
# first on the PATH.
use_cargo_ndk() {
    export PATH="$APP_DIR/$CARGO_NDK_ROOT/bin:$PATH"
    if ! cargo ndk --version 2>/dev/null | grep -q "$CARGO_NDK_VERSION"; then
        step "Installing cargo-ndk $CARGO_NDK_VERSION into $CARGO_NDK_ROOT"
        cargo install cargo-ndk --version "$CARGO_NDK_VERSION" --locked --root "$CARGO_NDK_ROOT"
    fi
}

build_android_libraries() {
    local sdk ndk strip
    sdk="$(find_sdk)"
    ndk="$sdk/ndk/$NDK_VERSION"
    [ -d "$ndk" ] || fail "NDK $NDK_VERSION is missing. Install it with: sdkmanager \"ndk;$NDK_VERSION\" (or Android Studio › SDK Manager › SDK Tools › NDK)."
    strip="$(ls "$ndk"/toolchains/llvm/prebuilt/*/bin/llvm-strip | head -n 1)"
    use_cargo_ndk
    if command -v rustup >/dev/null 2>&1; then
        for abi in "${ABIS[@]}"; do rustup target add "$(triple_for "$abi")" >/dev/null; done
    fi
    for abi in "${ABIS[@]}"; do
        local triple
        triple="$(triple_for "$abi")"
        step "Building lectio-ffi for $abi ($triple, API $MIN_SDK)"
        # `cargo rustc --crate-type cdylib` keeps lectio-ffi a plain library
        # for everything else in the workspace (ADR-029).
        ANDROID_NDK_HOME="$ndk" cargo ndk -t "$abi" --platform "$MIN_SDK" \
            rustc -p lectio-ffi --lib --crate-type cdylib --release --locked
        mkdir -p "$JNI_LIBS/$abi"
        # Symbols are only for debugging; stripping takes the library from
        # hundreds of MB to tens.
        "$strip" --strip-unneeded -o "$JNI_LIBS/$abi/liblectio_ffi.so" "target/$triple/release/liblectio_ffi.so"
        ls -lh "$JNI_LIBS/$abi/liblectio_ffi.so"
    done
}

host_library() {
    case "$(uname -s)" in
        Darwin) echo "target/debug/liblectio_ffi.dylib" ;;
        MINGW* | MSYS* | CYGWIN*) echo "target/debug/lectio_ffi.dll" ;;
        *) echo "target/debug/liblectio_ffi.so" ;;
    esac
}

generate_bindings() {
    step "Building lectio-ffi for this computer (JVM unit tests) and writing the Kotlin bindings"
    cargo rustc -p lectio-ffi --lib --crate-type cdylib --locked
    mkdir -p "$GENERATED"
    cargo run -p lectio-ffi --bin uniffi-bindgen --locked -- \
        generate --library "$(host_library)" --language kotlin --no-format --out-dir "$GENERATED"
}

copy_icons() {
    step "Copying the launcher icon layers from assets/icon"
    mkdir -p "$RES/drawable-nodpi"
    cp assets/icon/lectio-icon-android-foreground.png "$RES/drawable-nodpi/ic_launcher_foreground.png"
    cp assets/icon/lectio-icon-android-monochrome.png "$RES/drawable-nodpi/ic_launcher_monochrome.png"
}

if [ "$#" -gt 0 ]; then
    ABIS=("$@")
else
    ABIS=("${ALL_ABIS[@]}")
fi
for abi in "${ABIS[@]}"; do triple_for "$abi" >/dev/null; done

build_android_libraries
generate_bindings
copy_icons
step "Done. Open mobile/android in Android Studio, or run ./gradlew assembleDebug there."
