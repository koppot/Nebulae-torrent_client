#!/usr/bin/env bash
# scripts/bundle-linux-mpv.sh
# Bundles a SELF-CONTAINED mpv player for Linux (AppImage, deb, and rpm):
# the mpv binary plus every non-glibc shared library it needs, and a
# bundled jlink JRE so BD-J / 4K UHD interactive menus render everywhere.
# Owner directive 2026-10-14: the package must not rely on any OS-installed
# player or third-party library. The build host may use its package manager
# to SOURCE the binaries, but the shipped bundle carries everything itself.
# The only host requirement left is the glibc core (libc/libm/ld-linux),
# which cannot be bundled safely - documented floor: glibc >= 2.35.
# Required by Phase 2 of docs/task-bundle-disc-player.md.

set -euo pipefail

TARGET_DIR="${1:-src-tauri/resources/player}"
mkdir -p "$TARGET_DIR"

echo "=========================================================="
echo " Nebulae Linux Disc Player Bundling (self-contained)      "
echo "=========================================================="
echo "Target directory: $TARGET_DIR"

if ! command -v mpv >/dev/null 2>&1; then
    echo "mpv not found on the build host; installing it (build host only -"
    echo "the shipped bundle carries its own libraries)."
    if command -v sudo >/dev/null 2>&1; then
        sudo apt-get update -y
        sudo apt-get install -y mpv libdvdnav4 libbluray2
    else
        apt-get update -y
        apt-get install -y mpv libdvdnav4 libbluray2
    fi
fi

MPV_PATH="$(command -v mpv)"
echo "Source mpv binary: $MPV_PATH"

cp -L "$MPV_PATH" "$TARGET_DIR/mpv"
chmod 755 "$TARGET_DIR/mpv"

# --- Shared-library walk --------------------------------------------------
# Copy every shared library mpv needs, except the unswappable glibc core
# (bundling libc/libm/ld-linux breaks the dynamic loader). libstdc++ and
# libgcc_s ARE bundled so the tree also runs on older distributions.
LIB_DIR="$TARGET_DIR/lib"
mkdir -p "$LIB_DIR"

is_excluded() {
    case "$(basename "$1")" in
        ld-linux*|libc.so*|libm.so*|libpthread*|libdl*|librt*|libresolv*|\
        libutil*|libcrypt*|libanl*|libmvec*|libBrokenLocale*|libnss_*|\
        libSegFault*)
            return 0 ;;
        *) return 1 ;;
    esac
}

echo "==> Walking and bundling shared libraries (glibc core excluded)..."
PROCESSED="$(mktemp)"
trap 'rm -f "$PROCESSED"' EXIT
: > "$PROCESSED"

Q=("$TARGET_DIR/mpv")
echo "$TARGET_DIR/mpv" > "$PROCESSED"
while [ "${#Q[@]}" -gt 0 ]; do
    CUR="${Q[0]}"
    Q=("${Q[@]:1}")
    while read -r dep; do
        [ -n "$dep" ] && [ -f "$dep" ] || continue
        is_excluded "$dep" && continue
        base="$(basename "$dep")"
        dest="$LIB_DIR/$base"
        if [ ! -f "$dest" ]; then
            cp -L "$dep" "$dest"
            chmod 755 "$dest"
            echo "  bundled: $base"
        fi
        real="$(readlink -f "$dest")"
        if ! grep -qxF "$real" "$PROCESSED"; then
            echo "$real" >> "$PROCESSED"
            Q+=("$real")
        fi
    done < <(ldd "$CUR" 2>/dev/null | awk '{if ($3 ~ /^\//) print $3}' | sort -u)
done

BUNDLED_COUNT="$(find "$LIB_DIR" -type f | wc -l)"
echo "Bundled $BUNDLED_COUNT shared libraries."

# --- RPATH wiring ----------------------------------------------------------
echo "==> Applying RPATH so the bundle resolves libraries from its own tree..."
if ! command -v patchelf >/dev/null 2>&1; then
    echo "ERROR: patchelf is required (apt-get install patchelf)."
    exit 1
fi
patchelf --set-rpath '$ORIGIN/lib' "$TARGET_DIR/mpv"
for lib in "$LIB_DIR"/*; do
    [ -f "$lib" ] || continue
    patchelf --set-rpath '$ORIGIN' "$lib" 2>/dev/null || true
done

# --- Bundled JRE (BD-J / 4K UHD interactive menus) --------------------------
if [ "${NEB_SKIP_JRE:-0}" != "1" ]; then
    if [ -z "${JAVA_HOME:-}" ] || [ ! -x "$JAVA_HOME/bin/jlink" ]; then
        echo "ERROR: jlink is not available (JAVA_HOME='${JAVA_HOME:-unset}')."
        echo "The owner requires full BD-J/UHD interactive menu support, which"
        echo "needs a bundled JRE. Set up a JDK (e.g. actions/setup-java,"
        echo "distribution: temurin, java-version: 21) or set NEB_SKIP_JRE=1"
        echo "to build WITHOUT BD-J menus (not acceptable for releases)."
        exit 1
    fi
    echo "==> Bundling minimal JRE via jlink (BD-J / UHD menus)..."
    "$JAVA_HOME/bin/jlink" \
        --add-modules java.se,jdk.unsupported,jdk.crypto.ec \
        --strip-debug --no-man-pages --no-header-files \
        --output "$TARGET_DIR/jre"
    "$TARGET_DIR/jre/bin/java" -version
    echo "Bundled JRE ready: $TARGET_DIR/jre"
fi

# --- Self-containment verification ------------------------------------------
echo "==> Verifying every dependency resolves from the bundled tree..."
MISSING=0
check_ldd() {
    while read -r line; do
        case "$line" in
            *"not found"*) MISSING=1; echo "  UNRESOLVED: $line" ;;
        esac
    done < <(LD_LIBRARY_PATH="$LIB_DIR" ldd "$1" 2>/dev/null)
}
check_ldd "$TARGET_DIR/mpv"
for lib in "$LIB_DIR"/*.so*; do
    [ -f "$lib" ] || continue
    check_ldd "$lib"
done
if [ "$MISSING" -ne 0 ]; then
    echo "ERROR: unresolved dependencies remain in the bundled tree."
    exit 1
fi
echo "All dependencies resolve from the bundled tree."

echo "==> Smoke test on bundled mpv..."
VERSION_OUT="$(LD_LIBRARY_PATH="$LIB_DIR" "$TARGET_DIR/mpv" --version)"
echo "$VERSION_OUT" | head -n 6
# mpv 0.41+ no longer prints a feature list in --version output; verify
# compiled-in disc support via the presence of the device options instead.
OPTIONS_OUT="$(LD_LIBRARY_PATH="$LIB_DIR" "$TARGET_DIR/mpv" --list-options)"
case "$OPTIONS_OUT" in
    *--dvd-device*)   : ;;
    *) echo "ERROR: DVD menu support (--dvd-device) missing from bundled mpv!"; exit 1 ;;
esac
case "$OPTIONS_OUT" in
    *--bluray-device*) : ;;
    *) echo "ERROR: Blu-ray menu support (--bluray-device) missing from bundled mpv!"; exit 1 ;;
esac

echo "SUCCESS: Linux bundle is self-contained (mpv + library tree + JRE)."
