#!/usr/bin/env bash
# verify-vlc-runtime.sh
#
# Pre-build check for the staged hybrid VLC 3.0.23 runtime in
# src-tauri/resources/vlc/. Run by stage-vlc-runtime.sh and again before
# packaging. See stage-vlc-runtime.sh for the staging policy.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$REPO_ROOT/src-tauri/resources/vlc"

fail() { echo "ERROR: $*" >&2; exit 1; }

echo "=== Verifying VLC runtime staging ==="

# Required files
for f in lib/libvlc.5.dylib lib/libvlccore.9.dylib \
         plugins/libvideotoolbox_plugin.dylib plugins/libcaopengllayer_plugin.dylib \
         plugins/libglconv_cvpx_plugin.dylib \
         plugins/liblibbluray_plugin.dylib plugins/libdvdnav_plugin.dylib; do
  [ -f "$DEST/$f" ] || fail "required file missing: $f"
  echo "OK: $f ($(du -sh "$DEST/$f" | cut -f1))"
done

# Symlinks the loader relies on
[ -L "$DEST/lib/libvlc.dylib" ]    || fail "lib/libvlc.dylib symlink missing"
[ -L "$DEST/lib/libvlccore.dylib" ] || fail "lib/libvlccore.dylib symlink missing"

# Core must be the official build: no Homebrew/local-machine dependencies.
if otool -L "$DEST/lib/libvlccore.9.dylib" | grep -q '/opt/homebrew\|/usr/local'; then
  fail "staged libvlccore links a local-machine dependency ($(otool -L "$DEST/lib/libvlccore.9.dylib" | grep '/opt/homebrew\|/usr/local' | awk '{print $1}')) — stage the official build"
fi
echo "OK: libvlccore.9.dylib links only system libraries (official build)"

# Compatibility contract between custom libvlc and the staged core.
want="$(otool -L "$DEST/lib/libvlc.5.dylib" | awk '/libvlccore\.dylib/ && /compatibility/ { for (i=1;i<=NF;i++) if ($i=="version" && $(i-1) ~ /compatibility/) { v=$(i+1); gsub(/,/, "", v); print v; exit } }')"
have="$(otool -l "$DEST/lib/libvlccore.9.dylib" | awk '$1=="compatibility" && $2=="version" { print $3; exit }')"
[ "$want" = "$have" ] || fail "libvlccore compatibility mismatch: libvlc wants '$want', core is '$have'"
echo "OK: libvlccore compatibility version $want"

# VT-305 patch in the videotoolbox codec
vt_syms="$(nm -nm "$DEST/plugins/libvideotoolbox_plugin.dylib" 2>/dev/null || true)"
echo "$vt_syms" | grep -q "VTDecompressionSessionCanAcceptFormatDescription" \
  || fail "libvideotoolbox_plugin.dylib missing VT-305 patch symbol"
echo "OK: libvideotoolbox_plugin.dylib contains VT-305 patch"

# navigate_menu extension: check the export trie (what dlsym resolves),
# not just the static symbol table.
libvlc_exports="$(dyld_info -exports "$DEST/lib/libvlc.5.dylib" 2>/dev/null || true)"
echo "$libvlc_exports" | grep -q "_libvlc_media_player_navigate_menu" \
  || fail "libvlc.5.dylib does not export libvlc_media_player_navigate_menu"
echo "OK: libvlc.5.dylib exports libvlc_media_player_navigate_menu"
echo "$libvlc_exports" | grep -q "_libvlc_media_player_get_bluray_playlist_id" \
  || fail "libvlc.5.dylib does not export libvlc_media_player_get_bluray_playlist_id"
echo "OK: libvlc.5.dylib exports libvlc_media_player_get_bluray_playlist_id"

# bluray-playlist-id patch: "bluray-playlist-id" must appear in the bluray plugin.
strings "$DEST/plugins/liblibbluray_plugin.dylib" 2>/dev/null | grep -q "bluray-playlist-id" \
  || fail "liblibbluray_plugin.dylib missing bluray-playlist-id patch"
echo "OK: liblibbluray_plugin.dylib contains bluray-playlist-id patch"

# Hygiene: no stale framework copies, no plugin cache (mtime-keyed; bundling
# rewrites mtimes so a shipped cache is always stale).
[ ! -d "$DEST/Frameworks" ] || fail "stale Frameworks directory present (re-run stage-vlc-runtime.sh)"
[ ! -e "$DEST/plugins/plugins.dat" ] || fail "plugins.dat must not be staged (mtime-keyed cache, always stale after bundling)"

PLUGIN_COUNT=$(find "$DEST/plugins" -type f | wc -l | tr -d ' ')
[ "$PLUGIN_COUNT" -ge 300 ] || fail "suspiciously few plugins: $PLUGIN_COUNT"
echo "OK: $PLUGIN_COUNT plugins staged"

echo ""
echo "=== VLC runtime verification passed ==="
