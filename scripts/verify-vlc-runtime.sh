#!/usr/bin/env bash
# verify-vlc-runtime.sh
#
# Pre-build check: verifies that the VLC runtime staged in
# src-tauri/resources/vlc/ contains the VT-305 patched modules and
# required core libraries. Run before `pnpm tauri build` to ensure
# deterministic packaging.
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$REPO_ROOT/src-tauri/resources/vlc"

echo "=== Verifying VLC runtime staging ==="

# Required core libraries
REQUIRED_LIBS=(
  "lib/libvlc.5.dylib"
  "lib/libvlccore.9.dylib"
)

for lib in "${REQUIRED_LIBS[@]}"; do
  p="$DEST/$lib"
  if [ ! -f "$p" ]; then
    echo "ERROR: required library missing: $lib"
    exit 1
  fi
  SIZE=$(du -sh "$p" | cut -f1)
  echo "OK: $lib ($SIZE)"
done

# Required modules (both must be present)
REQUIRED_PLUGINS=(
  "plugins/libvideotoolbox_plugin.dylib"
  "plugins/libcaopengllayer_plugin.dylib"
)

for plugin in "${REQUIRED_PLUGINS[@]}"; do
  p="$DEST/$plugin"
  if [ ! -f "$p" ]; then
    echo "ERROR: required plugin missing: $plugin"
    exit 1
  fi
  SIZE=$(du -sh "$p" | cut -f1)
  echo "OK: $plugin ($SIZE)"
done

# Verify VT-305 patch is present in videotoolbox codec
VT_PLUGIN="$DEST/plugins/libvideotoolbox_plugin.dylib"
nm_output=$(nm -nm "$VT_PLUGIN" 2>/dev/null || true)
if echo "$nm_output" | grep -q "VTDecompressionSessionCanAcceptFormatDescription"; then
  echo "OK: libvideotoolbox_plugin.dylib contains VT-305 patch"
else
  echo "ERROR: libvideotoolbox_plugin.dylib missing VT-305 patch"
  exit 1
fi

PLUGIN_COUNT=$(find "$DEST/plugins" -type f | wc -l | tr -d ' ')
echo "Total plugins staged: $PLUGIN_COUNT"

echo ""
echo "=== VLC runtime verification passed ==="
echo "Ready for deterministic build: pnpm tauri build"
