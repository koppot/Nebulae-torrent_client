#!/usr/bin/env bash
# scripts/build-vlc-runtime.sh
#
# Builds, from pinned and checksum-verified sources, everything in Nebulae's
# macOS VLC runtime that is not an unmodified official VideoLAN binary:
#
#   1. libbluray 1.5.0: the native library AND the BD-J jars of the same
#      version (libbluray refuses jars of another version, so they are built
#      together and shipped together).
#   2. VLC 3.0.23 with patches/vlc-3.0.23 applied by scripts/apply-vlc-patches.sh:
#        libvlc.5.dylib                      navigate-menu + bluray-playlist-id
#        libvideotoolbox_plugin.dylib        vt-305
#        libcaopengllayer_plugin.dylib       hdr-tonemap
#        libglconv_cvpx_plugin.dylib         (matches the patched converter.h)
#        liblibbluray_plugin.dylib           bluray-playlist-id, linked to (1)
#
# Nothing here falls back to an unpatched or differently versioned component:
# every failure stops the script with a non-zero status.
#
# scripts/stage-vlc-runtime.sh then combines these artifacts with the official
# VLC 3.0.23 binaries, relocates their dependencies into the bundle and audits
# the result.
#
# Requirements (Homebrew names; the same ones the release workflow installs):
#   meson ninja pkgconf libudfread fontconfig freetype gettext
#   bison (optional), Xcode command line tools, a JDK 18 or newer (javac).
# Apache Ant, which only compiles the BD-J jars and is not shipped, is fetched
# as a pinned, checksum-verified upstream release unless NEBULAE_ANT_HOME names
# an existing installation.
#
# Usage (from anywhere):  scripts/build-vlc-runtime.sh
# Environment:
#   NEBULAE_VLC_WORK     work directory (default: src-tauri/vlc-runtime-build,
#                        git-ignored)
#   NEBULAE_TARGET_ARCH  arm64 (default) or x86_64; must equal the host, since
#                        the Homebrew libraries linked here are host-arch only
#   JAVA_HOME            JDK to build the jars with (default: newest JDK >= 18
#                        known to /usr/libexec/java_home)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORK="${NEBULAE_VLC_WORK:-$REPO_ROOT/src-tauri/vlc-runtime-build}"
TARGET_ARCH="${NEBULAE_TARGET_ARCH:-arm64}"

VLC_VERSION="3.0.23"
VLC_URL="https://download.videolan.org/pub/videolan/vlc/${VLC_VERSION}/vlc-${VLC_VERSION}.tar.xz"
VLC_SHA256="e891cae6aa3ccda69bf94173d5105cbc55c7a7d9b1d21b9b21666e69eff3e7e0"

LIBBLURAY_VERSION="1.5.0"
LIBBLURAY_URL="https://download.videolan.org/pub/videolan/libbluray/${LIBBLURAY_VERSION}/libbluray-${LIBBLURAY_VERSION}.tar.xz"
LIBBLURAY_SHA256="f676408e91a5d321abf8b8d4dfdae36205c297dab5c54c3ec519639025f474a2"

ANT_VERSION="1.10.18"
ANT_URL="https://archive.apache.org/dist/ant/binaries/apache-ant-${ANT_VERSION}-bin.tar.xz"
ANT_SHA512="3cc0c48a744dd21e8e7fed30c555e8efcac709839c527c57600ab6d7d02383207d7c619d434bb1c9b5ff3f956c77a0c587b1dcfbf3c72ec9d78300caf82c0567"

fail() { echo "ERROR: $*" >&2; exit 1; }
step() { echo; echo "==> $*"; }

[ "$(uname -s)" = "Darwin" ] || fail "build-vlc-runtime.sh builds the macOS runtime and only runs on macOS"
HOST_ARCH="$(uname -m)"
[ "$HOST_ARCH" = "$TARGET_ARCH" ] || fail "target architecture is $TARGET_ARCH but this machine is $HOST_ARCH. The Homebrew libraries linked here exist only for the host architecture, so a $TARGET_ARCH runtime cannot be built on it."

# Deployment target: what the app itself declares, so the artifacts built here
# do not demand a newer macOS than the main executable.
MIN_MACOS="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["bundle"]["macOS"]["minimumSystemVersion"])' "$REPO_ROOT/src-tauri/tauri.conf.json")" \
    || fail "could not read bundle.macOS.minimumSystemVersion from src-tauri/tauri.conf.json"
[ -n "$MIN_MACOS" ] || fail "empty minimumSystemVersion in src-tauri/tauri.conf.json"

for tool in curl shasum tar make meson ninja pkg-config python3 install_name_tool lipo otool; do
    command -v "$tool" >/dev/null 2>&1 || fail "required tool not found: $tool"
done

DOWNLOADS="$WORK/downloads"
LOGS="$WORK/logs"
mkdir -p "$DOWNLOADS" "$LOGS"

# --- helpers -----------------------------------------------------------------

# download_verified URL DEST ALGORITHM(256|512) EXPECTED_HEX
download_verified() {
    local url="$1" dest="$2" algo="$3" want="$4" got
    if [ -f "$dest" ]; then
        got="$(shasum -a "$algo" "$dest" | awk '{print $1}')"
        [ "$got" = "$want" ] && { echo "    cached and verified: $(basename "$dest")"; return 0; }
        echo "    cached copy of $(basename "$dest") does not match the pinned SHA-$algo; downloading again"
        rm -f "$dest"
    fi
    echo "    downloading $url"
    curl -fL --retry 3 --retry-delay 2 -sS -o "$dest.part" "$url" || { rm -f "$dest.part"; fail "download failed: $url"; }
    got="$(shasum -a "$algo" "$dest.part" | awk '{print $1}')"
    if [ "$got" != "$want" ]; then
        rm -f "$dest.part"
        fail "SHA-$algo mismatch for $url
  expected $want
  actual   $got"
    fi
    mv "$dest.part" "$dest"
    echo "    SHA-$algo verified: $(basename "$dest")"
}

# run_logged LOGNAME COMMAND...  (prints the log tail when the command fails)
run_logged() {
    local name="$1"; shift
    if ! "$@" >"$LOGS/$name.log" 2>&1; then
        echo "ERROR: $name failed (full log: $LOGS/$name.log); last lines:" >&2
        tail -n 40 "$LOGS/$name.log" >&2
        exit 1
    fi
}

archs_of() { lipo -archs "$1" 2>/dev/null; }
minos_of() { otool -l "$1" | awk '/LC_BUILD_VERSION/ {f=1} f && $1=="minos" {print $2; exit}'; }

# Paths that belong to this build machine rather than to the product.
BUILD_PATH_RE="^(/Users/|/private/|/opt/homebrew|/usr/local/(Cellar|opt)/|$WORK)"

# fail_on_build_paths FILE
# Stops when FILE names a directory of this machine in its C strings or in the
# debug map (the stabs nm -a lists). The install names and library paths in
# the load commands are not looked at: scripts/relocate-macos-dylibs.py
# rewrites them when the runtime is staged. The listings go to files, not into
# "... | grep -q": grep quits at the first match, the producer dies of SIGPIPE
# and, under pipefail, the pipeline then reports failure although it matched.
fail_on_build_paths() {
    local f="$1" name strings_out nm_out
    name="$(basename "$f")"
    strings_out="$LOGS/strings-$name.txt"
    nm_out="$LOGS/nm-$name.txt"
    strings -a "$f" >"$strings_out"
    nm -a "$f" >"$nm_out" 2>/dev/null || true
    if grep -qE "$BUILD_PATH_RE" "$strings_out"; then
        fail "$name contains a build-machine path: $(grep -E "$BUILD_PATH_RE" "$strings_out" | head -3 | tr '\n' ' ')"
    fi
    if grep -qE ' (OSO|SO|SOL) ' "$nm_out"; then
        fail "$name carries a debug map naming build-machine paths: $(grep -E ' (OSO|SO|SOL) ' "$nm_out" | head -2 | tr '\n' ' ')"
    fi
}

# true when dotted version $1 is greater than $2
version_gt() { [ "$(printf '%s\n%s\n' "$1" "$2" | sort -t. -k1,1n -k2,2n -k3,3n | tail -n1)" = "$1" ] && [ "$1" != "$2" ]; }

# --- tool-chain ----------------------------------------------------------------

step "Locating the JDK and Apache Ant that build the BD-J jars"
if [ -z "${JAVA_HOME:-}" ] || [ ! -x "$JAVA_HOME/bin/javac" ]; then
    JAVA_HOME="$(/usr/libexec/java_home -v 18+ 2>/dev/null || true)"
fi
[ -n "$JAVA_HOME" ] && [ -x "$JAVA_HOME/bin/javac" ] || fail "no JDK 18 or newer found. Install one (the release workflow uses Temurin 21) or set JAVA_HOME. libbluray builds its jars with the Java 8 source level only from javac 18 onwards."
export JAVA_HOME
export PATH="$JAVA_HOME/bin:$PATH"
echo "    JDK: $("$JAVA_HOME/bin/javac" -version 2>&1) ($JAVA_HOME)"

if [ -n "${NEBULAE_ANT_HOME:-}" ]; then
    ANT_HOME="$NEBULAE_ANT_HOME"
else
    download_verified "$ANT_URL" "$DOWNLOADS/apache-ant-${ANT_VERSION}-bin.tar.xz" 512 "$ANT_SHA512"
    ANT_HOME="$WORK/tools/apache-ant-${ANT_VERSION}"
    if [ ! -x "$ANT_HOME/bin/ant" ]; then
        rm -rf "$WORK/tools"
        mkdir -p "$WORK/tools"
        tar -xJf "$DOWNLOADS/apache-ant-${ANT_VERSION}-bin.tar.xz" -C "$WORK/tools"
    fi
fi
[ -x "$ANT_HOME/bin/ant" ] || fail "Apache Ant not found at $ANT_HOME"
export ANT_HOME
export PATH="$ANT_HOME/bin:$PATH"
echo "    $(ant -version 2>&1)"

# Homebrew gettext supplies libintl, which VLC's Blu-ray access module needs and
# macOS does not provide.
GETTEXT_PREFIX="${GETTEXT_PREFIX:-$(brew --prefix gettext 2>/dev/null || true)}"
[ -n "$GETTEXT_PREFIX" ] && [ -f "$GETTEXT_PREFIX/lib/libintl.dylib" ] || fail "Homebrew gettext (libintl) not found; run: brew install gettext"
BISON_PREFIX="$(brew --prefix bison 2>/dev/null || true)"
if [ -n "$BISON_PREFIX" ] && [ -x "$BISON_PREFIX/bin/bison" ]; then
    export PATH="$BISON_PREFIX/bin:$PATH"
fi
for pc in libudfread fontconfig freetype2 libxml-2.0; do
    pkg-config --exists "$pc" || fail "pkg-config cannot find $pc; run: brew install libudfread fontconfig freetype"
done
if ! pkg-config --atleast-version=1.2.0 libudfread; then
    fail "libudfread $(pkg-config --modversion libudfread) found, libbluray $LIBBLURAY_VERSION needs 1.2.0 or newer"
fi

# --- 1. libbluray ---------------------------------------------------------------

step "libbluray ${LIBBLURAY_VERSION} (native library + BD-J jars)"
LB_DIR="$WORK/libbluray"
LB_SRC="$LB_DIR/libbluray-${LIBBLURAY_VERSION}"
LB_BUILD="$LB_DIR/build"
LB_PREFIX="$LB_DIR/prefix"
download_verified "$LIBBLURAY_URL" "$DOWNLOADS/libbluray-${LIBBLURAY_VERSION}.tar.xz" 256 "$LIBBLURAY_SHA256"
rm -rf "$LB_DIR"
mkdir -p "$LB_DIR"
tar -xJf "$DOWNLOADS/libbluray-${LIBBLURAY_VERSION}.tar.xz" -C "$LB_DIR"

# The options mirror the Homebrew build that Nebulae has shipped (fontconfig,
# freetype and libxml2 enabled) and add the jars, which the Homebrew bottle
# leaves out. jdk_home stays unset on purpose: a value would be compiled into
# the library as a build-machine path.
export MACOSX_DEPLOYMENT_TARGET="$MIN_MACOS"
run_logged libbluray-setup meson setup "$LB_BUILD" "$LB_SRC" \
    --prefix="$LB_PREFIX" --buildtype=release --default-library=shared --wrap-mode=nofallback \
    -Dbdj_jar=enabled -Dbdj_type=j2se \
    -Dfontconfig=enabled -Dfreetype=enabled -Dlibxml2=enabled \
    -Denable_tools=false -Denable_docs=false -Denable_devtools=false -Denable_examples=false
run_logged libbluray-compile meson compile -C "$LB_BUILD"
run_logged libbluray-install meson install -C "$LB_BUILD"
unset MACOSX_DEPLOYMENT_TARGET

LB_DYLIB="$LB_PREFIX/lib/libbluray.4.dylib"
LB_JAR="$LB_PREFIX/share/java/libbluray-j2se-${LIBBLURAY_VERSION}.jar"
LB_AWT_JAR="$LB_PREFIX/share/java/libbluray-awt-j2se-${LIBBLURAY_VERSION}.jar"
for f in "$LB_DYLIB" "$LB_JAR" "$LB_AWT_JAR" "$LB_PREFIX/lib/pkgconfig/libbluray.pc"; do
    [ -f "$f" ] || fail "libbluray build did not produce $f"
done
[ "$(archs_of "$LB_DYLIB")" = "$TARGET_ARCH" ] || fail "libbluray was built for '$(archs_of "$LB_DYLIB")', expected $TARGET_ARCH"
# The strings listing goes to a file, not into "strings | grep -q": under
# pipefail, grep -q quits at the first match, strings then dies of SIGPIPE and
# the pipeline reports failure even though the pattern was found.
LB_STRINGS="$LOGS/libbluray-dylib-strings.txt"
strings -a "$LB_DYLIB" >"$LB_STRINGS"
# The library looks for exactly this jar name next to itself.
grep -qxF "libbluray-j2se-${LIBBLURAY_VERSION}.jar" "$LB_STRINGS" \
    || fail "libbluray.4.dylib does not look for libbluray-j2se-${LIBBLURAY_VERSION}.jar; native library and jars are not the same version"
fail_on_build_paths "$LB_DYLIB"
# The jars must be loadable by the JRE the app ships (Java 21): class file
# major version 52 is Java 8 bytecode, which every newer JRE runs.
major="$(unzip -p "$LB_JAR" org/videolan/Libbluray.class | od -An -tu1 -j6 -N2 | awk 'NF >= 2 {print $1 * 256 + $2}')"
[ "$major" = "52" ] || fail "libbluray jar classes have major version '$major', expected 52 (Java 8)"
echo "    built: $(basename "$LB_DYLIB") + $(basename "$LB_JAR") + $(basename "$LB_AWT_JAR") (class major $major)"

# --- 2. VLC ---------------------------------------------------------------------

step "VLC ${VLC_VERSION} with the Nebulae patches"
VLC_SRC="$WORK/vlc-${VLC_VERSION}"
VLC_BUILD="$WORK/vlc-${VLC_VERSION}-build"
download_verified "$VLC_URL" "$DOWNLOADS/vlc-${VLC_VERSION}.tar.xz" 256 "$VLC_SHA256"
rm -rf "$VLC_SRC" "$VLC_BUILD" "$WORK/vlc-${VLC_VERSION}-install"
tar -xJf "$DOWNLOADS/vlc-${VLC_VERSION}.tar.xz" -C "$WORK"
"$SCRIPT_DIR/apply-vlc-patches.sh" "$VLC_SRC" | tee "$LOGS/vlc-patches.log" | sed 's/^/    /'

mkdir -p "$VLC_BUILD"
cd "$VLC_BUILD"
# Only libvlc and the five modules above are needed; the official binaries
# supply everything else. --disable-sparkle keeps configure from requiring the
# Sparkle framework for VLC.app's own interface, which is not built here.
# libbluray is found through the pkg-config file of the build made above, never
# through Homebrew's copy.
export PKG_CONFIG_PATH="$LB_PREFIX/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
# The shipped binaries must not carry the build machine's directory names. They
# would get in twice: as __FILE__ strings (VLC's assertions and log calls) and,
# for -g builds, as the object-file paths the linker records in the debug map.
# The prefix maps rewrite the first, -g0 avoids the second.
case "$WORK" in *[[:space:]]*) fail "the work directory must not contain spaces: $WORK" ;; esac
PREFIX_MAPS="-ffile-prefix-map=$VLC_SRC/=vlc-${VLC_VERSION}/ -ffile-prefix-map=$VLC_BUILD/=vlc-${VLC_VERSION}-build/ -ffile-prefix-map=$LB_PREFIX/=libbluray-${LIBBLURAY_VERSION}/ -ffile-prefix-map=$REPO_ROOT/=nebulae/"
run_logged vlc-configure "$VLC_SRC/configure" --prefix="$WORK/vlc-${VLC_VERSION}-install" \
    --disable-qt --disable-lua --disable-vdpau --disable-wayland --disable-sparkle \
    --enable-vlc --enable-bluray --with-macosx-version-min="$MIN_MACOS" \
    CFLAGS="-O2 -g0 $PREFIX_MAPS" CXXFLAGS="-O2 -g0 $PREFIX_MAPS" OBJCFLAGS="-O2 -g0 $PREFIX_MAPS"
JOBS="$(sysctl -n hw.ncpu)"
# The default target of compat/, src/ and lib/ builds only that directory's
# library plus the headers it generates (vlc_about.h, fourcc_tables.h,
# libvlc_version.h); asking for the .la file alone would skip them. modules/ is
# built by name: its default target compiles every module that configure found
# a Homebrew library for, none of which Nebulae uses.
run_logged vlc-compat make -j"$JOBS" -C compat
run_logged vlc-libvlccore make -j"$JOBS" -C src
run_logged vlc-libvlc make -j"$JOBS" -C lib
run_logged vlc-plugins make -j"$JOBS" -C modules \
    libvideotoolbox_plugin.la libcaopengllayer_plugin.la libglconv_cvpx_plugin.la
# libintl (needed by the module's translated strings) is not on the default
# link path; the override replaces the configure-time LDFLAGS, so repeat the
# header padding install_name_tool needs when the library is relocated.
run_logged vlc-bluray-plugin make -C modules liblibbluray_plugin.la \
    LDFLAGS="-Wl,-headerpad_max_install_names -L$GETTEXT_PREFIX/lib" LIBS="-lintl"
cd "$REPO_ROOT"

# --- 3. Checks on what was built ------------------------------------------------

step "Checking the build outputs"
ARTIFACTS=(
    "$VLC_BUILD/lib/.libs/libvlc.5.dylib"
    "$VLC_BUILD/modules/.libs/libvideotoolbox_plugin.dylib"
    "$VLC_BUILD/modules/.libs/libcaopengllayer_plugin.dylib"
    "$VLC_BUILD/modules/.libs/libglconv_cvpx_plugin.dylib"
    "$VLC_BUILD/modules/.libs/liblibbluray_plugin.dylib"
)
for f in "${ARTIFACTS[@]}"; do
    [ -f "$f" ] || fail "missing build output: $f"
    [ "$(archs_of "$f")" = "$TARGET_ARCH" ] || fail "$f is '$(archs_of "$f")', expected $TARGET_ARCH"
    fail_on_build_paths "$f"
    m="$(minos_of "$f")"
    if [ -n "$m" ] && version_gt "$m" "$MIN_MACOS"; then
        fail "$f needs macOS $m, newer than the declared minimum $MIN_MACOS"
    fi
    echo "    $(basename "$f"): $TARGET_ARCH, minimum macOS ${m:-unknown}"
done
BLURAY_PLUGIN="$VLC_BUILD/modules/.libs/liblibbluray_plugin.dylib"
PLUGIN_LIBS="$(otool -L "$BLURAY_PLUGIN")"
grep -qF "$LB_DYLIB" <<<"$PLUGIN_LIBS" \
    || fail "the Blu-ray plugin is not linked to the libbluray built here ($LB_DYLIB):
$PLUGIN_LIBS"
if grep -qE "/libbluray/[0-9.]+/|/opt/libbluray/" <<<"$PLUGIN_LIBS"; then
    fail "the Blu-ray plugin is linked to Homebrew's libbluray as well:
$PLUGIN_LIBS"
fi
echo "    the Blu-ray plugin links $LB_DYLIB"

# --- 4. Build record --------------------------------------------------------------

python3 - "$WORK/build-info.json" <<PY
import json, subprocess, sys

def out(*cmd):
    return subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False).stdout.decode().strip().splitlines()[0]

info = {
    "target_arch": "$TARGET_ARCH",
    "deployment_target": "$MIN_MACOS",
    "vlc": {"version": "$VLC_VERSION", "url": "$VLC_URL", "sha256": "$VLC_SHA256"},
    "libbluray": {"version": "$LIBBLURAY_VERSION", "url": "$LIBBLURAY_URL", "sha256": "$LIBBLURAY_SHA256"},
    "ant": {"version": "$ANT_VERSION", "url": "$ANT_URL", "sha512": "$ANT_SHA512", "shipped": False},
    "tools": {
        "clang": out("clang", "--version"),
        "meson": out("meson", "--version"),
        "ninja": out("ninja", "--version"),
        "javac": out("$JAVA_HOME/bin/javac", "-version"),
    },
}
with open(sys.argv[1], "w", encoding="utf-8") as fh:
    json.dump(info, fh, indent=2)
    fh.write("\n")
PY

echo
echo "=== VLC runtime build finished ==="
echo "  libbluray prefix : $LB_PREFIX"
echo "  VLC build tree   : $VLC_BUILD"
echo "  build record     : $WORK/build-info.json"
echo "  next             : scripts/stage-vlc-runtime.sh"
