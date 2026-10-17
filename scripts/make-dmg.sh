#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Packs dist/ScreenYield.app into a disk image with a link to Applications (drag to install).
# Usage: scripts/make-dmg.sh [target.dmg]   (run scripts/bundle-macos.sh first)
set -euo pipefail

cd "$(dirname "$0")/.."
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
OUT="${1:-dist/ScreenYield-$VERSION-macOS.dmg}"
APP="dist/ScreenYield.app"
[ -d "$APP" ] || { echo "Missing: $APP (run scripts/bundle-macos.sh first)" >&2; exit 1; }

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
ditto "$APP" "$STAGE/ScreenYield.app"
ln -s /Applications "$STAGE/Applications"
cp README.md LICENSE "$STAGE/"
# Licenses of the libraries (written by the release workflow)
if [ -f dist/THIRD-PARTY-LICENSES.yml ]; then cp dist/THIRD-PARTY-LICENSES.yml "$STAGE/"; fi
rm -f "$OUT"
hdiutil create -quiet -volname "ScreenYield $VERSION" -srcfolder "$STAGE" -format UDZO "$OUT"
echo "Done: $OUT"
