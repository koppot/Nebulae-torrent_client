#!/usr/bin/env bash
# set-dmg-icon.sh — assign a custom Finder icon to a .dmg file.
#
# Usage: set-dmg-icon.sh <dmg-path> [icon.icns]
#
#   <dmg-path>   the FINAL .dmg file to decorate (must exist, writable)
#   [icon.icns]  icon source; default: icon.icns beside THIS script
#
# Implementation: NSWorkspace.setIcon(forFile:) via a small Swift helper in a
# private temporary directory (no Homebrew, no Finder UI scripting, no admin
# rights). The helper is removed on success or failure.
#
# LIMITATION (macOS custom icons are stored in filesystem metadata):
#  - The MOUNTED VOLUME icon lives INSIDE the image (HFS+/APFS
#    .VolumeIcon.icns + custom-icon attribute) — tauri's bundler already
#    embeds it, and it survives downloads.
#  - The OUTER .dmg FILE icon lives in a resource fork / Finder metadata
#    (com.apple.FinderInfo xattr) attached to the file. Web upload/download
#    and metadata-stripping transfers (some browsers, email gateways, zip
#    detours) may REMOVE it. Merely shipping icon.icns next to a downloaded
#    DMG does NOT give that DMG an icon on the recipient's machine; the
#    recipient's Finder regenerates a generic icon from the volume once
#    mounted.
#
# NOTE ON SIGNING: applying the icon mutates the file, so it must happen
# BEFORE codesigning/notarization if/when those are enabled for the DMG.
# This project currently ships unsigned mac artifacts, so this script runs
# right after `tauri build` in release.yml.

set -euo pipefail

log() { printf '%s\n' "$*"; }
fail() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ $# -lt 1 ]]; then
    fail "Usage: $0 <dmg-path> [icon.icns]"
fi

DMG_PATH="$1"

# --- 1. Resolve and validate the icon -------------------------------------
# Explicit argument wins; otherwise default to icon.icnes beside the script
# (resolved from the script location, never the caller's cwd).
if [[ $# -ge 2 ]]; then
    VOLUME_ICON_FILE="$2"
else
    VOLUME_ICON_FILE="$SCRIPT_DIR/icon.icns"
fi

if [[ ! -f "$VOLUME_ICON_FILE" || ! -r "$VOLUME_ICON_FILE" ]]; then
    fail "Icon file missing or unreadable: $VOLUME_ICON_FILE"
fi

VOLUME_ICON_FILE="$(
    cd "$(dirname "$VOLUME_ICON_FILE")" &&
    printf '%s/%s\n' "$PWD" "$(basename "$VOLUME_ICON_FILE")"
)"

if [[ ! -f "$DMG_PATH" ]]; then
    fail "DMG does not exist: $DMG_PATH"
fi
if [[ ! -w "$DMG_PATH" ]]; then
    fail "DMG is not writable: $DMG_PATH"
fi

# --- 2. Preflight osascript --------------------------------------------------
# JXA (osascript) is used instead of a Swift helper: it calls the same
# NSWorkspace API without compiling against the C SDK, so it works even on
# machines whose Swift module building is broken by stale /usr/local/include
# headers. osascript ships with every macOS - no dev tools required.
command -v osascript >/dev/null ||
    fail "osascript not found (macOS only). Cannot set the DMG file icon."

# --- 3. JXA helper ------------------------------------------------------------
# Arguments are passed as ARGV entries (never interpolated into the source).
# NSImage(contentsOfFile:) loads the actual artwork; NSWorkspace.setIcon
# assigns it to the target file and returns a Boolean.
log "Setting custom icon on final DMG file..."

OUT="$(osascript -l JavaScript \
    - "$VOLUME_ICON_FILE" "$DMG_PATH" <<'JXA' 2>&1
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
        return fail('Failed to assign custom icon to: ' + targetPath);
    }
    return 'OK';
}
JXA
)"

if [[ "$OUT" != "OK" ]]; then
    fail "Icon helper failed: $OUT"
fi

log "Custom icon set on: $DMG_PATH"
