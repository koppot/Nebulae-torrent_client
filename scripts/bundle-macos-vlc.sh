#!/usr/bin/env bash
# scripts/bundle-macos-vlc.sh TARGET
#
# Stages the VLC runtime of the macOS app into src-tauri/resources/vlc. It is the
# one entry point that release.yml, candidate-build.yml and build-test.yml call,
# so the three cannot ship different runtimes again.
#
# TARGET aarch64-apple-darwin  (needs an arm64 Mac)
#   The Nebulae runtime:
#     1. downloads the official VLC 3.0.23 arm64 disk image; its SHA-256 is
#        pinned below, the VideoLAN checksum file is not trusted at run time;
#     2. scripts/build-vlc-runtime.sh builds libbluray 1.5.0 with its BD-J jars
#        and VLC 3.0.23 with all four patches in patches/vlc-3.0.23, including
#        bluray-playlist-id.patch. apply-vlc-patches.sh stops the build when a
#        patch does not apply or the patched sources differ from the recorded
#        hashes; no step falls back to an unpatched component;
#     3. scripts/stage-vlc-runtime.sh stages the result, relocates its
#        dependencies into the bundle and runs scripts/verify-vlc-runtime.sh.
#
# TARGET x86_64-apple-darwin
#   The unmodified official Intel VLC 3.0.23 (SHA-256 pinned), as the workflows
#   have always shipped for this target. The patched runtime links Homebrew
#   libraries that exist only for the architecture of the build machine, and the
#   x86_64 leg runs on an arm64 runner, so Nebulae's patches (menu navigation,
#   Blu-ray playlist id, HDR tone mapping, VT-305) are NOT in this runtime. The
#   script says so in the log and as a workflow warning.
#
# Environment:
#   NEBULAE_VLC_WORK   work and download directory, shared with
#                      build-vlc-runtime.sh (default src-tauri/vlc-runtime-build)
#   NEBULAE_VLC_DEST   x86_64 only: where to stage (default
#                      src-tauri/resources/vlc; the directory is emptied first
#                      and must be named "vlc")
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORK="${NEBULAE_VLC_WORK:-$REPO_ROOT/src-tauri/vlc-runtime-build}"
export NEBULAE_VLC_WORK="$WORK"

VLC_VERSION="3.0.23"
# Published by VideoLAN next to the images:
#   https://download.videolan.org/pub/videolan/vlc/3.0.23/macosx/vlc-3.0.23-<arch>.dmg.sha256
SHA256_ARM64="fc6fac08d87f538517d44aca0c5e7a244b67c8c4cb589bf478363a7315fd5e0d"
SHA256_INTEL64="ec01530ce69d849dd057fba8876e68ac39bf279dc28de4e9c04e4aec11fc98db"

fail() { echo "ERROR: $*" >&2; exit 1; }

TARGET="${1:-}"
case "$TARGET" in
    aarch64-apple-darwin) DMG_ARCH="arm64";   DMG_SHA256="$SHA256_ARM64" ;;
    x86_64-apple-darwin)  DMG_ARCH="intel64"; DMG_SHA256="$SHA256_INTEL64" ;;
    *) fail "usage: bundle-macos-vlc.sh aarch64-apple-darwin|x86_64-apple-darwin" ;;
esac
[ "$(uname -s)" = "Darwin" ] || fail "bundle-macos-vlc.sh stages the macOS runtime and only runs on macOS"

DMG_URL="https://download.videolan.org/pub/videolan/vlc/${VLC_VERSION}/macosx/vlc-${VLC_VERSION}-${DMG_ARCH}.dmg"
DOWNLOADS="$WORK/downloads"
DMG="$DOWNLOADS/vlc-${VLC_VERSION}-${DMG_ARCH}.dmg"
mkdir -p "$DOWNLOADS"

echo "=== VLC ${VLC_VERSION} runtime for ${TARGET} ==="

got=""
[ -f "$DMG" ] && got="$(shasum -a 256 "$DMG" | awk '{print $1}')"
if [ "$got" = "$DMG_SHA256" ]; then
    echo "    cached and verified: $(basename "$DMG")"
else
    [ -z "$got" ] || { echo "    cached $(basename "$DMG") does not match the pinned SHA-256; downloading again"; rm -f "$DMG"; }
    echo "    downloading $DMG_URL"
    curl -fL --retry 3 --retry-delay 2 -sS -o "$DMG.part" "$DMG_URL" || { rm -f "$DMG.part"; fail "download failed: $DMG_URL"; }
    got="$(shasum -a 256 "$DMG.part" | awk '{print $1}')"
    if [ "$got" != "$DMG_SHA256" ]; then
        rm -f "$DMG.part"
        fail "SHA-256 mismatch for $DMG_URL
  expected $DMG_SHA256
  actual   $got"
    fi
    mv "$DMG.part" "$DMG"
    echo "    SHA-256 verified: $(basename "$DMG")"
fi

MNT="$(mktemp -d "${TMPDIR:-/tmp}/nebulae-vlc-dmg.XXXXXX")"
cleanup() {
    hdiutil detach "$MNT" -quiet >/dev/null 2>&1 || true
    rmdir "$MNT" 2>/dev/null || true
}
trap cleanup EXIT
hdiutil attach "$DMG" -mountpoint "$MNT" -nobrowse -readonly -noautoopen -quiet \
    || fail "could not mount $DMG"
OFFICIAL_APP="$MNT/VLC.app"
[ -d "$OFFICIAL_APP/Contents/MacOS/lib" ] && [ -d "$OFFICIAL_APP/Contents/MacOS/plugins" ] \
    || fail "$DMG does not contain VLC.app with lib/ and plugins/"

case "$TARGET" in
    aarch64-apple-darwin)
        NEBULAE_TARGET_ARCH=arm64 "$SCRIPT_DIR/build-vlc-runtime.sh"
        VLC_OFFICIAL_APP="$OFFICIAL_APP" "$SCRIPT_DIR/stage-vlc-runtime.sh"
        ;;
    x86_64-apple-darwin)
        DEST="${NEBULAE_VLC_DEST:-$REPO_ROOT/src-tauri/resources/vlc}"
        [ "$(basename "$DEST")" = "vlc" ] || fail "NEBULAE_VLC_DEST must name a directory called vlc, not $DEST"
        echo "WARNING: the x86_64 macOS runtime is the unmodified official VLC ${VLC_VERSION}."
        echo "         Nebulae's VLC patches are not part of it; see the header of this script."
        echo "::warning title=macOS x86_64 VLC runtime is unpatched::The x86_64 macOS build bundles the unmodified official VLC ${VLC_VERSION}. The Nebulae patches (menu navigation, Blu-ray playlist id, HDR tone mapping, VT-305) are only built for arm64."
        rm -rf "$DEST"
        mkdir -p "$DEST"
        cp -R "$OFFICIAL_APP/Contents/MacOS/lib" "$DEST/"
        cp -R "$OFFICIAL_APP/Contents/MacOS/plugins" "$DEST/"
        cp -R "$OFFICIAL_APP/Contents/Frameworks" "$DEST/" 2>/dev/null || true
        # VLC validates plugins.dat by mtime and size; bundling rewrites mtimes.
        rm -f "$DEST/plugins/plugins.dat"
        find "$DEST" -name .DS_Store -type f -delete
        archs="$(lipo -archs "$DEST/lib/libvlccore.9.dylib")"
        [ "$archs" = "x86_64" ] || fail "staged libvlccore has architectures '$archs', expected x86_64"
        echo "    staged the official Intel runtime in $DEST"
        # Informational: this runtime is VideoLAN's own, so findings here are
        # VideoLAN's (for example the Sparkle and Growl frameworks), not a
        # reason to stop; the patched arm64 runtime is held to the strict audit.
        python3 -B "$SCRIPT_DIR/audit-macos-bundle.py" --arch x86_64 "$DEST" || true
        ;;
esac
