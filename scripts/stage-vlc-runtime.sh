#!/usr/bin/env bash
# scripts/stage-vlc-runtime.sh
#
# Stages the macOS VLC 3.0.23 runtime into src-tauri/resources/vlc/, which Tauri
# bundles as Contents/Resources/resources/vlc/. It combines three sources:
#
#   1. The OFFICIAL VideoLAN VLC 3.0.23 app (VLC_OFFICIAL_APP):
#        lib/libvlccore.9.dylib and every plugin in plugins/, except the ones
#        listed under "Left out" below. The official core links only system
#        libraries and is the known-good reference for HDR output.
#
#   2. What scripts/build-vlc-runtime.sh built from the pinned sources
#      (patches/vlc-3.0.23/, NEBULAE_VLC_WORK):
#        lib/libvlc.5.dylib                      navigate-menu + bluray-playlist-id
#                                                + bluray-menu-first
#        plugins/libvideotoolbox_plugin.dylib    vt-305
#        plugins/libcaopengllayer_plugin.dylib   hdr-tonemap
#        plugins/libglconv_cvpx_plugin.dylib     matches the patched converter.h
#        plugins/liblibbluray_plugin.dylib       bluray-playlist-id + bluray-menu-first
#        lib/libbluray.4.dylib                   libbluray 1.5.0, the library the
#        lib/libbluray-j2se-1.5.0.jar            plugin is linked to, and the BD-J
#        lib/libbluray-awt-j2se-1.5.0.jar        jars of exactly that version
#
#   3. The Homebrew libraries the Blu-ray plugin needs (fontconfig, freetype,
#      libpng, libudfread, libintl): copied next to libbluray and rewritten to
#      @rpath references by scripts/relocate-macos-dylibs.py, so that the app
#      does not depend on a Homebrew installation. Their versions, hashes and
#      license files are recorded under licenses/ and in bundled-libraries.json.
#
# Left out of the official plugin set, on purpose:
#   plugins.dat                  VLC validates cache entries by mtime and size and
#                                bundling rewrites mtimes, so a shipped cache is
#                                stale by construction; VLC writes a valid one on
#                                the first launch.
#   libbluray-*-1.4.0.jar        They belong to the libbluray 1.4.0 that is built
#                                into the official Blu-ray plugin. libbluray only
#                                accepts jars of its own version, so the staged
#                                plugin (libbluray 1.5.0) uses the 1.5.0 jars.
#   libmacosx_plugin.dylib       VLC.app's own interface; it needs
#                                Sparkle.framework, which only VLC.app ships.
#   libosx_notifications_plugin  Needs Growl.framework, which only VLC.app ships.
#                                An embedded libvlc never loads either of them,
#                                and both fail to load here: nothing is lost.
#
# The custom artifacts are ABI-compatible with the official core (same 3.0.23,
# install name @rpath/libvlccore.dylib, compatibility version 10.0.0). This
# script checks the compatibility version and that every VLC symbol the custom
# libvlc imports is exported by the staged official core. The result is
# verified by scripts/verify-vlc-runtime.sh, including a strict recursive audit.
#
# Usage (from anywhere):  scripts/stage-vlc-runtime.sh
# Environment:
#   VLC_OFFICIAL_APP     official VLC 3.0.23 app (default /Applications/VLC.app;
#                        CI passes the checksum-verified copy it unpacked)
#   NEBULAE_VLC_WORK     work directory of build-vlc-runtime.sh
#   VLC_BUILD_DIR        VLC build tree (default $NEBULAE_VLC_WORK/vlc-3.0.23-build)
#   LIBBLURAY_PREFIX     libbluray install prefix (default
#                        $NEBULAE_VLC_WORK/libbluray/prefix)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORK="${NEBULAE_VLC_WORK:-$REPO_ROOT/src-tauri/vlc-runtime-build}"
OFFICIAL_APP="${VLC_OFFICIAL_APP:-/Applications/VLC.app}"
OFFICIAL="$OFFICIAL_APP/Contents/MacOS"
BUILD_DIR="${VLC_BUILD_DIR:-$WORK/vlc-3.0.23-build}"
LB_PREFIX="${LIBBLURAY_PREFIX:-$WORK/libbluray/prefix}"
RESOURCES="$REPO_ROOT/src-tauri/resources"
DEST="$RESOURCES/vlc"

fail() { echo "ERROR: $*" >&2; exit 1; }

# has_string FILE TEXT: succeeds when the binary contains TEXT. The listing goes
# through a file because "strings | grep -q" fails under pipefail: grep quits at
# the first match and strings then dies of SIGPIPE.
has_string() {
  local tmp rc=0
  tmp="$(mktemp)"
  strings -a "$1" >"$tmp"
  grep -qF -- "$2" "$tmp" || rc=$?
  rm -f "$tmp"
  return "$rc"
}

if [ "$(uname -s)" != "Darwin" ]; then
  echo "Notice: stage-vlc-runtime.sh stages the macOS runtime and does nothing on $(uname -s)."
  exit 0
fi

# A missing input is an error, never a reason to ship something else.
[ -d "$OFFICIAL" ] || fail "official VLC app not found at $OFFICIAL_APP (set VLC_OFFICIAL_APP to a VLC 3.0.23 app)"
INFO="$WORK/build-info.json"
[ -f "$INFO" ] || fail "no build record at $INFO; run scripts/build-vlc-runtime.sh first"
info() {
  python3 -c 'import json, sys
d = json.load(open(sys.argv[1]))
for key in sys.argv[2].split("."):
    d = d[key]
print(d)' "$INFO" "$1"
}
TARGET_ARCH="$(info target_arch)"
VLC_VERSION="$(info vlc.version)"
LB_VERSION="$(info libbluray.version)"
LB_JAR="libbluray-j2se-${LB_VERSION}.jar"
LB_AWT_JAR="libbluray-awt-j2se-${LB_VERSION}.jar"

echo "=== Staging the VLC ${VLC_VERSION} runtime (${TARGET_ARCH}) ==="
echo "  official binaries : $OFFICIAL_APP"
echo "  custom build      : ${BUILD_DIR#"$REPO_ROOT"/}"
echo "  libbluray         : ${LB_VERSION} (${LB_PREFIX#"$REPO_ROOT"/})"

# --- 1. Source checks -------------------------------------------------------

OFFICIAL_VERSION="$(defaults read "$OFFICIAL_APP/Contents/Info.plist" CFBundleShortVersionString 2>/dev/null || echo unknown)"
[ "$OFFICIAL_VERSION" = "$VLC_VERSION" ] \
  || fail "official VLC at $OFFICIAL_APP is '$OFFICIAL_VERSION', need exactly $VLC_VERSION (ABI contract of the custom artifacts)"

SRC_LIBVLC="$BUILD_DIR/lib/.libs/libvlc.5.dylib"
SRC_VT="$BUILD_DIR/modules/.libs/libvideotoolbox_plugin.dylib"
SRC_GL="$BUILD_DIR/modules/.libs/libcaopengllayer_plugin.dylib"
SRC_CVPX="$BUILD_DIR/modules/.libs/libglconv_cvpx_plugin.dylib"
SRC_BLURAY="$BUILD_DIR/modules/.libs/liblibbluray_plugin.dylib"
for f in "$SRC_LIBVLC" "$SRC_VT" "$SRC_GL" "$SRC_CVPX" "$SRC_BLURAY" \
         "$LB_PREFIX/lib/libbluray.4.dylib" "$LB_PREFIX/share/java/$LB_JAR" "$LB_PREFIX/share/java/$LB_AWT_JAR" \
         "$OFFICIAL/lib/libvlccore.9.dylib"; do
  [ -f "$f" ] || fail "missing input: $f (run scripts/build-vlc-runtime.sh)"
done
[ -d "$OFFICIAL/plugins" ] || fail "official plugins directory missing in $OFFICIAL_APP"
VLC_SRC="$WORK/vlc-${VLC_VERSION}"
LB_SRC="$WORK/libbluray/libbluray-${LB_VERSION}"
for f in "$VLC_SRC/COPYING" "$VLC_SRC/COPYING.LIB" "$LB_SRC/COPYING"; do
  [ -f "$f" ] || fail "missing license text: $f"
done

for f in "$SRC_LIBVLC" "$SRC_VT" "$SRC_GL" "$SRC_CVPX" "$SRC_BLURAY" "$LB_PREFIX/lib/libbluray.4.dylib" "$OFFICIAL/lib/libvlccore.9.dylib"; do
  archs="$(lipo -archs "$f")"
  case " $archs " in
    *" $TARGET_ARCH "*) ;;
    *) fail "$f has architectures '$archs', need $TARGET_ARCH" ;;
  esac
done

# --- 2. Patch and ABI verification ------------------------------------------

has_string "$SRC_BLURAY" "bluray-playlist-id" \
  || fail "the custom Blu-ray plugin lacks the bluray-playlist-id patch (string not found)"
echo "OK: custom Blu-ray plugin contains the bluray-playlist-id patch"
has_string "$SRC_BLURAY" "bluray-menu-open" \
  || fail "the custom Blu-ray plugin lacks the bluray-menu-first patch (string not found)"
echo "OK: custom Blu-ray plugin contains the bluray-menu-first patch"
VT_SYMS="$(nm "$SRC_VT" 2>/dev/null || true)"
grep -q "VTDecompressionSessionCanAcceptFormatDescription" <<<"$VT_SYMS" \
  || fail "the custom videotoolbox plugin lacks the VT-305 patch symbol"
echo "OK: custom videotoolbox plugin contains the VT-305 patch"

LIBVLC_EXPORTS="$(dyld_info -exports "$SRC_LIBVLC" 2>/dev/null || nm -gU "$SRC_LIBVLC")"
for sym in _libvlc_media_player_navigate_menu _libvlc_media_player_get_bluray_playlist_id _libvlc_media_player_get_bluray_menu_open; do
  grep -q "$sym" <<<"$LIBVLC_EXPORTS" || fail "the custom libvlc does not export $sym"
  echo "OK: custom libvlc exports ${sym#_}"
done

# Compatibility version of @rpath/libvlccore.dylib: what the custom libvlc
# imports against what the official core declares.
want="$(otool -L "$SRC_LIBVLC" | awk '/libvlccore\.dylib/ && /compatibility/ { for (i=1;i<=NF;i++) if ($i=="version" && $(i-1) ~ /compatibility/) { v=$(i+1); gsub(/,/, "", v); print v; exit } }')"
have="$(otool -l "$OFFICIAL/lib/libvlccore.9.dylib" | awk '$1=="compatibility" && $2=="version" { print $3; exit }')"
[ -n "$want" ] && [ -n "$have" ] || fail "could not read the libvlccore compatibility versions (want='$want' have='$have')"
[ "$want" = "$have" ] || fail "libvlccore compatibility version mismatch: the custom libvlc wants $want, the official core is $have"
echo "OK: libvlccore compatibility version $want matches"

# Every VLC-namespaced symbol the custom libvlc imports must be exported by the
# official core. libSystem imports are resolved by the system libraries.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
nm -u "$SRC_LIBVLC" | awk '{print $NF}' | sort -u >"$TMP/undefined"
nm -gU "$OFFICIAL/lib/libvlccore.9.dylib" | awk '{print $NF}' | sort -u >"$TMP/core-exports"
comm -23 "$TMP/undefined" "$TMP/core-exports" >"$TMP/missing-all"
grep -E '^_(libvlc|vlm|input|aout|vout|config|var|playlist|vlc|decoder|es|stream|demux|access|module|tz|runnable|interrupt|http|tls|inet|net|url|filesystem)_' "$TMP/missing-all" >"$TMP/missing" || true
[ ! -s "$TMP/missing" ] || fail "the custom libvlc imports VLC symbols the official core does not export: $(tr '\n' ' ' <"$TMP/missing")"
echo "OK: every VLC-namespaced import of the custom libvlc is exported by the official core"

# --- 3. Stage ---------------------------------------------------------------

rm -rf "$DEST/lib" "$DEST/plugins" "$DEST/Frameworks" "$DEST/licenses" "$DEST/bundled-libraries.json"
mkdir -p "$DEST/lib" "$DEST/plugins" "$DEST/licenses"

cp "$OFFICIAL/lib/libvlccore.9.dylib" "$DEST/lib/libvlccore.9.dylib"
ln -s libvlccore.9.dylib "$DEST/lib/libvlccore.dylib"
cp "$SRC_LIBVLC" "$DEST/lib/libvlc.5.dylib"
ln -s libvlc.5.dylib "$DEST/lib/libvlc.dylib"

SKIPPED=""
for f in "$OFFICIAL/plugins/"*; do
  name="$(basename "$f")"
  case "$name" in
    plugins.dat|libbluray-*.jar|libmacosx_plugin.dylib|libosx_notifications_plugin.dylib)
      SKIPPED="$SKIPPED $name"
      continue
      ;;
  esac
  cp -p "$f" "$DEST/plugins/$name"
done
cp "$SRC_VT" "$DEST/plugins/libvideotoolbox_plugin.dylib"
cp "$SRC_GL" "$DEST/plugins/libcaopengllayer_plugin.dylib"
cp "$SRC_CVPX" "$DEST/plugins/libglconv_cvpx_plugin.dylib"
cp "$SRC_BLURAY" "$DEST/plugins/liblibbluray_plugin.dylib"
chmod u+w "$DEST/lib/"*.dylib "$DEST/plugins/"*.dylib

# The Blu-ray plugin links the libbluray built by build-vlc-runtime.sh, which in
# turn links Homebrew libraries. Bring them all into lib/ and rewrite every
# reference. libvlccore stays the untouched official file: the process that
# loads the plugin supplies it.
python3 "$SCRIPT_DIR/relocate-macos-dylibs.py" \
  --lib-dir "$DEST/lib" \
  --provided libvlccore.dylib \
  --add-rpath "@loader_path/../lib" \
  --arch "$TARGET_ARCH" \
  --manifest "$DEST/bundled-libraries.json" \
  --licenses-dir "$DEST/licenses" \
  "$DEST/plugins/liblibbluray_plugin.dylib"

# libbluray finds its BD-J jars next to the library it was loaded from, and only
# under the exact name of its own version.
cp "$LB_PREFIX/share/java/$LB_JAR" "$LB_PREFIX/share/java/$LB_AWT_JAR" "$DEST/lib/"
chmod 644 "$DEST/lib/$LB_JAR" "$DEST/lib/$LB_AWT_JAR"

# Licenses and the corresponding-source information for what was built here.
VLC_URL="$(info vlc.url)"
VLC_SHA256="$(info vlc.sha256)"
LB_URL="$(info libbluray.url)"
LB_SHA256="$(info libbluray.sha256)"

LB_LICENSE_DIR="$DEST/licenses/libbluray-${LB_VERSION}"
mkdir -p "$LB_LICENSE_DIR"
cp "$LB_SRC/COPYING" "$LB_LICENSE_DIR/COPYING"
cat >"$LB_LICENSE_DIR/SOURCE.txt" <<SOURCE_TXT
libbluray ${LB_VERSION} as shipped in Nebulae
============================================

Upstream source : ${LB_URL}
SHA-256         : ${LB_SHA256}

Built unmodified by scripts/build-vlc-runtime.sh (Meson, with the BD-J jars built
by Apache Ant, which is a build tool and is not shipped).

Shipped files   : lib/libbluray.4.dylib
                  lib/${LB_JAR}
                  lib/${LB_AWT_JAR}
                  The library only accepts jars of its own version, so the three
                  files always change together.

Licence         : GNU Lesser General Public License 2.1 (COPYING).
Dependencies    : the Homebrew libraries copied next to it in lib/ are listed, with
                  versions and SHA-256 sums, in ../../bundled-libraries.json; their
                  licence files are in the sibling directories of this one.
SOURCE_TXT

VLC_LICENSE_DIR="$DEST/licenses/vlc-${VLC_VERSION}"
mkdir -p "$VLC_LICENSE_DIR"
cp "$VLC_SRC/COPYING" "$VLC_SRC/COPYING.LIB" "$VLC_LICENSE_DIR/"
cp "$REPO_ROOT/patches/vlc-3.0.23/"*.patch "$VLC_LICENSE_DIR/"
cat >"$VLC_LICENSE_DIR/SOURCE.txt" <<SOURCE_TXT
VLC media player ${VLC_VERSION} as shipped in Nebulae
===================================================

Upstream source : ${VLC_URL}
SHA-256         : ${VLC_SHA256}

Unmodified VideoLAN binaries, taken from the official VLC ${VLC_VERSION} app:
    lib/libvlccore.9.dylib and every plugin in plugins/ except the five below.

Built from the source above with the *.patch files in this directory, applied in
the order given by patches/vlc-3.0.23/README.md of the Nebulae repository, using
scripts/build-vlc-runtime.sh:
    lib/libvlc.5.dylib                      navigate-menu.patch, bluray-playlist-id.patch, bluray-menu-first.patch
    plugins/libvideotoolbox_plugin.dylib    vt-305.patch
    plugins/libcaopengllayer_plugin.dylib   hdr-tonemap.patch
    plugins/libglconv_cvpx_plugin.dylib     built from the source patched by hdr-tonemap.patch
    plugins/liblibbluray_plugin.dylib       bluray-playlist-id.patch, bluray-menu-first.patch

Not shipped: plugins.dat (stale by construction), VLC.app's own interface and
notification plugins (they need Sparkle.framework and Growl.framework, which are
not part of this app) and the libbluray 1.4.0 jars of the official Blu-ray plugin
(replaced by the libbluray ${LB_VERSION} files described in ../libbluray-${LB_VERSION}/SOURCE.txt).

Licences        : COPYING (GNU GPL 2) and COPYING.LIB (GNU LGPL 2.1) from the source
                  tarball. libvlc and libvlccore are licensed under the LGPL 2.1 or
                  later; each plugin carries the licence named in its source files,
                  most under the LGPL 2.1 or later and some under the GPL 2 or later.
SOURCE_TXT

# Index of the notices in licenses/. It also carries the credit that the FreeType
# License asks for in the documentation of a binary distribution, because the app
# does not ship THIRD-PARTY-NOTICES.md.
python3 - "$DEST" "$VLC_VERSION" "$LB_VERSION" <<'PY'
import json
import os
import sys

dest, vlc, lb = sys.argv[1:4]
licence_of = {
    "libudfread": "GNU LGPL 2.1 or later",
    "freetype": "FreeType License (FTL); the alternative is the GNU GPL 2",
    "fontconfig": "HPND-sell-variant, Unicode-3.0, MIT-Modern-Variant and MIT (see COPYING)",
    "libpng": "PNG Reference Library License version 2",
    "gettext": "libintl: GNU LGPL 2.1 or later (the GPL 3 text in COPYING covers the gettext tools)",
}
libs = json.load(open(os.path.join(dest, "bundled-libraries.json")))["libraries"]
rows = []
for e in libs:
    if e.get("origin") != "homebrew":
        continue
    name, version = e["homebrew_formula"], e["homebrew_version"]
    rows.append("    %-20s lib/%s\n        %s" % (
        "%s-%s/" % (name, version), e["name"], licence_of.get(name, "see the licence files in that directory")))

text = f"""Nebulae macOS VLC runtime: components and licences
==================================================

The licence texts and the source information for what is in ../lib and ../plugins
are in this directory, one directory per component. The version and SHA-256 sum of
every file of the runtime are in ../../build-manifest.json; the libraries that were
copied from Homebrew are described in ../bundled-libraries.json.

VLC media player {vlc}                  vlc-{vlc}/
    libvlc and libvlccore are licensed under the GNU LGPL 2.1 or later. Each plugin in
    ../plugins carries the licence named in its source files: most are LGPL 2.1 or
    later, some are GPL 2 or later. Five of the files are built by Nebulae from the VLC
    source with the patches in vlc-{vlc}/; SOURCE.txt there lists them. All other
    files are VideoLAN's own binaries, which include codec and format libraries that
    VideoLAN builds from the "contrib" sources named in the VLC source tree.

libbluray {lb}                          libbluray-{lb}/
    GNU LGPL 2.1. Built unmodified from the upstream source, together with the BD-J
    jars of the same version (lib/libbluray-*-{lb}.jar). SOURCE.txt has the URL and hash.

Libraries copied from Homebrew. Only their load commands and their signature were
changed; bundled-libraries.json has the hash of each file before and after.
{chr(10).join(rows)}

Replacing a library. The LGPL libraries (libvlc, libvlccore, libbluray, libudfread,
libintl) are separate files that the app loads at run time. To use a modified version,
replace the file in ../lib with a build of the same version and ABI, then sign the app
again (codesign --force --deep --sign - Nebulae.app): macOS refuses an app whose
signed contents have changed.

FreeType. This software is based in part on the work of the FreeType Team
(https://www.freetype.org). Portions of this software are copyright (c) 2026 The
FreeType Project (www.freetype.org). All rights reserved.
"""
with open(os.path.join(dest, "licenses", "README.txt"), "w") as fh:
    fh.write(text)
PY

# macOS Finder litter must never reach the app bundle.
find "$RESOURCES" -name .DS_Store -type f -delete

PLUGIN_COUNT="$(find "$DEST/plugins" -type f | wc -l | tr -d ' ')"
[ "$PLUGIN_COUNT" -ge 300 ] || fail "suspiciously few plugins staged: $PLUGIN_COUNT"

# --- 4. Manifest ------------------------------------------------------------

python3 - "$RESOURCES/build-manifest.json" "$DEST" "$REPO_ROOT" "$INFO" "$OFFICIAL_VERSION" "${BUILD_DIR#"$REPO_ROOT"/}" <<'PY'
import datetime
import hashlib
import json
import os
import subprocess
import sys

manifest, dest, repo, info_path, vlc_version, build_dir = sys.argv[1:7]
info = json.load(open(info_path))
bundled = json.load(open(os.path.join(dest, "bundled-libraries.json")))["libraries"]
relocated = {e["name"] for e in bundled}
lb_files = {"lib/libbluray.4.dylib"} | {
    "lib/" + n for n in os.listdir(os.path.join(dest, "lib")) if n.endswith(".jar")
}
custom = {
    "lib/libvlc.5.dylib",
    "plugins/libvideotoolbox_plugin.dylib",
    "plugins/libcaopengllayer_plugin.dylib",
    "plugins/libglconv_cvpx_plugin.dylib",
    "plugins/liblibbluray_plugin.dylib",
}


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def source_of(rel):
    if rel in custom:
        return "custom-build"
    if rel in lb_files:
        return "libbluray-build"
    if rel.startswith("lib/") and os.path.basename(rel) in relocated:
        return "homebrew-relocated"
    if rel.startswith("licenses/") or rel == "bundled-libraries.json":
        return "nebulae-notice"
    return "official-vlc-app"


files = []
links = []
for root, dirs, names in os.walk(dest):
    for name in names + dirs:
        path = os.path.join(root, name)
        rel = os.path.relpath(path, dest)
        if os.path.islink(path):
            links.append({"path": "vlc/" + rel, "target": os.readlink(path)})
        elif os.path.isfile(path):
            files.append({"path": "vlc/" + rel, "sha256": sha256(path), "source": source_of(rel)})
files.sort(key=lambda e: e["path"])
links.sort(key=lambda e: e["path"])

try:
    rev = subprocess.run(["git", "-C", repo, "rev-parse", "--short", "HEAD"], stdout=subprocess.PIPE,
                         stderr=subprocess.DEVNULL, check=True).stdout.decode().strip()
except Exception:
    rev = "unknown"

doc = {
    "staged_at_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "git_rev": rev,
    "vlc_version": vlc_version,
    "target_arch": info["target_arch"],
    "sources": {
        "official_app": "VLC media player %s, libvlccore.9.dylib sha256 %s" % (
            vlc_version, sha256(os.path.join(dest, "lib/libvlccore.9.dylib"))),
        "custom_build": build_dir,
        "custom_patches": ["patches/vlc-3.0.23/%s.patch" % n for n in ("vt-305", "navigate-menu", "hdr-tonemap", "bluray-playlist-id", "bluray-menu-first")],
        "vlc_source": info["vlc"],
        "libbluray_source": info["libbluray"],
        "relocated_homebrew_libraries": bundled,
    },
    "files": files,
    # The Tauri bundler copies a symlink as a regular file with the target's
    # content, so the bundled app is checked against these entries as well.
    "links": links,
}
with open(manifest, "w", encoding="utf-8") as fh:
    json.dump(doc, fh, indent=2)
    fh.write("\n")
PY

echo
echo "=== Staged ==="
echo "  lib/libvlccore.9.dylib        official ($OFFICIAL_VERSION)"
echo "  lib/libvlc.5.dylib            custom (navigate-menu.patch + bluray-playlist-id.patch + bluray-menu-first.patch)"
echo "  lib/libbluray.4.dylib + jars  libbluray ${LB_VERSION}, built from source"
echo "  lib/ (relocated)              $(python3 -c 'import json,sys; print(", ".join(sorted(e["name"] for e in json.load(open(sys.argv[1]))["libraries"])))' "$DEST/bundled-libraries.json")"
echo "  plugins/ ($PLUGIN_COUNT files)  official, except libvideotoolbox, libcaopengllayer, libglconv_cvpx and liblibbluray (custom)"
echo "  not staged from the official app:${SKIPPED}"
echo "  manifest: $RESOURCES/build-manifest.json"
echo

"$SCRIPT_DIR/verify-vlc-runtime.sh"
