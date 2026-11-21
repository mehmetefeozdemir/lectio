#!/usr/bin/env bash
# The App Store build of the iOS app (ADR-037): builds the Rust core and the
# project (tools/mobile/build-ios.sh), archives the Release configuration,
# and exports it for App Store Connect with automatic signing.
#
#   LECTIO_TEAM_ID=ABCDE12345 tools/mobile/archive-ios.sh          → target/ios-release/export/Lectio.ipa
#   LECTIO_TEAM_ID=ABCDE12345 tools/mobile/archive-ios.sh upload   → also uploads it to App Store Connect
#
# The team id is your Apple Developer team (developer.apple.com › Membership);
# it is never stored in the repository. Signing uses the Apple account signed
# in to Xcode or, if these are set (as in CI), an App Store Connect API key:
#   APP_STORE_CONNECT_KEY_PATH (the .p8 file), APP_STORE_CONNECT_KEY_ID, APP_STORE_CONNECT_ISSUER_ID
set -euo pipefail

APP_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$APP_DIR"

fail() { printf 'archive-ios: %s\n' "$*" >&2; exit 1; }

TEAM="${LECTIO_TEAM_ID:-}"
[[ "$TEAM" =~ ^[A-Z0-9]{10}$ ]] || fail "set LECTIO_TEAM_ID to your 10-character Apple Developer team id."
MODE="${1:-export}"
[[ "$MODE" == "export" || "$MODE" == "upload" ]] || fail "unknown mode '$MODE' (use: export, upload)"

AUTH=()
if [ -n "${APP_STORE_CONNECT_KEY_PATH:-}" ]; then
    AUTH=(-authenticationKeyPath "$APP_STORE_CONNECT_KEY_PATH"
          -authenticationKeyID "${APP_STORE_CONNECT_KEY_ID:?}"
          -authenticationKeyIssuerID "${APP_STORE_CONNECT_ISSUER_ID:?}")
fi

tools/mobile/build-ios.sh

OUT="target/ios-release"
mkdir -p "$OUT"
ARCHIVE="$OUT/Lectio.xcarchive"
rm -rf "$ARCHIVE" "$OUT/export"

# ${AUTH[@]+...}: an empty array is "unbound" to bash 3.2 (macOS) under set -u.
xcodebuild archive \
    -project mobile/ios/Lectio.xcodeproj -scheme Lectio -configuration Release \
    -destination 'generic/platform=iOS' -archivePath "$ARCHIVE" \
    DEVELOPMENT_TEAM="$TEAM" -allowProvisioningUpdates ${AUTH[@]+"${AUTH[@]}"}

# The export options with this run's team (and destination) filled in.
OPTIONS="$OUT/ExportOptions.plist"
cp mobile/ios/ExportOptions.plist "$OPTIONS"
/usr/libexec/PlistBuddy -c "Add :teamID string $TEAM" "$OPTIONS"
if [ "$MODE" == "upload" ]; then
    /usr/libexec/PlistBuddy -c "Set :destination upload" "$OPTIONS"
fi

xcodebuild -exportArchive \
    -archivePath "$ARCHIVE" -exportOptionsPlist "$OPTIONS" -exportPath "$OUT/export" \
    -allowProvisioningUpdates ${AUTH[@]+"${AUTH[@]}"}

ls -la "$OUT/export"
