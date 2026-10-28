#!/usr/bin/env bash
# scripts/verify-vlc-runtime.sh [VLC_DIR]
#
# Checks the staged macOS VLC runtime (default src-tauri/resources/vlc/, or the
# vlc directory of a built app) before it is packaged. Run by
# stage-vlc-runtime.sh and again before packaging. The staging policy is
# described in stage-vlc-runtime.sh.
#
#   1. the files, links and patches that must be there, and nothing stale
#   2. libbluray and the BD-J jars are one version
#   3. a strict recursive Mach-O audit: no reference to anything outside the
#      bundle, no unresolved dependency, the right architecture, no .DS_Store
#   4. every image has a valid code signature
#   5. the libraries load the way the app loads them and nothing outside the
#      bundle is pulled in (a separate process; no app, no profile, no network)
#
# NEBULAE_TARGET_ARCH selects the architecture (default arm64).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DEST="${1:-$REPO_ROOT/src-tauri/resources/vlc}"
ARCH="${NEBULAE_TARGET_ARCH:-arm64}"

fail() { echo "ERROR: $*" >&2; exit 1; }
ok() { echo "OK: $*"; }

# has_string FILE TEXT: succeeds when the binary contains TEXT (see
# stage-vlc-runtime.sh for why this does not use "strings | grep -q").
has_string() {
  local tmp rc=0
  tmp="$(mktemp)"
  strings -a "$1" >"$tmp"
  grep -qF -- "$2" "$tmp" || rc=$?
  rm -f "$tmp"
  return "$rc"
}

[ -d "$DEST" ] || fail "no VLC runtime at $DEST"
DEST="$(cd "$DEST" && pwd -P)"

echo "=== Verifying the VLC runtime in $DEST ==="

# --- 1. Files, links, patches, hygiene ----------------------------------------

for f in lib/libvlc.5.dylib lib/libvlccore.9.dylib lib/libbluray.4.dylib \
         plugins/libvideotoolbox_plugin.dylib plugins/libcaopengllayer_plugin.dylib \
         plugins/libglconv_cvpx_plugin.dylib plugins/liblibbluray_plugin.dylib \
         plugins/libdvdnav_plugin.dylib bundled-libraries.json; do
  [ -f "$DEST/$f" ] || fail "required file missing: $f"
done
ok "required files present"

# is_alias NAME TARGET: NAME is a link to TARGET (the staged tree) or a regular file
# with TARGET's content (a built app: Tauri's bundler copies links as files).
is_alias() {
  if [ -L "$1" ]; then
    [ "$(readlink "$1")" = "$(basename "$2")" ]
  else
    [ -f "$1" ] && cmp -s "$1" "$2"
  fi
}
is_alias "$DEST/lib/libvlc.dylib" "$DEST/lib/libvlc.5.dylib" || fail "lib/libvlc.dylib is neither a link to libvlc.5.dylib nor a copy of it"
is_alias "$DEST/lib/libvlccore.dylib" "$DEST/lib/libvlccore.9.dylib" || fail "lib/libvlccore.dylib is neither a link to libvlccore.9.dylib nor a copy of it"

# The core must be the official build, which links system libraries only.
if otool -L "$DEST/lib/libvlccore.9.dylib" | tail -n +3 | grep -vE '^[[:space:]]+(/usr/lib/|/System/)' >/dev/null; then
  fail "libvlccore.9.dylib links something other than system libraries: $(otool -L "$DEST/lib/libvlccore.9.dylib" | tail -n +3 | grep -vE '^[[:space:]]+(/usr/lib/|/System/)')"
fi
ok "libvlccore.9.dylib links only system libraries (official build)"

# Compatibility contract between the custom libvlc and the staged core.
want="$(otool -L "$DEST/lib/libvlc.5.dylib" | awk '/libvlccore\.dylib/ && /compatibility/ { for (i=1;i<=NF;i++) if ($i=="version" && $(i-1) ~ /compatibility/) { v=$(i+1); gsub(/,/, "", v); print v; exit } }')"
have="$(otool -l "$DEST/lib/libvlccore.9.dylib" | awk '$1=="compatibility" && $2=="version" { print $3; exit }')"
[ -n "$want" ] && [ "$want" = "$have" ] || fail "libvlccore compatibility mismatch: libvlc wants '$want', the core is '$have'"
ok "libvlccore compatibility version $want"

# The plugins must use the same module entry points as the official ones.
entry_of() { nm -gU "$1" | grep -o '_vlc_entry__[0-9a-z_]*' || true; }
ENTRY="$(entry_of "$DEST/plugins/libdvdnav_plugin.dylib")"
[ -n "$ENTRY" ] || fail "could not read the module entry point of the official libdvdnav plugin"
for p in libvideotoolbox libcaopengllayer libglconv_cvpx liblibbluray; do
  [ "$(entry_of "$DEST/plugins/${p}_plugin.dylib")" = "$ENTRY" ] \
    || fail "${p}_plugin.dylib does not export the module entry point $ENTRY of the official plugins"
done
ok "custom plugins export the official module entry point ${ENTRY#_}"

VT_SYMS="$(nm -nm "$DEST/plugins/libvideotoolbox_plugin.dylib" 2>/dev/null || true)"
grep -q "VTDecompressionSessionCanAcceptFormatDescription" <<<"$VT_SYMS" \
  || fail "libvideotoolbox_plugin.dylib is missing the VT-305 patch symbol"
ok "libvideotoolbox_plugin.dylib contains the VT-305 patch"

# The export trie is what dlsym resolves, not just the static symbol table.
LIBVLC_EXPORTS="$(dyld_info -exports "$DEST/lib/libvlc.5.dylib" 2>/dev/null || true)"
for sym in _libvlc_media_player_navigate_menu _libvlc_media_player_get_bluray_playlist_id _libvlc_media_player_get_bluray_menu_open; do
  grep -q "$sym" <<<"$LIBVLC_EXPORTS" || fail "libvlc.5.dylib does not export ${sym#_}"
  ok "libvlc.5.dylib exports ${sym#_}"
done

has_string "$DEST/plugins/liblibbluray_plugin.dylib" "bluray-playlist-id" \
  || fail "liblibbluray_plugin.dylib is missing the bluray-playlist-id patch"
ok "liblibbluray_plugin.dylib contains the bluray-playlist-id patch"
has_string "$DEST/plugins/liblibbluray_plugin.dylib" "bluray-menu-open" \
  || fail "liblibbluray_plugin.dylib is missing the bluray-menu-first patch"
ok "liblibbluray_plugin.dylib contains the bluray-menu-first patch"

# No stale framework copy, no plugin cache (mtime-keyed, always stale after
# bundling), no plugin that needs a framework the app does not ship.
[ ! -d "$DEST/Frameworks" ] || fail "stale Frameworks directory present (run stage-vlc-runtime.sh again)"
[ ! -e "$DEST/plugins/plugins.dat" ] || fail "plugins.dat must not be staged (mtime-keyed cache, always stale after bundling)"
for p in libmacosx_plugin.dylib libosx_notifications_plugin.dylib; do
  [ ! -e "$DEST/plugins/$p" ] || fail "plugins/$p needs a framework (Sparkle or Growl) that this app does not ship"
done
DS_STORES="$(find "$DEST" -name .DS_Store)"
[ -z "$DS_STORES" ] || fail ".DS_Store files in the runtime: $(echo "$DS_STORES" | tr '\n' ' ')"
PLUGIN_COUNT="$(find "$DEST/plugins" -type f | wc -l | tr -d ' ')"
[ "$PLUGIN_COUNT" -ge 300 ] || fail "suspiciously few plugins: $PLUGIN_COUNT"
ok "$PLUGIN_COUNT plugins staged; no stale or unloadable files"

# --- 2. libbluray and the BD-J jars are one version -----------------------------

set -- "$DEST"/lib/libbluray-j2se-*.jar
[ "$#" -eq 1 ] && [ -f "$1" ] || fail "expected exactly one lib/libbluray-j2se-<version>.jar"
LB_JAR="$(basename "$1")"
set -- "$DEST"/lib/libbluray-awt-j2se-*.jar
[ "$#" -eq 1 ] && [ -f "$1" ] || fail "expected exactly one lib/libbluray-awt-j2se-<version>.jar"
LB_AWT_JAR="$(basename "$1")"
LB_VERSION="${LB_JAR#libbluray-j2se-}"
LB_VERSION="${LB_VERSION%.jar}"
[ "$LB_AWT_JAR" = "libbluray-awt-j2se-${LB_VERSION}.jar" ] \
  || fail "the BD-J jars are of different versions: $LB_JAR and $LB_AWT_JAR"
# libbluray looks for the jar of its own version, by name, next to itself.
has_string "$DEST/lib/libbluray.4.dylib" "$LB_JAR" \
  || fail "libbluray.4.dylib does not look for $LB_JAR: native library and jars are not the same version"
for jar in "$LB_JAR" "$LB_AWT_JAR"; do
  unzip -tq "$DEST/lib/$jar" >/dev/null || fail "$jar is not a valid jar"
done
JAR_ENTRIES="$(unzip -Z1 "$DEST/lib/$LB_JAR")"
grep -qx "org/videolan/Libbluray.class" <<<"$JAR_ENTRIES" || fail "$LB_JAR does not contain org/videolan/Libbluray.class"
STRAY_JARS="$(find "$DEST" -name '*.jar' -not -path "$DEST/lib/*")"
[ -z "$STRAY_JARS" ] || fail "jar files outside lib/ (libbluray would not find them, and an old one confuses): $(echo "$STRAY_JARS" | tr '\n' ' ')"
ok "libbluray $LB_VERSION: library and both BD-J jars are one version and sit side by side in lib/"

# --- 3. Strict recursive audit --------------------------------------------------

echo "--- strict Mach-O audit"
python3 "$SCRIPT_DIR/audit-macos-bundle.py" --strict --strict-strings --arch "$ARCH" "$DEST" \
  || fail "the audit found problems in the VLC runtime (see above)"

# A bundled library keeps the deployment target it was built with. Report what
# needs a newer macOS than the app declares; NEBULAE_ENFORCE_MINOS=1 fails on it.
DECLARED_MIN_MACOS="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["bundle"]["macOS"]["minimumSystemVersion"])' "$REPO_ROOT/src-tauri/tauri.conf.json" 2>/dev/null || true)"
if [ -n "$DECLARED_MIN_MACOS" ]; then
  MINOS_REPORT="$(python3 "$SCRIPT_DIR/audit-macos-bundle.py" --arch "$ARCH" --max-minos "$DECLARED_MIN_MACOS" "$DEST" | grep -E '^MINOS' || true)"
  if [ -n "$MINOS_REPORT" ]; then
    echo "WARNING: some images need a newer macOS than the declared minimum (${DECLARED_MIN_MACOS}):"
    printf '%s\n' "$MINOS_REPORT"
    [ "${NEBULAE_ENFORCE_MINOS:-0}" != "1" ] || fail "NEBULAE_ENFORCE_MINOS=1"
  else
    ok "every image runs on the declared minimum macOS (${DECLARED_MIN_MACOS})"
  fi
fi

# --- 4. Signatures ----------------------------------------------------------------

# install_name_tool invalidates a signature and arm64 refuses to load an image
# whose signature is invalid, so every modified file has to be signed again.
BAD_SIGNATURES=""
while IFS= read -r image; do
  codesign --verify "$image" >/dev/null 2>&1 || BAD_SIGNATURES="${BAD_SIGNATURES}${image#"$DEST"/} "
done < <(find "$DEST" -type f -name '*.dylib')
[ -z "$BAD_SIGNATURES" ] || fail "invalid or missing code signature: $BAD_SIGNATURES"
ok "every dylib has a valid code signature"

# --- 5. Load check in a separate process -------------------------------------------

LOAD_CHECK="$(mktemp)"
trap 'rm -f "$LOAD_CHECK"' EXIT
cat >"$LOAD_CHECK" <<'PY'
import ctypes
import json
import os
import sys

dest, order, entry, want_version = sys.argv[1:5]
libdir = os.path.join(dest, "lib")
dest_real = os.path.realpath(dest) + os.sep

system = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
system._dyld_image_count.restype = ctypes.c_uint32
system._dyld_get_image_name.restype = ctypes.c_char_p
system._dyld_get_image_name.argtypes = [ctypes.c_uint32]


def images():
    return {os.path.realpath(system._dyld_get_image_name(i).decode()) for i in range(system._dyld_image_count())}


before = images()

# player/engine.rs loads every library in lib/ by absolute path (libvlc itself
# last) so the plugins find the bundled libraries under their install names.
# The directory order is arbitrary there, so both orders are tried.
names = sorted(n for n in os.listdir(libdir) if n.endswith(".dylib") and not n.startswith("libvlc."))
if order == "reverse":
    names.reverse()
handles = {n: ctypes.CDLL(os.path.join(libdir, n)) for n in names}
ctypes.CDLL(os.path.join(libdir, "libvlc.dylib"))
plugin = ctypes.CDLL(os.path.join(dest, "plugins", "liblibbluray_plugin.dylib"))
getattr(plugin, entry.lstrip("_"))

major, minor, micro = ctypes.c_int(), ctypes.c_int(), ctypes.c_int()
handles["libbluray.4.dylib"].bd_get_version(ctypes.byref(major), ctypes.byref(minor), ctypes.byref(micro))
version = "%d.%d.%d" % (major.value, minor.value, micro.value)
if version != want_version:
    sys.exit("libbluray reports version %s, the jars are %s" % (version, want_version))

new = sorted(images() - before)
system_prefixes = ("/usr/lib/", "/System/", "/Library/Apple/")
stray = [p for p in new if not p.startswith(dest_real) and not p.startswith(system_prefixes)]
if stray:
    sys.exit("images loaded from outside the bundle: " + ", ".join(stray))

bundled = {os.path.basename(p) for p in new if p.startswith(dest_real)}
expected = {e["name"] for e in json.load(open(os.path.join(dest, "bundled-libraries.json")))["libraries"]}
missing = sorted(expected - bundled)
if missing:
    sys.exit("bundled libraries that did not load from the bundle: " + ", ".join(missing))
cores = sorted(os.path.basename(p) for p in new if os.path.basename(p).startswith("libvlccore"))
print("%s order: libvlc, %d bundled libraries and the Blu-ray plugin loaded; libbluray %s; nothing from outside the bundle; "
      "libvlccore images: %s" % (order, len(expected), version, ", ".join(cores)))
PY
for order in forward reverse; do
  env -i PATH=/usr/bin:/bin /usr/bin/python3 -B "$LOAD_CHECK" "$DEST" "$order" "$ENTRY" "$LB_VERSION" \
    || fail "the load check failed (${order} order)"
done
ok "load check passed"

echo
echo "=== VLC runtime verification passed ==="
