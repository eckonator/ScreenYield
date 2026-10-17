#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Puts the ScreenYield signing certificate into a keychain of its own and prints the lines to
# export for scripts/bundle-macos.sh. The certificate is self-signed; the app installs an update
# only if it is signed with the same certificate.
#
# Usage: eval "$(scripts/signing-keychain.sh CERT.p12 KEYCHAIN_PATH)"   (password in SY_SIGN_PASSWORD)
set -euo pipefail

P12="$1"
KEYCHAIN="$2"
PASSWORD="${SY_SIGN_PASSWORD:?SY_SIGN_PASSWORD is not set}"
# Only this script uses the keychain, its own password does not need to be secret
LOCK=screenyield

rm -f "$KEYCHAIN"
security create-keychain -p "$LOCK" "$KEYCHAIN"
security set-keychain-settings "$KEYCHAIN"
security unlock-keychain -p "$LOCK" "$KEYCHAIN"
security import "$P12" -k "$KEYCHAIN" -P "$PASSWORD" -T /usr/bin/codesign >/dev/null
security set-key-partition-list -S apple-tool:,apple: -s -k "$LOCK" "$KEYCHAIN" >/dev/null
# codesign looks for the private key in the search list; on the CI machine add the keychain there
# (on a developer's Mac the list stays as it is)
if [ -n "${CI:-}" ]; then
  # shellcheck disable=SC2046
  security list-keychains -d user -s "$KEYCHAIN" $(security list-keychains -d user | tr -d '"')
fi
IDENTITY=$(security find-identity -p codesigning "$KEYCHAIN" | awk '/^ *1\)/ {print $2; exit}')
if [ -z "$IDENTITY" ]; then
  echo "no signing identity in $P12" >&2
  exit 1
fi
echo "export SY_SIGN_IDENTITY=$IDENTITY"
echo "export SY_SIGN_KEYCHAIN=$KEYCHAIN"
