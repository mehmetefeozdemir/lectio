#!/usr/bin/env bash
# Builds the Android APK for a release (ADR-035):
#   dist-release/Lectio-<version>-android.apk               signed with the release key
#   dist-release/Lectio-<version>-android-debug-signed.apk  when no key is set
#
# The release key comes only from the environment (CI secrets), never from
# the repository:
#   LECTIO_KEYSTORE_FILE, LECTIO_KEYSTORE_PASSWORD, LECTIO_KEY_ALIAS, LECTIO_KEY_PASSWORD
# Without it the APK is signed with this machine's debug key: it installs by
# hand (sideloading), but a later build signed by another machine can't
# update it in place.
#
# Usage: tools/package/android.sh 0.5.0
set -euo pipefail

VERSION="${1:?usage: tools/package/android.sh <version>}"
APP_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$APP_DIR"

tools/mobile/build-android.sh

(cd mobile/android && ./gradlew --no-daemon assembleRelease "-Plectio.version=$VERSION")

mkdir -p dist-release
if [ -n "${LECTIO_KEYSTORE_FILE:-}" ] && [ -f "$LECTIO_KEYSTORE_FILE" ]; then
    NAME="Lectio-$VERSION-android.apk"
else
    NAME="Lectio-$VERSION-android-debug-signed.apk"
fi
cp mobile/android/app/build/outputs/apk/release/app-release.apk "dist-release/$NAME"
ls -lh "dist-release/$NAME"
