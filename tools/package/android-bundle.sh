#!/usr/bin/env bash
# The Google Play upload (ADR-038): a release Android App Bundle (.aab),
# minified with R8 and signed with your upload key, written to
#   dist-release/Lectio-<version>-<build>.aab
# (version and build from mobile/version.properties).
#
# The upload key is never in the repository. Give it either through the
# environment (as CI does):
#   LECTIO_KEYSTORE_FILE, LECTIO_KEYSTORE_PASSWORD, LECTIO_KEY_ALIAS, LECTIO_KEY_PASSWORD
# or in mobile/android/keystore.properties (gitignored):
#   storeFile=/path/to/upload.jks   storePassword=…   keyAlias=…   keyPassword=…
# Play refuses bundles signed with a debug key, so without a key this stops.
#
# Usage: tools/package/android-bundle.sh
set -euo pipefail

APP_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$APP_DIR"

fail() { printf 'android-bundle: %s\n' "$*" >&2; exit 1; }

VERSION="$(sed -n 's/^version=//p' mobile/version.properties)"
BUILD="$(sed -n 's/^build=//p' mobile/version.properties)"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ && "$BUILD" =~ ^[1-9][0-9]*$ ]] || fail "mobile/version.properties needs version=1.2.3 and build=<number>."

if [ -z "${LECTIO_KEYSTORE_FILE:-}" ] && [ ! -f mobile/android/keystore.properties ]; then
    fail "no upload key: set LECTIO_KEYSTORE_FILE and friends, or create mobile/android/keystore.properties (docs/STORE.md, Play step 3)."
fi

tools/mobile/build-android.sh

# The 64-bit libraries must have 16 KB-aligned segments (Google Play requires
# it for devices with 16 KB memory pages, which are all 64-bit; 32-bit
# armeabi-v7a never runs with 16 KB pages, P-073).
SDK="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
READELF="$(ls "$SDK"/ndk/*/toolchains/llvm/prebuilt/*/bin/llvm-readelf 2>/dev/null | tail -n 1)"
[ -n "$READELF" ] || fail "llvm-readelf not found in the Android NDK."
for so in mobile/android/app/src/main/jniLibs/{arm64-v8a,x86_64}/liblectio_ffi.so; do
    if "$READELF" -lW "$so" | awk '$1 == "LOAD" { if ($NF != "0x4000" && $NF != "0x10000") bad = 1 } END { exit bad }'; then
        echo "16 KB aligned: $so"
    else
        fail "$so is not 16 KB aligned"
    fi
done

(cd mobile/android && ./gradlew --no-daemon bundleRelease)

AAB=mobile/android/app/build/outputs/bundle/release/app-release.aab
SIGNER="$(keytool -printcert -jarfile "$AAB" 2>/dev/null | sed -n 's/^Owner: //p' | head -n 1)"
[ -n "$SIGNER" ] || fail "the bundle isn't signed."
case "$SIGNER" in
    *"CN=Android Debug"*) fail "the bundle is signed with a debug key; check the upload key settings." ;;
esac

mkdir -p dist-release
OUT="dist-release/Lectio-$VERSION-$BUILD.aab"
cp "$AAB" "$OUT"
echo "Signed by: $SIGNER"
ls -lh "$OUT"
