#!/usr/bin/env bash
# scripts/verify-delivered-package.sh [DMG_PATH] [UPDATER_TAR_PATH]
#
# Post-delivery verification gate enforcing:
# 1. Mounts candidate DMG read-only.
# 2. Verifies Nebulae.app/Contents/_CodeSignature/CodeResources exists.
# 3. Runs codesign --verify --deep --strict on Nebulae.app.
# 4. Verifies nested helper signatures (transmission-daemon, tailscaled).
# 5. Verifies mounted volume has .VolumeIcon.icns and kHasCustomIcon bit (GetFileInfo -aC).
# 6. Verifies outer DMG file has com.apple.FinderInfo, com.apple.ResourceFork, and kHasCustomIcon bit.
# 7. Verifies detached updater signature on updater/Nebulae.app.tar.gz via verify-payload-signature.
# 8. Fails delivery task immediately (exit 1) if any check fails.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

DMG_PATH="${1:-/Users/dev/.download/Nebulae-0.2.5-macOS-arm64.dmg}"
UPDATER_PATH="${2:-/Users/dev/.download/updater/Nebulae.app.tar.gz}"
UPDATER_SIG_PATH="${UPDATER_PATH}.sig"

fail() {
    printf "\n❌ GATE VERIFICATION FAILED: %s\n" "$*" >&2
    exit 1
}

pass() {
    printf "✅ %s\n" "$*"
}

log() {
    printf "=== %s ===\n" "$*"
}

log "Running Post-Delivery Verification Gate on Candidate Packages"

# -----------------------------------------------------------------------------
# Check 1: Outer DMG File Existence and Finder Custom Icon Attributes
# -----------------------------------------------------------------------------
log "1. Verifying Outer DMG File Attributes & Branding"
[[ -f "$DMG_PATH" ]] || fail "DMG file not found at $DMG_PATH"
[[ -s "$DMG_PATH" ]] || fail "DMG file is empty (0 bytes): $DMG_PATH"

pass "DMG file exists ($(stat -f '%z' "$DMG_PATH") bytes): $DMG_PATH"

# Outer DMG Finder custom icon bit (kHasCustomIcon = 0x0400)
DMG_ICON_BIT="$(GetFileInfo -aC "$DMG_PATH" 2>/dev/null || echo "0")"
if [[ "$DMG_ICON_BIT" != "1" ]]; then
    fail "Outer DMG does not have kHasCustomIcon bit set (GetFileInfo -aC returned: $DMG_ICON_BIT)"
fi
pass "Outer DMG kHasCustomIcon bit verified (1)"

# Outer DMG extended attributes (FinderInfo and ResourceFork)
XATTRS="$(xattr "$DMG_PATH" 2>/dev/null || true)"
if ! echo "$XATTRS" | grep -q "com.apple.FinderInfo"; then
    fail "Outer DMG missing com.apple.FinderInfo extended attribute"
fi
if ! echo "$XATTRS" | grep -q "com.apple.ResourceFork"; then
    fail "Outer DMG missing com.apple.ResourceFork extended attribute"
fi
pass "Outer DMG com.apple.FinderInfo and com.apple.ResourceFork attributes verified"

# -----------------------------------------------------------------------------
# Check 2: Mount DMG Read-Only and Audit Volume Attributes
# -----------------------------------------------------------------------------
log "2. Mounting Candidate DMG Read-Only"
MNT_DIR="$(mktemp -d /tmp/nebulae-verify-dmg.XXXXXX)"
DETACH_NEEDED=0

cleanup() {
    if [[ "$DETACH_NEEDED" -eq 1 && -d "$MNT_DIR" ]]; then
        hdiutil detach "$MNT_DIR" -force >/dev/null 2>&1 || diskutil eject "$MNT_DIR" >/dev/null 2>&1 || true
    fi
    rm -rf "$MNT_DIR"
}
trap cleanup EXIT

hdiutil attach "$DMG_PATH" -nobrowse -readonly -mountpoint "$MNT_DIR" >/dev/null 2>&1 || \
    diskutil image attach "$DMG_PATH" --mountOptions nobrowse --readOnly --mountPoint "$MNT_DIR" >/dev/null 2>&1 || \
    fail "Failed to mount DMG read-only at $MNT_DIR"
DETACH_NEEDED=1
pass "DMG mounted read-only at $MNT_DIR"

log "3. Verifying Mounted Volume Custom Icon Metadata"
[[ -f "$MNT_DIR/.VolumeIcon.icns" ]] || fail "Mounted volume missing .VolumeIcon.icns"
[[ -s "$MNT_DIR/.VolumeIcon.icns" ]] || fail "Mounted volume .VolumeIcon.icns is empty"
pass "Volume embedded .VolumeIcon.icns verified ($(stat -f '%z' "$MNT_DIR/.VolumeIcon.icns") bytes)"

VOL_ICON_BIT="$(GetFileInfo -aC "$MNT_DIR" 2>/dev/null || echo "0")"
if [[ "$VOL_ICON_BIT" != "1" ]]; then
    fail "Mounted volume root does not have kHasCustomIcon bit set (returned: $VOL_ICON_BIT)"
fi
pass "Mounted volume kHasCustomIcon bit verified (1)"

# -----------------------------------------------------------------------------
# Check 3: Audit Payload Integrity and Code Signatures
# -----------------------------------------------------------------------------
log "4. Auditing Nebulae.app Payload Integrity & Signatures"
APP_DIR="$MNT_DIR/Nebulae.app"
[[ -d "$APP_DIR" ]] || fail "Mounted DMG does not contain Nebulae.app"

# Verify _CodeSignature/CodeResources
CODE_RESOURCES="$APP_DIR/Contents/_CodeSignature/CodeResources"
[[ -f "$CODE_RESOURCES" ]] || fail "CRITICAL OMISSION: $APP_DIR is missing Contents/_CodeSignature/CodeResources"
[[ -s "$CODE_RESOURCES" ]] || fail "Contents/_CodeSignature/CodeResources is empty (0 bytes)"
pass "CodeResources seal resource verified ($(stat -f '%z' "$CODE_RESOURCES") bytes)"

# Verify entire app bundle signature deep and strict
codesign --verify --deep --strict --verbose=2 "$APP_DIR" || fail "codesign --verify --deep --strict failed on $APP_DIR"
pass "codesign deep strict verification passed for Nebulae.app"

# Verify nested transmission-daemon helper
TD_BIN="$APP_DIR/Contents/Resources/resources/engine/transmission/transmission-daemon"
[[ -f "$TD_BIN" ]] || fail "Missing nested transmission-daemon helper at $TD_BIN"
codesign --verify --verbose=2 "$TD_BIN" || fail "codesign verification failed on transmission-daemon"
pass "transmission-daemon nested code signature verified"

# Verify nested tailscaled helper
TS_BIN="$APP_DIR/Contents/Resources/resources/engine/tailscale/tailscaled"
[[ -f "$TS_BIN" ]] || fail "Missing nested tailscaled helper at $TS_BIN"
codesign --verify --verbose=2 "$TS_BIN" || fail "codesign verification failed on tailscaled"
pass "tailscaled nested code signature verified"

# Unmount cleanly
hdiutil detach "$MNT_DIR" -force >/dev/null 2>&1 || diskutil eject "$MNT_DIR" >/dev/null 2>&1 || true
DETACH_NEEDED=0
rm -rf "$MNT_DIR"
pass "DMG unmounted cleanly"

# -----------------------------------------------------------------------------
# Check 4: Verify Detached Updater Tarball and Minisign Signature
# -----------------------------------------------------------------------------
log "5. Verifying Detached Updater Tarball & Minisign Signature"
if [[ -f "$UPDATER_PATH" ]]; then
    [[ -s "$UPDATER_PATH" ]] || fail "Updater payload is empty: $UPDATER_PATH"
    [[ -f "$UPDATER_SIG_PATH" ]] || fail "Missing detached signature for updater: $UPDATER_SIG_PATH"

    (cd "$REPO_ROOT/src-tauri" && cargo run --quiet --features="ci-verify-payload-signature" --bin verify-payload-signature -- "$UPDATER_PATH" "$UPDATER_SIG_PATH") || \
        fail "Minisign cryptographic signature verification failed on $UPDATER_PATH"
    pass "Updater payload minisign signature cryptographically verified"
else
    fail "Updater payload not found at $UPDATER_PATH"
fi

printf "\n============================================================\n"
printf "🎉 ALL POST-DELIVERY VERIFICATION GATE CHECKS PASSED (100%%)!\n"
printf "============================================================\n"
