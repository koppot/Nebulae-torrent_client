#!/usr/bin/env bash
# bundle-vlc-macos.sh
#
# Stage a VLC runtime into src-tauri/resources/vlc/
# for LOCAL developer builds on macOS.
# Supports both prebuilt VLC 3.x DMGs and VLC 4.x built from source.
#
# Usage (run from repo root):
#   ./scripts/bundle-vlc-macos.sh [arm64|intel64] [mode]
#   mode: auto (default), download, build
#
# Requirements: curl, hdiutil, meson, ninja (for build mode)
#               git (for build mode)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VLC_VERSION="4.0.0"
DEST="$REPO_ROOT/src-tauri/resources/vlc"
TMP="${TMPDIR:-/tmp}/nebulae-vlc-stage"
BUILD_MODE="${2:-auto}"

# Architecture selection
ARCH="${1:-}"
if [ -z "$ARCH" ]; then
    case "$(uname -m)" in
        arm64) ARCH="arm64" ;;
        *)     ARCH="intel64" ;;
    esac
fi

echo "Staging VLC $VLC_VERSION ($ARCH) -> $DEST"
echo "Build mode: $BUILD_MODE"

# Check if we have a prebuilt DMG available (VLC 4.x may not have DMGs yet)
if [ "$BUILD_MODE" = "download" ] || { [ "$BUILD_MODE" = "auto" ] && [ -f "$TMP/vlc-$VLC_VERSION-$ARCH.dmg" ]; }; then
    echo "Downloading/preparing VLC $VLC_VERSION DMG..."
    VLC_URL="https://download.videolan.org/pub/videolan/vlc/$VLC_VERSION/macosx/vlc-$VLC_VERSION-$ARCH.dmg"
    DMG_PATH="$TMP/vlc-$VLC_VERSION-$ARCH.dmg"
    MOUNT_POINT="/tmp/nebulae-vlcmnt"

    mkdir -p "$TMP"

    # Download (cache locally for iterative builds)
    if [ ! -f "$DMG_PATH" ]; then
        echo "Downloading $VLC_URL ..."
        curl -fL --progress-bar -o "$DMG_PATH" "$VLC_URL" || {
            echo "Download failed, falling back to build mode"
            BUILD_MODE="build"
        }
    fi

    if [ "$BUILD_MODE" != "build" ]; then
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
        echo "VLC staged from DMG:"
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
        exit 0
    fi
fi

# Build VLC from source if DMG not available or build mode selected
if [ "$BUILD_MODE" = "build" ] || { [ "$BUILD_MODE" = "auto" ] && [ ! -f "$TMP/vlc-$VLC_VERSION-$ARCH.dmg" ]; }; then
    echo "Building VLC $VLC_VERSION from source..."
    
    SOURCE_DIR="/tmp/vlc4-source"
    BUILD_DIR="$TMP/vlc4-build-$ARCH"
    INSTALL_DIR="$TMP/cand-vlc4"
    
    mkdir -p "$BUILD_DIR" "$INSTALL_DIR"
    
    # Clone if not present
    if [ ! -d "$SOURCE_DIR" ]; then
        echo "Cloning VLC source from GitHub..."
        git clone https://github.com/videolan/vlc.git "$SOURCE_DIR" 2>&1 | tail -5
    fi
    
    cd "$SOURCE_DIR"
    git pull origin master 2>&1 | tail -3
    echo "VLC source updated to: $(git rev-parse HEAD)"
    
    # Configure VLC 4 with macOS-specific options
    echo "Configuring VLC 4..."
    meson setup "$BUILD_DIR" \
        --prefix="$INSTALL_DIR" \
        --buildtype=release \
        -Dprefix="$INSTALL_DIR" \
        -Dlibdir=lib \
        -Denable-vlc=true \
        -Dplugins=true \
        -Dcontribs=disabled \
        -Dlua=disabled \
        -Dqt=disabled \
        -Dpython=disabled \
        -Dtest=disabled \
        -Ddoc=disabled \
        2>&1 | tail -30
    
    # Build only the components we need
    echo "Building libvlc, vlccore, and caopengllayer..."
    meson compile -C "$BUILD_DIR" libvlc vlccore caopengllayer 2>&1 | tail -20
    
    # Install to candidate directory
    echo "Installing to $INSTALL_DIR..."
    meson install -C "$BUILD_DIR" 2>&1 | tail -10
    
    # Copy to destination
    echo "Copying built VLC to $DEST..."
    mkdir -p "$DEST/lib" "$DEST/plugins"
    cp -R "$INSTALL_DIR/lib/"*.dylib "$DEST/lib/" 2>/dev/null || echo "No dylibs in install prefix"
    cp -R "$INSTALL_DIR/lib/vlc/plugins/"* "$DEST/plugins/" 2>/dev/null || echo "No plugins in install prefix"
    
    # Check what we got
    echo ""
    echo "VLC staged from build:"
    ls -la "$DEST/lib"
    ls "$DEST/plugins" | head -20
    
    PLUGIN_COUNT=$(find "$DEST/plugins" -type f | wc -l | tr -d ' ')
    echo "Plugins staged: $PLUGIN_COUNT files"
    echo ""
    echo "VLC 4 build candidate at: $INSTALL_DIR"
    echo "Done. Build with: pnpm tauri build"
fi
