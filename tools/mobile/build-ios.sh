#!/usr/bin/env bash
# Builds the shared Rust core for the iOS app (ADR-029, ADR-030):
#
#   1. lectio-ffi as a static library for iPhone/iPad (arm64) and for the
#      Simulator (arm64 and x86_64, joined into one file with lipo);
#   2. the Swift bindings UniFFI writes from that library;
#   3. LectioCore.xcframework, which Xcode links;
#   4. the Xcode project, generated from mobile/ios/project.yml by a pinned
#      XcodeGen (downloaded and checked by SHA-256 if it isn't installed).
#
# Usage (from anywhere):  tools/mobile/build-ios.sh            everything
#                         tools/mobile/build-ios.sh project    only step 4
#
# Outputs (none of them are committed):
#   mobile/ios/Generated/LectioCore.xcframework
#   mobile/ios/Generated/lectio_ffi.swift
#   mobile/ios/Lectio.xcodeproj
set -euo pipefail

APP_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$APP_DIR"

IOS_MIN="16.0"
DEVICE_TARGET="aarch64-apple-ios"
SIM_TARGETS=("aarch64-apple-ios-sim" "x86_64-apple-ios")
LIB="liblectio_ffi.a"
WORK="target/ios"
GENERATED="mobile/ios/Generated"

XCODEGEN_VERSION="2.48.0"
XCODEGEN_SHA256="620c9f0dc10849f94728ab5df52d225ebab8187439dd6fee1925e59e80dff892"
XCODEGEN_URL="https://github.com/yonaskolb/XcodeGen/releases/download/${XCODEGEN_VERSION}/xcodegen.zip"
XCODEGEN_DIR="target/tools/xcodegen-${XCODEGEN_VERSION}"

step() { printf '\n==> %s\n' "$*"; }
fail() { printf 'build-ios: %s\n' "$*" >&2; exit 1; }

need() {
    command -v "$1" >/dev/null 2>&1 || fail "$1 is missing. $2"
}

check_tools() {
    need cargo "Install Rust from https://rustup.rs."
    need xcodebuild "Install Xcode and run: sudo xcode-select -s /Applications/Xcode.app"
    need lipo "Install the Xcode command line tools."
    if command -v rustup >/dev/null 2>&1; then
        rustup target add "$DEVICE_TARGET" "${SIM_TARGETS[@]}" >/dev/null
    fi
}

build_libraries() {
    # The deployment target goes to rustc and to the C code some crates
    # compile, so the linker doesn't warn about objects built for a newer iOS.
    export IPHONEOS_DEPLOYMENT_TARGET="$IOS_MIN"
    for target in "$DEVICE_TARGET" "${SIM_TARGETS[@]}"; do
        step "Building lectio-ffi for $target"
        cargo rustc -p lectio-ffi --lib --crate-type staticlib --release --locked --target "$target"
    done
    mkdir -p "$WORK/simulator"
    step "Joining the two Simulator slices"
    lipo -create \
        "target/${SIM_TARGETS[0]}/release/$LIB" \
        "target/${SIM_TARGETS[1]}/release/$LIB" \
        -output "$WORK/simulator/$LIB"
}

generate_bindings() {
    step "Writing the Swift bindings"
    rm -rf "$WORK/bindings"
    # The bindgen runs on this Mac; it reads the interface UniFFI recorded in
    # the device library, so the bindings match the code exactly.
    cargo run -p lectio-ffi --bin uniffi-bindgen --locked -- \
        generate --library "target/$DEVICE_TARGET/release/$LIB" \
        --language swift --out-dir "$WORK/bindings"
    mkdir -p "$WORK/headers"
    cp "$WORK/bindings/lectio_ffiFFI.h" "$WORK/headers/"
    # Xcode looks for a file named module.modulemap next to the header.
    cp "$WORK/bindings/lectio_ffiFFI.modulemap" "$WORK/headers/module.modulemap"
}

make_xcframework() {
    step "Making LectioCore.xcframework"
    mkdir -p "$GENERATED"
    rm -rf "$GENERATED/LectioCore.xcframework"
    xcodebuild -create-xcframework \
        -library "target/$DEVICE_TARGET/release/$LIB" -headers "$WORK/headers" \
        -library "$WORK/simulator/$LIB" -headers "$WORK/headers" \
        -output "$GENERATED/LectioCore.xcframework"
    cp "$WORK/bindings/lectio_ffi.swift" "$GENERATED/lectio_ffi.swift"
}

xcodegen_binary() {
    if command -v xcodegen >/dev/null 2>&1; then
        command -v xcodegen
        return
    fi
    local binary="$XCODEGEN_DIR/xcodegen/bin/xcodegen"
    if [ ! -x "$binary" ]; then
        # Downloaded into its own empty folder and checked before unpacking.
        rm -rf "$XCODEGEN_DIR"
        mkdir -p "$XCODEGEN_DIR"
        curl -sSfL -o "$XCODEGEN_DIR/xcodegen.zip" "$XCODEGEN_URL" >&2
        local sum
        sum="$(shasum -a 256 "$XCODEGEN_DIR/xcodegen.zip" | cut -d' ' -f1)"
        [ "$sum" = "$XCODEGEN_SHA256" ] || fail "XcodeGen download has SHA-256 $sum, expected $XCODEGEN_SHA256."
        unzip -q "$XCODEGEN_DIR/xcodegen.zip" -d "$XCODEGEN_DIR"
    fi
    echo "$APP_DIR/$binary"
}

generate_project() {
    step "Generating the Xcode project (XcodeGen $XCODEGEN_VERSION)"
    # The icon comes from assets/icon (drawn by src/media/icon.rs), so the
    # desktop and the phone never drift apart.
    cp assets/icon/lectio-icon-ios.png mobile/ios/Lectio/Assets.xcassets/AppIcon.appiconset/AppIcon.png
    # Version and build number from the one shared file (ADR-036).
    LECTIO_VERSION="$(sed -n 's/^version=//p' mobile/version.properties)"
    LECTIO_BUILD="$(sed -n 's/^build=//p' mobile/version.properties)"
    [[ "$LECTIO_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "mobile/version.properties: version must look like 1.0.0"
    [[ "$LECTIO_BUILD" =~ ^[1-9][0-9]*$ ]] || fail "mobile/version.properties: build must be a whole number above 0"
    export LECTIO_VERSION LECTIO_BUILD
    echo "Version $LECTIO_VERSION, build $LECTIO_BUILD"
    local xcodegen
    xcodegen="$(xcodegen_binary)"
    "$xcodegen" generate --spec mobile/ios/project.yml --project mobile/ios
}

case "${1:-all}" in
    all)
        check_tools
        build_libraries
        generate_bindings
        make_xcframework
        generate_project
        ;;
    project)
        generate_project
        ;;
    *)
        fail "unknown step '$1' (use: all, project)"
        ;;
esac

step "Done. Open mobile/ios/Lectio.xcodeproj in Xcode."
