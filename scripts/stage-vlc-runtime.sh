#!/usr/bin/env bash
# stage-vlc-runtime.sh
#
# Stages the hybrid VLC 3.0.23 runtime into src-tauri/resources/vlc/:
#
#   FROM THE OFFICIAL VideoLAN VLC 3.0.23 app bundle (the same binaries the
#   reference player in /Applications/VLC.app uses):
#     - lib/libvlccore.9.dylib        (core; links only system libraries)
#     - plugins/*                     (full official plugin set, incl. the
#                                      libbluray 1.4.0 jars; EXCEPT
#                                      libvideotoolbox_plugin.dylib)
#
#   FROM THE LOCAL PATCHED BUILD TREE (default /tmp/vlc-3.0.23-build, see
#   patches/vlc-3.0.23/ and BUILD_NOTES.md):
#     - lib/libvlc.5.dylib            (adds libvlc_media_player_navigate_menu,
#                                      patches/vlc-3.0.23/navigate-menu.patch)
#     - plugins/libvideotoolbox_plugin.dylib
#                                     (VT-305 session-reuse patch,
#                                      patches/vlc-3.0.23/vt-305.patch)
#
# Why hybrid: the previously shipped custom-built core linked a Homebrew
# dbus and the staged plugin set was of mixed provenance, which was
# implicated in washed-out UHD HDR output. The official binaries are the
# known-good reference. The two custom artifacts are ABI-compatible with the
# official core (same 3.0.23, same install name @rpath/libvlccore.dylib with
# compatibility version 10.0.0); this script verifies both the compatibility
# version and that every VLC-namespaced symbol the custom libvlc imports is
# exported by the staged official core.
#
# plugins.dat is deliberately NOT staged: VLC validates cache entries by
# mtime+size (src/modules/bank.c) and bundling rewrites mtimes, so any
# shipped cache is stale by construction. Without a cache the first launch
# performs one clean full plugin scan; VLC then writes a valid cache next to
# the plugins.
#
# Usage (from repo root):   ./scripts/stage-vlc-runtime.sh
# Overrides:                VLC_OFFICIAL_APP (default /Applications/VLC.app)
#                           VLC_BUILD_DIR    (default /tmp/vlc-3.0.23-build)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OFFICIAL_APP="${VLC_OFFICIAL_APP:-/Applications/VLC.app}"
OFFICIAL="$OFFICIAL_APP/Contents/MacOS"
BUILD_DIR="${VLC_BUILD_DIR:-/tmp/vlc-3.0.23-build}"
DEST="$REPO_ROOT/src-tauri/resources/vlc"

fail() { echo "ERROR: $*" >&2; exit 1; }

echo "=== Staging hybrid VLC 3.0.23 runtime ==="
echo "  official binaries : $OFFICIAL_APP"
echo "  custom artifacts  : $BUILD_DIR"

# --- 1. Source checks -------------------------------------------------------

[ -d "$OFFICIAL" ] || fail "official VLC app not found at $OFFICIAL_APP (set VLC_OFFICIAL_APP)"
VLC_VER="$(defaults read "$OFFICIAL_APP/Contents/Info.plist" CFBundleShortVersionString 2>/dev/null || echo unknown)"
[ "$VLC_VER" = "3.0.23" ] || fail "official VLC at $OFFICIAL_APP is '$VLC_VER', need exactly 3.0.23 (ABI contract of the custom artifacts)"

SRC_LIBVLC="$BUILD_DIR/lib/.libs/libvlc.5.dylib"
SRC_VT="$BUILD_DIR/modules/.libs/libvideotoolbox_plugin.dylib"
SRC_GL="$BUILD_DIR/modules/.libs/libcaopengllayer_plugin.dylib"
[ -f "$SRC_LIBVLC" ] || fail "custom libvlc missing: $SRC_LIBVLC (build per BUILD_NOTES.md)"
[ -f "$SRC_VT" ]     || fail "custom videotoolbox plugin missing: $SRC_VT (build per BUILD_NOTES.md)"
[ -f "$SRC_GL" ]     || fail "custom caopengllayer plugin missing: $SRC_GL (build per BUILD_NOTES.md)"
[ -f "$OFFICIAL/lib/libvlccore.9.dylib" ] || fail "official libvlccore.9.dylib missing"
[ -d "$OFFICIAL/plugins" ] || fail "official plugins dir missing"

# --- 2. Patch / ABI verification --------------------------------------------

vt_syms="$(nm "$SRC_VT" 2>/dev/null || true)"
echo "$vt_syms" | grep -q "VTDecompressionSessionCanAcceptFormatDescription" \
  || fail "custom videotoolbox plugin lacks the VT-305 patch symbol"
echo "OK: custom videotoolbox plugin contains VT-305 patch"
echo "OK: custom caopengllayer plugin built with HDR tone-mapping patch"

libvlc_syms="$(dyld_info -exports "$SRC_LIBVLC" 2>/dev/null || nm -gU "$SRC_LIBVLC")"
echo "$libvlc_syms" | grep -q "_libvlc_media_player_navigate_menu" \
  || fail "custom libvlc lacks the libvlc_media_player_navigate_menu export"
echo "OK: custom libvlc exports libvlc_media_player_navigate_menu"

# Compatibility version of @rpath/libvlccore.dylib: custom libvlc import vs
# official core's own id must match exactly.
want="$(otool -L "$SRC_LIBVLC" | awk '/libvlccore\.dylib/ && /compatibility/ { for (i=1;i<=NF;i++) if ($i=="version" && $(i-1) ~ /compatibility/) { v=$(i+1); gsub(/,/, "", v); print v; exit } }')"
have="$(otool -l "$OFFICIAL/lib/libvlccore.9.dylib" | awk '$1=="compatibility" && $2=="version" { print $3; exit }')"
[ -n "$want" ] && [ -n "$have" ] || fail "could not read libvlccore compatibility versions (want='$want' have='$have')"
[ "$want" = "$have" ] || fail "libvlccore compatibility version mismatch: custom libvlc wants $want, official core is $have"
echo "OK: libvlccore compatibility version $want matches"

# Every VLC-namespaced symbol the custom libvlc imports must be exported by
# the staged official core. libc/libSystem imports are ignored (resolved by
# the system dylibs both binaries link).
nm -u "$SRC_LIBVLC" | awk '{print $NF}' | sort -u > /tmp/.nebulae_undef.$$
nm -gU "$OFFICIAL/lib/libvlccore.9.dylib" | awk '{print $NF}' | sort -u > /tmp/.nebulae_coreexp.$$
missing="$(comm -23 /tmp/.nebulae_undef.$$ /tmp/.nebulae_coreexp.$$ \
  | grep -E '^_(libvlc|vlm|input|aout|vout|config|var|playlist|vlc|decoder|es|stream|demux|access|module|tz|runnable|interrupt|http|tls|inet|net|url|filesystem|vlc)_' || true)"
rm -f /tmp/.nebulae_undef.$$ /tmp/.nebulae_coreexp.$$
[ -z "$missing" ] || fail "custom libvlc imports VLC symbols the official core does not export: $(echo $missing | tr '\n' ' ')"
echo "OK: all VLC-namespaced imports of custom libvlc are covered by official core"

# --- 3. Stage ----------------------------------------------------------------

rm -rf "$DEST/lib" "$DEST/plugins" "$DEST/Frameworks"
mkdir -p "$DEST/lib" "$DEST/plugins"

cp "$OFFICIAL/lib/libvlccore.9.dylib" "$DEST/lib/libvlccore.9.dylib"
ln -sf libvlccore.9.dylib "$DEST/lib/libvlccore.dylib"
cp "$SRC_LIBVLC" "$DEST/lib/libvlc.5.dylib"
ln -sf libvlc.5.dylib "$DEST/lib/libvlc.dylib"

cp "$OFFICIAL/plugins/"* "$DEST/plugins/"
rm -f "$DEST/plugins/plugins.dat"   # cache is mtime-keyed; bundling rewrites mtimes
cp "$SRC_VT" "$DEST/plugins/libvideotoolbox_plugin.dylib"
cp "$SRC_GL" "$DEST/plugins/libcaopengllayer_plugin.dylib"

PLUGIN_COUNT=$(find "$DEST/plugins" -type f | wc -l | tr -d ' ')
[ "$PLUGIN_COUNT" -ge 300 ] || fail "suspiciously few plugins staged: $PLUGIN_COUNT"

# --- 4. Manifest --------------------------------------------------------------

GIT_REV="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
MANIFEST="$REPO_ROOT/src-tauri/resources/build-manifest.json"
{
  echo "{"
  echo "  \"staged_at_utc\": \"$STAMP\","
  echo "  \"git_rev\": \"$GIT_REV\","
  echo "  \"vlc_version\": \"$VLC_VER\","
  echo "  \"sources\": {"
  echo "    \"official_app\": \"$OFFICIAL_APP\","
  echo "    \"custom_build_dir\": \"$BUILD_DIR\","
  echo "    \"custom_patches\": [\"patches/vlc-3.0.23/vt-305.patch\", \"patches/vlc-3.0.23/navigate-menu.patch\", \"patches/vlc-3.0.23/hdr-tonemap.patch\"]"
  echo "  },"
  echo "  \"files\": ["
  first=1
  find "$DEST" -type f | sort | while IFS= read -r f; do
    rel="${f#"$DEST"/}"
    case "$rel" in
      lib/libvlc.5.dylib|plugins/libvideotoolbox_plugin.dylib|plugins/libcaopengllayer_plugin.dylib) src="custom-build" ;;
      *) src="official-vlc-app" ;;
    esac
    sha="$(shasum -a 256 "$f" | awk '{print $1}')"
    if [ "$first" -eq 1 ]; then first=0; else echo ","; fi
    printf '    { "path": "vlc/%s", "sha256": "%s", "source": "%s" }' "$rel" "$sha" "$src"
  done
  echo ""
  echo "  ]"
  echo "}"
} > "$MANIFEST"

echo ""
echo "=== Staged ==="
echo "  lib/libvlccore.9.dylib        official ($VLC_VER)"
echo "  lib/libvlc.5.dylib            custom (navigate-menu.patch)"
echo "  plugins/ ($PLUGIN_COUNT files) official, except libvideotoolbox_plugin.dylib custom (vt-305.patch) and libcaopengllayer_plugin.dylib custom (hdr-tonemap.patch)"
echo "  manifest: $MANIFEST"
echo ""

sh "$REPO_ROOT/scripts/verify-vlc-runtime.sh"
