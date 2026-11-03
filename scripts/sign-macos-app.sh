#!/usr/bin/env bash
# scripts/sign-macos-app.sh APP [IDENTITY]
#
# Seals a Nebulae.app from the inside out with a valid signing identity if
# available, or an ad-hoc signature with explicit designated requirements:
#
#   1. Nested helper executables (such as transmission-daemon) are signed
#      with their explicit identifier and designated requirement.
#   2. Nested libraries and plugins are verified.
#   3. The main executable in Contents/MacOS is signed.
#   4. The top-level bundle is sealed (without --deep, preserving nested code signatures).
#   5. codesign --verify --deep --strict verifies the entire bundle tree.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="${1:-}"
IDENTITY="${2:-${SIGNING_IDENTITY:-${APPLE_SIGNING_IDENTITY:-}}}"
fail() { echo "ERROR: $*" >&2; exit 1; }
[ -n "$APP" ] || fail "usage: sign-macos-app.sh /path/to/Nebulae.app [IDENTITY]"
[ -d "$APP/Contents/MacOS" ] || fail "$APP is not an application bundle"
APP="$(cd "$APP" && pwd)"

# If no identity was specified, look for an Apple Development identity in the keychain
if [ -z "$IDENTITY" ]; then
    DEV_IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null | grep -E "Apple Development:" | head -n 1 | awk '{print $2}' || true)"
    if [ -n "$DEV_IDENTITY" ]; then
        IDENTITY="$DEV_IDENTITY"
        echo "Found local Apple Development identity: $IDENTITY"
    else
        IDENTITY="-"
        echo "No Apple Development identity found; using ad-hoc signature (-)"
    fi
fi

echo "=== Sealing $APP (Identity: $IDENTITY) ==="

# 1. Sign helper executables if present
TD_BIN="$APP/Contents/Resources/resources/engine/transmission/transmission-daemon"
if [ -f "$TD_BIN" ]; then
    echo "--- 1. Signing transmission-daemon helper"
    if [ "$IDENTITY" != "-" ]; then
        codesign --force --sign "$IDENTITY" -i "org.asiandvdclub.nebulae.transmission-daemon" "$TD_BIN"
    else
        codesign --force --sign - -i "org.asiandvdclub.nebulae.transmission-daemon" -r='designated => identifier "org.asiandvdclub.nebulae.transmission-daemon"' "$TD_BIN"
    fi
fi

TS_BIN="$APP/Contents/Resources/resources/engine/tailscale/tailscaled"
if [ -f "$TS_BIN" ]; then
    echo "--- 1b. Signing tailscaled helper"
    if [ "$IDENTITY" != "-" ]; then
        codesign --force --sign "$IDENTITY" -i "org.asiandvdclub.nebulae.tailscaled" "$TS_BIN"
    else
        codesign --force --sign - -i "org.asiandvdclub.nebulae.tailscaled" -r='designated => identifier "org.asiandvdclub.nebulae.tailscaled"' "$TS_BIN"
    fi
fi

# 2. Check all nested images
echo "--- 2. Nested images check"
python3 -B "$SCRIPT_DIR/check-macos-signatures.py" "$APP" \
    || fail "a nested image has no valid signature; find out why before sealing the bundle"

# 3. Main executable and bundle
echo "--- 3. Main executable and bundle"
EXE_NAME="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP/Contents/Info.plist")"
BUNDLE_ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Contents/Info.plist")"
MAIN_EXE="$APP/Contents/MacOS/$EXE_NAME"

if [ "$IDENTITY" != "-" ]; then
    codesign --force --sign "$IDENTITY" -i "$BUNDLE_ID" "$MAIN_EXE"
    codesign --force --sign "$IDENTITY" "$APP"
else
    codesign --force --sign - -i "$BUNDLE_ID" -r="designated => identifier \"$BUNDLE_ID\"" "$MAIN_EXE"
    codesign --force --sign - "$APP"
fi

# 4. Verification
echo "--- 4. Verification"
codesign --verify --deep --strict --verbose=2 "$APP"
echo "OK: sealed. $EXE_NAME SHA-256 $(shasum -a 256 "$APP/Contents/MacOS/$EXE_NAME" | awk '{print $1}')"
