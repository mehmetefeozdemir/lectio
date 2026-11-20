#!/usr/bin/env bash
# Builds Lectio.app and a .dmg for macOS (ADR-028).
#
#   tools/package/macos.sh VERSION [--universal]
#
# --universal builds for Apple Silicon and Intel and joins them with `lipo`
# (needs `rustup target add aarch64-apple-darwin x86_64-apple-darwin`).
# Signing: with MACOS_SIGNING_IDENTITY set (a "Developer ID Application"
# certificate in the keychain) the app is signed with the hardened runtime,
# and with APPLE_ID, APPLE_TEAM_ID and APPLE_APP_PASSWORD also set it is
# notarised. Otherwise it gets an ad-hoc signature, which Apple Silicon needs
# to run at all; the install guide explains how to open it the first time.
# Run from app/. Output: dist-release/Lectio-VERSION-macos-<arch>.dmg
set -euo pipefail

VERSION="${1:?usage: macos.sh VERSION [--universal]}"
UNIVERSAL="${2:-}"
OUT=dist-release
APP="$OUT/Lectio.app"
rm -rf "$APP" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

if [ "$UNIVERSAL" = "--universal" ]; then
  cargo build --release --locked --target aarch64-apple-darwin
  cargo build --release --locked --target x86_64-apple-darwin
  lipo -create -output "$APP/Contents/MacOS/lectio" \
    target/aarch64-apple-darwin/release/lectio target/x86_64-apple-darwin/release/lectio
  ARCH=universal
else
  cargo build --release --locked
  cp target/release/lectio "$APP/Contents/MacOS/lectio"
  ARCH=$(uname -m)
fi

# The icon: every size macOS asks for, from the 1024 px PNG.
ICONSET="$OUT/Lectio.iconset"
rm -rf "$ICONSET" && mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z $size $size assets/icon/lectio-icon.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z $double $double assets/icon/lectio-icon.png --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Lectio.icns"
rm -rf "$ICONSET"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Lectio</string>
  <key>CFBundleDisplayName</key><string>Lectio</string>
  <key>CFBundleIdentifier</key><string>org.lectio.Lectio</string>
  <key>CFBundleExecutable</key><string>lectio</string>
  <key>CFBundleIconFile</key><string>Lectio</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>Lectio records lectures so your notes are linked to the audio.</string>
  <key>NSCameraUsageDescription</key><string>Lectio can add photos of the board to your notes.</string>
  <key>NSAudioCaptureUsageDescription</key><string>Lectio can record your computer's sound for online classes.</string>
  <key>NSScreenCaptureUsageDescription</key><string>Lectio can add a screenshot of part of the screen to your notes.</string>
</dict>
</plist>
PLIST

if [ -n "${MACOS_SIGNING_IDENTITY:-}" ]; then
  codesign --force --options runtime --timestamp --sign "$MACOS_SIGNING_IDENTITY" "$APP"
else
  echo "No signing identity: ad-hoc signature (users open it with right-click › Open the first time)."
  codesign --force --sign - "$APP"
fi
codesign --verify --verbose "$APP"

DMG="$OUT/Lectio-$VERSION-macos-$ARCH.dmg"
STAGE="$OUT/dmg"
rm -rf "$STAGE" "$DMG" && mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
# hdiutil's own size estimate is sometimes too small on CI runners ("No space
# left on device"), so give it the folder's size plus room, and retry: it also
# fails now and then with "Resource busy".
SIZE_MB=$(( $(du -sm "$STAGE" | cut -f1) + 64 ))
for attempt in 1 2 3; do
  if hdiutil create -volname "Lectio $VERSION" -srcfolder "$STAGE" -size "${SIZE_MB}m" \
      -fs HFS+ -ov -format UDZO "$DMG" >/dev/null; then
    break
  fi
  [ "$attempt" = 3 ] && { echo "hdiutil failed 3 times" >&2; exit 1; }
  echo "hdiutil failed (attempt $attempt), retrying"; sleep 5
done
rm -rf "$STAGE"

if [ -n "${MACOS_SIGNING_IDENTITY:-}" ] && [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ] && [ -n "${APPLE_APP_PASSWORD:-}" ]; then
  codesign --sign "$MACOS_SIGNING_IDENTITY" --timestamp "$DMG"
  xcrun notarytool submit "$DMG" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" --wait
  xcrun stapler staple "$DMG"
fi
echo "$DMG"
