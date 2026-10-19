#!/usr/bin/env bash
# package-macos-dmg.sh — build the release DMG with BOTH icon targets set:
#
#   1. The MOUNTED VOLUME icon: embedded INSIDE the image
#      (.VolumeIcon.icns + kHasCustomIcon on the volume root). This survives
#      any download because it is part of the image payload.
#   2. The OUTER .dmg FILE icon: Finder metadata (resource fork /
#      com.apple.FinderInfo) set via NSWorkspace.setIcon.
#
# HONEST LIMITATION: the outer file icon is local filesystem metadata. Web
# upload/download and metadata-stripping transfers (some browsers, email
# gateways, zips created without resource forks) can REMOVE it. A recipient
# who downloads the raw .dmg may see a generic icon; macOS then regenerates
# an icon from the volume's embedded .VolumeIcon.icns once mounted. The
# DMG-in-ZIP artifact (ditto with --sequesterRsrc) is the metadata-preserving
# delivery option. Setting an icon on a CI runner guarantees nothing about
# the raw file after an arbitrary transfer.
#
# Usage:
#   package-macos-dmg.sh --app <Nebulae.app> --output <name.dmg> \
#                        [--icon <icon.icns>] [--layout-from <tauri.dmg>]
#
#   --app          Nebulae.app to package
#   --output       final DMG path (absolute or relative to cwd)
#   --icon         icon source; default: <repo>/src-tauri/icons/icon.icns
#                  resolved from THIS script's location, never the caller's cwd
#   --layout-from  a DMG produced by the tauri bundler whose .DS_Store
#                  (window size / icon positions from tauri.conf.json) should
#                  be carried over; optional

set -euo pipefail

log() { printf '%s\n' "$*"; }
fail() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

APP_DIR=""
OUTPUT=""
ICON_FILE="$REPO_ROOT/src-tauri/icons/icon.icns"
LAYOUT_DMG=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --app) APP_DIR="$2"; shift 2 ;;
        --output) OUTPUT="$2"; shift 2 ;;
        --icon) ICON_FILE="$2"; shift 2 ;;
        --layout-from) LAYOUT_DMG="$2"; shift 2 ;;
        *) fail "Unknown argument: $1" ;;
    esac
done

[[ -n "$APP_DIR" && -d "$APP_DIR" ]] || fail "App bundle not found: $APP_DIR"
[[ -n "$OUTPUT" ]] || fail "Missing --output"

# --- Icon resolution and validation (never relative to the caller's cwd) ----
if [[ ! -f "$ICON_FILE" || ! -r "$ICON_FILE" ]]; then
    fail "Icon file missing or unreadable: $ICON_FILE"
fi
ICON_FILE="$(cd "$(dirname "$ICON_FILE")" && printf '%s/%s\n' "$PWD" "$(basename "$ICON_FILE")")"
# ICNS magic: 'icns' (decoding is proven again by the JXA helper below).
[[ "$(head -c 4 "$ICON_FILE" | od -An -c | tr -d ' \n')" == "icns" ]] ||
    fail "Not a valid ICNS file: $ICON_FILE"

command -v SetFile >/dev/null || fail "SetFile not available (needs macOS)"
command -v osascript >/dev/null || fail "osascript not available (macOS only)"

# --- Temporary state with cleanup traps --------------------------------------
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/nebulae-dmg.XXXXXX")"
STAGING_DIR="$WORK_DIR/staging"
mkdir -p "$STAGING_DIR"
RW_DMG="$WORK_DIR/nebulae-rw.dmg"
MNT_CLEANED=0
verify_mnt=""

cleanup() {
    # Always detach any mounted image, then remove the work dir only.
    if [[ -n "${verify_mnt:-}" && "${MNT_CLEANED:-0}" -eq 0 ]]; then
        hdiutil detach "$verify_mnt" -force >/dev/null 2>&1 || true
    fi
    if [[ -n "${rw_mnt:-}" && -d "${rw_mnt:-}" ]]; then
        hdiutil detach "$rw_mnt" -force >/dev/null 2>&1 || true
    fi
    rm -rf "$WORK_DIR"
}
trap cleanup EXIT

log "=== Packaging DMG: $(basename "$OUTPUT") ==="

# --- Stage the payload --------------------------------------------------------
cp -R "$APP_DIR" "$STAGING_DIR/Nebulae.app"
ln -s /Applications "$STAGING_DIR/Applications"

# Carry over the bundler's .DS_Store so the DMG window layout configured in
# tauri.conf.json (window size, app / Applications positions) survives.
if [[ -n "$LAYOUT_DMG" && -f "$LAYOUT_DMG" ]]; then
    LAYOUT_MNT="$WORK_DIR/layout"
    mkdir -p "$LAYOUT_MNT"
    if hdiutil attach "$LAYOUT_DMG" -mountpoint "$LAYOUT_MNT" -nobrowse -readonly -quiet; then
        if [[ -f "$LAYOUT_MNT/.DS_Store" ]]; then
            cp "$LAYOUT_MNT/.DS_Store" "$STAGING_DIR/.DS_Store"
            log "Carried DMG window layout from tauri bundler output."
        fi
        hdiutil detach "$LAYOUT_MNT" -quiet || true
    else
        log "WARNING: could not mount $LAYOUT_DMG for layout; using default layout."
    fi
fi

# --- Mounted-volume icon (inside the image) ----------------------------------
cp "$ICON_FILE" "$STAGING_DIR/.VolumeIcon.icns"
SetFile -c icnC "$STAGING_DIR/.VolumeIcon.icns" ||
    fail "SetFile -c icnC failed for .VolumeIcon.icns"

# HFS+ is required for the custom-icon flag: hdiutil defaults to APFS on
# recent runners, where Finder flags do not persist.
rm -f "$RW_DMG"
hdiutil create -volname "Nebulae" -srcfolder "$STAGING_DIR" -fs HFS+ \
    -ov -format UDRW "$RW_DMG" ||
    fail "hdiutil create failed"

rw_mnt="$WORK_DIR/rw-volume"
mkdir -p "$rw_mnt"
hdiutil attach "$RW_DMG" -mountpoint "$rw_mnt" -nobrowse -noautoopen ||
    fail "Could not mount the read-write image"
SetFile -a C "$rw_mnt" ||
    fail "SetFile -a C failed on the mounted volume"
hdiutil detach "$rw_mnt" || fail "Could not detach the read-write image"
rw_mnt=""

# --- Convert to the final compressed image ------------------------------------
hdiutil convert "$RW_DMG" -format UDZO -o "$OUTPUT" -ov ||
    fail "hdiutil convert failed"

# --- Verify the FINAL image (read-only mount; always detached) -----------------
log "Verifying final DMG volume icon..."
verify_mnt="$WORK_DIR/verify-volume"
mkdir -p "$verify_mnt"
hdiutil attach "$OUTPUT" -mountpoint "$verify_mnt" -nobrowse -readonly ||
    fail "Could not mount the final DMG for verification"

[[ -f "$verify_mnt/.VolumeIcon.icns" ]] ||
    fail ".VolumeIcon.icns missing from the final DMG volume"
cmp -s "$ICON_FILE" "$verify_mnt/.VolumeIcon.icns" ||
    fail ".VolumeIcon.icns differs from $ICON_FILE"
# Verify the ACTUAL custom-icon flag (kHasCustomIcon, 0x0400 in the Finder
# flags at bytes 8-9 of the 32-byte FinderInfo), not just the existence of
# the xattr. (GetFileInfo -aa does not report flags on volume roots.)
FINDER_HEX="$(xattr -px com.apple.FinderInfo "$verify_mnt" | tr -d ' \n')" ||
    fail "com.apple.FinderInfo missing from the final DMG volume"
CUSTOM_FLAG=$((16#${FINDER_HEX:16:2}${FINDER_HEX:18:2}))
(( CUSTOM_FLAG & 0x0400 )) ||
    fail "Volume custom-icon flag (kHasCustomIcon) not set on the final DMG"
hdiutil detach "$verify_mnt" || fail "Could not detach after verification"
verify_mnt=""
MNT_CLEANED=1
log "DMG volume icon verified (embedded + flag set)."

# --- Outer .dmg FILE icon (Finder metadata; see limitation above) --------------
log "Setting custom icon on final DMG file..."
OUT="$(osascript -l JavaScript - "$ICON_FILE" "$OUTPUT" <<'JXA'
ObjC.import('AppKit');
ObjC.import('Foundation');

function run(argv) {
    const iconPath = argv[0];
    const targetPath = argv[1];
    const fm = $.NSFileManager.defaultManager;

    function fail(msg) { return 'ERROR: ' + msg; }

    const image = $.NSImage.alloc.initWithContentsOfFile(iconPath);
    if (image.isNil() || !image.isValid) {
        return fail('Cannot decode icon: ' + iconPath);
    }
    if (!fm.fileExistsAtPath(targetPath)) {
        return fail('Target does not exist: ' + targetPath);
    }
    if (!fm.isWritableFileAtPath(targetPath)) {
        return fail('Target is not writable: ' + targetPath);
    }
    const ok = $.NSWorkspace.sharedWorkspace
        .setIconForFileOptions(image, targetPath, 0);
    if (!ok) {
        return fail('setIcon returned false for: ' + targetPath);
    }
    return 'OK';
}
JXA
)" || fail "osascript failed"
[[ "$OUT" == "OK" ]] || fail "Icon helper failed: $OUT"
xattr -p com.apple.FinderInfo "$OUTPUT" >/dev/null 2>&1 ||
    fail "Outer DMG icon metadata (com.apple.FinderInfo) missing after setIcon"
log "Custom icon set on file: $OUTPUT"

log "DMG packaging done: $OUTPUT"
