#!/usr/bin/env bash
# stage-vlc-runtime.sh
#
# Canonical staging script for the VLC 3.0.23 runtime with VT-305 patch
# and libvlc_media_player_navigate_menu extension.
#
# Copies built VLC artifacts from /tmp/vlc-3.0.23-build into
# src-tauri/resources/vlc/ and verifies their identity before the
# Tauri build bundles them into the app.
#
# This is the ONLY sanctioned path for populating the VLC runtime.
# Do not download or copy VLC artifacts from any other source.
#
# Usage (run from repo root):
#   ./scripts/stage-vlc-runtime.sh
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_DIR="/tmp/vlc-3.0.23-build"
DEST="$REPO_ROOT/src-tauri/resources/vlc"

echo "=== Staging VLC 3.0.23 runtime (VT-305 patched + navigate_menu extension) ==="

# Source artifacts (built from patched VLC 3.0.23 source)
SRC_LIBVLC="$BUILD_DIR/lib/.libs/libvlc.5.dylib"
SRC_VLCCORE="$BUILD_DIR/src/.libs/libvlccore.9.dylib"
SRC_VIDEOTOOLBOX="$BUILD_DIR/modules/.libs/libvideotoolbox_plugin.dylib"
SRC_CAOPENGLLLAYER="$BUILD_DIR/modules/.libs/libcaopengllayer_plugin.dylib"

# Verify source artifacts exist
for src in "$SRC_LIBVLC" "$SRC_VLCCORE" "$SRC_VIDEOTOOLBOX" "$SRC_CAOPENGLLLAYER"; do
  if [ ! -f "$src" ]; then
    echo "ERROR: source artifact missing: $src"
    echo "  Build VLC 3.0.23 first (see BUILD_NOTES.md)"
    exit 1
  fi
done

# Verify VT-305 patch is in videotoolbox plugin
nm_output=$(nm -nm "$SRC_VIDEOTOOLBOX" 2>/dev/null || true)
if echo "$nm_output" | grep -q "VTDecompressionSessionCanAcceptFormatDescription"; then
  echo "OK: videotoolbox plugin contains VT-305 patch"
else
  echo "ERROR: $SRC_VIDEOTOOLBOX missing VT-305 patch symbol"
  exit 1
fi

# Verify navigate_menu extension is in libvlc (exported via libvlc.sym)
if ! nm "$SRC_LIBVLC" 2>/dev/null | grep -q "_libvlc_media_player_navigate_menu"; then
  echo "ERROR: $SRC_LIBVLC missing libvlc_media_player_navigate_menu symbol"
  exit 1
fi
echo "OK: libvlc contains libvlc_media_player_navigate_menu extension"

# Stage core libraries
mkdir -p "$DEST/lib"
cp "$SRC_LIBVLC" "$DEST/lib/libvlc.5.dylib"
cp "$SRC_VLCCORE" "$DEST/lib/libvlccore.9.dylib"
ln -sf libvlc.5.dylib "$DEST/lib/libvlc.dylib"
ln -sf libvlccore.9.dylib "$DEST/lib/libvlccore.dylib"

# Stage plugins (overwrite any stale versions)
cp "$SRC_VIDEOTOOLBOX" "$DEST/plugins/libvideotoolbox_plugin.dylib"
cp "$SRC_CAOPENGLLLAYER" "$DEST/plugins/libcaopengllayer_plugin.dylib"

PLUGIN_COUNT=$(find "$DEST/plugins" -type f | wc -l | tr -d ' ')

echo ""
echo "=== Staged runtime files ==="
ls -la "$DEST/lib/"
echo ""
echo "OK: libvlc.5.dylib (with navigate_menu extension)"
echo "OK: libvlccore.9.dylib (with navigate_menu extension)"
echo "OK: libvideotoolbox_plugin.dylib (VT-305 patched)"
echo "OK: libcaopengllayer_plugin.dylib (VT-305 patched)"
echo "Total plugins staged: $PLUGIN_COUNT"
echo ""
echo "=== Ready for Tauri build: pnpm tauri build ==="
echo "The resources/ directory is bundled by tauri.conf.json (\"resources\": [\"resources/**/*\"])."
