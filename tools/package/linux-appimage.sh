#!/usr/bin/env bash
# Builds Lectio-VERSION-linux-x86_64.AppImage (ADR-028): one file that runs
# on most desktop Linux systems. linuxdeploy copies the libraries the app
# links to into the image, except the ones every system has (glibc, X11,
# Wayland, ALSA, PulseAudio), which must come from the system so they match
# its drivers and sound server. Build on the oldest Ubuntu you support: an
# AppImage runs on systems with the same or a newer glibc.
#
#   tools/package/linux-appimage.sh VERSION
#
# Run from app/. Needs curl and the build packages from ci.yml.
set -euo pipefail

VERSION="${1:?usage: linux-appimage.sh VERSION}"
OUT=dist-release
APPDIR="$OUT/Lectio.AppDir"
TOOLS="$OUT/tools"
# Pinned release of linuxdeploy (the AppImage builder).
LINUXDEPLOY_URL="https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20250213-2/linuxdeploy-x86_64.AppImage"

cargo build --release --locked
rm -rf "$APPDIR" && mkdir -p "$APPDIR/usr/bin" "$TOOLS"
cp target/release/lectio "$APPDIR/usr/bin/lectio"

cat > "$OUT/lectio.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Lectio
Comment=Lecture notes timestamped to the audio
Exec=lectio
Icon=lectio
Categories=Education;Office;
Terminal=false
DESKTOP
cp assets/icon/lectio-icon.png "$OUT/lectio.png"

if [ ! -x "$TOOLS/linuxdeploy" ]; then
  curl -sSfL -o "$TOOLS/linuxdeploy" "$LINUXDEPLOY_URL"
  chmod +x "$TOOLS/linuxdeploy"
fi
# Runners have no FUSE: let the tool unpack itself instead of mounting.
export APPIMAGE_EXTRACT_AND_RUN=1
export LDAI_OUTPUT="$OUT/Lectio-$VERSION-linux-x86_64.AppImage"
"$TOOLS/linuxdeploy" --appdir "$APPDIR" \
  --executable "$APPDIR/usr/bin/lectio" \
  --desktop-file "$OUT/lectio.desktop" \
  --icon-file "$OUT/lectio.png" \
  --exclude-library 'libasound.so*' --exclude-library 'libpulse*.so*' --exclude-library 'libpipewire*.so*' \
  --exclude-library 'libwayland*.so*' --exclude-library 'libxkbcommon*.so*' --exclude-library 'libX*.so*' \
  --exclude-library 'libxcb*.so*' --exclude-library 'libEGL*.so*' --exclude-library 'libGL*.so*' \
  --exclude-library 'libvulkan*.so*' --exclude-library 'libgbm*.so*' --exclude-library 'libdrm*.so*' \
  --exclude-library 'libdbus*.so*' \
  --output appimage
echo "$LDAI_OUTPUT"
