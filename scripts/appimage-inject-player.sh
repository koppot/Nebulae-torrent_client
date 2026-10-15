#!/usr/bin/env bash
# scripts/appimage-inject-player.sh
# Injects the self-contained disc player (mpv + library tree + JRE) into a
# freshly built Nebulae AppImage WITHOUT disturbing the AppImage runtime.
#
# Why post-build injection: tauri-bundler runs linuxdeploy over the AppDir,
# and linuxdeploy auto-scans AppDir/usr/lib recursively. The bundled player
# library tree and JRE placed under usr/lib/nebulae/resources make
# linuxdeploy fail. So the AppImage is built WITHOUT the player resources
# (deb/rpm keep them as regular resources), and this script adds the player
# to the squashfs payload afterwards while preserving the original runtime
# prefix (dd-extracted via --appimage-offset).
#
# Runtime probe path (src-tauri/src/commands/media.rs):
#   $APPDIR/usr/share/nebulae/resources/player/mpv

set -euo pipefail

STAGING="${STAGING:-src-tauri/player-staging}"

if [ ! -d "$STAGING" ]; then
    echo "ERROR: player staging directory not found: $STAGING"
    echo "This script expects the player to have been moved out of"
    echo "src-tauri/resources before the AppImage build."
    exit 1
fi
if [ ! -f "$STAGING/mpv" ]; then
    echo "ERROR: staging has no top-level mpv binary (stale or nested staging):"
    find "$STAGING" -maxdepth 2 | head -n 20
    exit 1
fi

APPIMAGE="$(find src-tauri/target -name '*.AppImage' -type f | head -n 1)"
if [ -z "$APPIMAGE" ]; then
    echo "ERROR: no AppImage found under src-tauri/target"
    exit 1
fi
echo "AppImage: $APPIMAGE"
echo "Player staging: $STAGING"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# 1. Preserve the original AppImage runtime prefix.
OFFSET="$("$APPIMAGE" --appimage-offset)"
# count=1 copies the FIRST block (the runtime ELF); skip=1 would copy the
# payload instead and corrupt the repacked image.
dd if="$APPIMAGE" of="$WORK/runtime" bs="$OFFSET" count=1 status=none
head -c 4 "$WORK/runtime" | grep -q $'\x7fELF' || {
    echo "ERROR: runtime extraction failed (no ELF magic at offset 0)"
    exit 1
}

# 2. Extract the payload, add the player, and repack with the same runtime.
rm -rf squashfs-root
"$APPIMAGE" --appimage-extract > /dev/null
mkdir -p squashfs-root/usr/share/nebulae/resources
# Remove any stale destination first so `cp -a SRC/.` cannot nest the tree
# one level deeper (a pre-existing destination dir would reproduce it).
rm -rf squashfs-root/usr/share/nebulae/resources/player
mkdir -p squashfs-root/usr/share/nebulae/resources/player
cp -a "$STAGING"/. squashfs-root/usr/share/nebulae/resources/player/
rm -rf squashfs-root/usr/lib/nebulae/resources/player
# Fail fast, with a clear message, if the bundled binary is not where the
# runtime probe expects it (staging layout problem, not an AppImage problem).
if [ ! -f squashfs-root/usr/share/nebulae/resources/player/mpv ]; then
    echo "ERROR: mpv binary not produced at resources/player/mpv by staging:"
    find "$STAGING" -maxdepth 2 | head -n 20
    exit 1
fi
mksquashfs squashfs-root "$WORK/payload.squashfs" -root-owned -noappend
cat "$WORK/runtime" "$WORK/payload.squashfs" > "$APPIMAGE.new"
chmod +x "$APPIMAGE.new"
head -c 4 "$APPIMAGE.new" | grep -q $'\x7fELF' || {
    echo "ERROR: repacked AppImage has no ELF magic at offset 0"
    exit 1
}
mv "$APPIMAGE.new" "$APPIMAGE"
rm -rf squashfs-root

# 3. Verify the injection: the player must be present inside the payload.
"$APPIMAGE" --appimage-offset > /dev/null
# squashfs-tools >= 4.6 writes the -l listing to stderr, 4.4/4.5 to stdout;
# capture both so the check works on either runner image generation.
LISTING="$(unsquashfs -l "$WORK/payload.squashfs" 2>&1 || true)"
printf '%s\n' "$LISTING" | grep -q "usr/share/nebulae/resources/player/mpv" || {
    echo "ERROR: player missing from repacked payload; payload resources tree:"
    printf '%s\n' "$LISTING" | grep -E "resources" | head -n 20 || true
    exit 1
}
echo "SUCCESS: player injected into $APPIMAGE (runtime preserved)"
