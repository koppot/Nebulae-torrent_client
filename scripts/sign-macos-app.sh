#!/usr/bin/env bash
# scripts/sign-macos-app.sh APP
#
# Seals a Nebulae.app that "tauri build" left unsealed, with an ad-hoc signature,
# from the inside out:
#
#   1. Every nested Mach-O image (VLC plugins and libraries, Transmission,
#      Tailscale, the JRE) must already carry a valid signature of its own. The
#      staging scripts sign what they rewrite, so this is a check, not a repair:
#      an image that fails was changed after it was signed, and signing it again
#      here would hide that. scripts/check-macos-signatures.py lists the images.
#   2. codesign --force --deep --sign - signs the executables in Contents/MacOS
#      and then the bundle, which seals the hash of every file under
#      Contents/Resources. Nothing in Contents/Resources is rewritten.
#   3. codesign --verify --deep --strict must pass.
#
# Without a signing identity, tauri build leaves the executable with only the
# linker's ad-hoc signature and the bundle unsealed ("Sealed Resources=none"), and
# codesign --verify --deep --strict then fails. Signing the executable changes its
# SHA-256: measure the executable after this script, not before.
#
# Only the ad-hoc signature that local candidates have always used is implemented.
# Developer ID signing and notarization are Tauri's job (APPLE_SIGNING_IDENTITY).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="${1:-}"
fail() { echo "ERROR: $*" >&2; exit 1; }
[ -n "$APP" ] || fail "usage: sign-macos-app.sh /path/to/Nebulae.app"
[ -d "$APP/Contents/MacOS" ] || fail "$APP is not an application bundle"
APP="$(cd "$APP" && pwd)"

echo "=== Sealing $APP (ad-hoc) ==="

echo "--- 1. Nested images are signed before the bundle"
python3 -B "$SCRIPT_DIR/check-macos-signatures.py" "$APP" \
    || fail "a nested image has no valid signature; find out why before sealing the bundle"

echo "--- 2. Executables and bundle"
codesign --force --deep --sign - "$APP"

echo "--- 3. Verification"
codesign --verify --deep --strict --verbose=2 "$APP"
EXE_NAME="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP/Contents/Info.plist")"
echo "OK: sealed. $EXE_NAME SHA-256 $(shasum -a 256 "$APP/Contents/MacOS/$EXE_NAME" | awk '{print $1}')"
