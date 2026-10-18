#!/usr/bin/env bash
# bundle-vlc-linux.sh
#
# Stage the system-installed VLC runtime into src-tauri/resources/vlc/
# for LOCAL developer builds on Linux. The release workflow (release.yml)
# does the same steps inline after installing libvlc5.
#
# Usage (run from repo root):
#   ./scripts/bundle-vlc-linux.sh
#
# Requirements:
#   libvlc5 and vlc-plugin-base installed:
#     sudo apt-get install -y libvlc5 vlc-plugin-base vlc-plugin-video-output libbluray2

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$REPO_ROOT/src-tauri/resources/vlc"

# Detect lib directory (x86_64 vs aarch64)
if   [ -d /usr/lib/x86_64-linux-gnu ]; then
    LIBDIR=/usr/lib/x86_64-linux-gnu
elif [ -d /usr/lib/aarch64-linux-gnu ]; then
    LIBDIR=/usr/lib/aarch64-linux-gnu
elif [ -d /usr/lib/arm-linux-gnueabihf ]; then
    LIBDIR=/usr/lib/arm-linux-gnueabihf
else
    LIBDIR=/usr/lib
fi

echo "Staging VLC from $LIBDIR -> $DEST"

# Verify libvlc is installed
if ! ldconfig -p | grep -q libvlc.so; then
    echo "ERROR: libvlc not found. Install with:"
    echo "  sudo apt-get install -y libvlc5 vlc-plugin-base vlc-plugin-video-output"
    exit 1
fi

mkdir -p "$DEST/lib" "$DEST/plugins"

# Copy VLC shared libraries
echo "Copying libvlc*.so* ..."
cp -rL "$LIBDIR"/libvlc*.so* "$DEST/lib/" 2>/dev/null || {
    # Fallback: locate via ldconfig
    ldconfig -p | awk '/libvlc\.so|libvlccore\.so/ {print $NF}' | while read -r lib; do
        cp -L "$lib" "$DEST/lib/" 2>/dev/null || true
    done
}

# Copy plugins
PLUGIN_SRC=""
for candidate in \
    "$LIBDIR/vlc/plugins" \
    /usr/lib/vlc/plugins \
    /usr/lib64/vlc/plugins; do
    if [ -d "$candidate" ]; then
        PLUGIN_SRC="$candidate"
        break
    fi
done

if [ -n "$PLUGIN_SRC" ]; then
    echo "Copying plugins from $PLUGIN_SRC ..."
    cp -rL "$PLUGIN_SRC/." "$DEST/plugins/" 2>/dev/null || true
else
    echo "WARNING: VLC plugin directory not found — playback may be limited"
fi

# Also copy libbluray if available (disc menu support)
for lib in "$LIBDIR"/libbluray.so*; do
    [ -e "$lib" ] && cp -L "$lib" "$DEST/lib/" 2>/dev/null && echo "Copied $(basename "$lib")" || true
done

chmod -R u+rwX,go+rX "$DEST"

echo ""
echo "VLC staged:"
ls "$DEST"

# Verify required libraries
for req in libvlc.so.5 libvlccore.so.9; do
    if ls "$DEST/lib/$req" 2>/dev/null | grep -q .; then
        echo "OK: $req"
    else
        # Accept any .so with the right prefix (version numbers vary)
        if ls "$DEST/lib/$(echo "$req" | cut -d. -f1)".* 2>/dev/null | grep -q .; then
            echo "OK: $(ls "$DEST/lib/$(echo "$req" | cut -d. -f1)".* | head -1 | xargs basename)"
        else
            echo "WARNING: $req not found in staged dir — check system VLC install"
        fi
    fi
done

PLUGIN_COUNT=$(find "$DEST/plugins" -type f 2>/dev/null | wc -l | tr -d ' ')
echo "Plugins staged: $PLUGIN_COUNT files"
echo ""
echo "Done. Build with: pnpm tauri build"
