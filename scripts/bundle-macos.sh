#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Builds dist/ScreenYield.app.
# SY_SIGN_IDENTITY / SY_SIGN_KEYCHAIN: certificate to sign with (default: ad-hoc).
# Usage: scripts/bundle-macos.sh   (SY_SKIP_BUILD=1 uses the program already in target/release)
set -euo pipefail

cd "$(dirname "$0")/.."
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
APP="dist/ScreenYield.app"

if [ -z "${SY_SKIP_BUILD:-}" ]; then
  cargo build --release
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/screenyield "$APP/Contents/MacOS/ScreenYield"

# The app icon is drawn by the program itself
ICONS=$(mktemp -d)
trap 'rm -rf "$ICONS"' EXIT
"$APP/Contents/MacOS/ScreenYield" --write-icons "$ICONS"
iconutil -c icns -o "$APP/Contents/Resources/ScreenYield.icns" "$ICONS/ScreenYield.iconset"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>ScreenYield</string>
  <key>CFBundleDisplayName</key><string>ScreenYield</string>
  <key>CFBundleIdentifier</key><string>de.eckonator.screenyield</string>
  <key>CFBundleExecutable</key><string>ScreenYield</string>
  <key>CFBundleIconFile</key><string>ScreenYield</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>NSHumanReadableCopyright</key><string>© 2026 Markus Eckert · GPL-3.0-or-later</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <!-- Menu bar app: no Dock icon, not in ⌘Tab -->
  <key>LSUIElement</key><true/>
</dict>
</plist>
PLIST

# Signed with the project's own certificate if there is one (see scripts/signing-keychain.sh), so
# the app accepts updates signed the same way; otherwise ad-hoc, so Apple silicon Macs start it at all
if [ -z "${SY_SIGN_IDENTITY:-}" ]; then
  echo "Warning: no SY_SIGN_IDENTITY, signing ad-hoc" >&2
fi
SIGN=(--force --sign "${SY_SIGN_IDENTITY:--}")
if [ -n "${SY_SIGN_KEYCHAIN:-}" ]; then
  SIGN+=(--keychain "$SY_SIGN_KEYCHAIN")
fi
codesign "${SIGN[@]}" --identifier de.eckonator.screenyield "$APP"
codesign --verify --deep --strict "$APP"
echo "Done: $APP"
