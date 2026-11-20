#!/usr/bin/env bash
# Builds the web version as a zip (ADR-028): the files trunk writes to dist/,
# ready to put on any static web host (it must serve .wasm files as
# application/wasm, and over https so the microphone can be used).
#
#   tools/package/web.sh VERSION
#
# Run from app/. Needs trunk and the wasm32-unknown-unknown target.
set -euo pipefail

VERSION="${1:?usage: web.sh VERSION}"
OUT=dist-release
mkdir -p "$OUT"
trunk build --release
ZIP="$PWD/$OUT/Lectio-$VERSION-web.zip"
rm -f "$ZIP"
(cd dist && zip -qr "$ZIP" . -x '.trunk-lock')
echo "$ZIP"
