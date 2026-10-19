#!/usr/bin/env bash
# bundle-vlc-macos.sh
#
# Stage a pinned VLC 3.0.21 runtime into src-tauri/resources/vlc/
# for LOCAL developer builds on macOS. The release workflow (release.yml)
# does the same steps inline on the macOS runner.
#
# Usage (run from repo root):
#   ./scripts/bundle-vlc-macos.sh [arm64|intel64]
#   (default: auto-detect from uname -m)
#
# Requirements: curl, hdiutil (standard macOS), internet access.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VLC_VERSION="3.0.21"
DEST="$REPO_ROOT/src-tauri/resources/vlc"
TMP="${TMPDIR:-/tmp}/nebulae-vlc-stage"

# Architecture selection
ARCH="${1:-}"
if [ -z "$ARCH" ]; then
    case "$(uname -m)" in
        arm64) ARCH="arm64" ;;
        *)     ARCH="intel64" ;;
    esac
fi
echo "Staging VLC $VLC_VERSION ($ARCH) -> $DEST"

VLC_URL="https://download.videolan.org/pub/videolan/vlc/$VLC_VERSION/macosx/vlc-$VLC_VERSION-$ARCH.dmg"
DMG_PATH="$TMP/vlc-$VLC_VERSION-$ARCH.dmg"
MOUNT_POINT="/tmp/nebulae-vlcmnt"

mkdir -p "$TMP"

# Download (cache locally for iterative builds)
if [ ! -f "$DMG_PATH" ]; then
    echo "Downloading $VLC_URL ..."
    curl -fL --progress-bar -o "$DMG_PATH" "$VLC_URL"
else
    echo "Using cached $DMG_PATH"
fi

# Mount
hdiutil detach "$MOUNT_POINT" -quiet 2>/dev/null || true
hdiutil attach "$DMG_PATH" -mountpoint "$MOUNT_POINT" -nobrowse -quiet
trap 'hdiutil detach "$MOUNT_POINT" -quiet 2>/dev/null || true' EXIT

VLC_APP="$MOUNT_POINT/VLC.app"
if [ ! -d "$VLC_APP" ]; then
    echo "ERROR: VLC.app not found in mounted DMG at $MOUNT_POINT"
    ls "$MOUNT_POINT"
    exit 1
fi

# Stage lib, plugins, Frameworks
mkdir -p "$DEST"
echo "Copying lib/ ..."
cp -R "$VLC_APP/Contents/MacOS/lib" "$DEST/"
echo "Copying plugins/ ..."
cp -R "$VLC_APP/Contents/MacOS/plugins" "$DEST/"
echo "Copying Frameworks/ ..."
cp -R "$VLC_APP/Contents/Frameworks" "$DEST/" 2>/dev/null || echo "(no Frameworks — skipping)"

echo ""
echo "VLC staged:"
ls "$DEST"

# Verify required dylibs
REQUIRED=(libvlc.dylib libvlccore.dylib)
for dylib in "${REQUIRED[@]}"; do
    p="$DEST/lib/$dylib"
    if [ ! -f "$p" ]; then
        echo "ERROR: required file missing after staging: $dylib"
        exit 1
    fi
    SIZE=$(du -sh "$p" | cut -f1)
    echo "OK: $dylib ($SIZE)"
done

PLUGIN_COUNT=$(find "$DEST/plugins" -type f | wc -l | tr -d ' ')
echo "Plugins staged: $PLUGIN_COUNT files"
echo ""
echo "Done. Build with: pnpm tauri build"
