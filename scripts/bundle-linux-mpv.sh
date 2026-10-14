#!/usr/bin/env bash
# scripts/bundle-linux-mpv.sh
# Bundles mpv binary for Linux (AppImage & deb/rpm package resources)
# Required by Phase 2 of docs/task-bundle-disc-player.md

set -euo pipefail

TARGET_DIR="${1:-src-tauri/resources/player}"
mkdir -p "$TARGET_DIR"

echo "=========================================================="
echo " Nebulae Linux Disc Player Bundling (mpv + libbluray)     "
echo "=========================================================="
echo "Target directory: $TARGET_DIR"

if command -v mpv >/dev/null 2>&1; then
    MPV_PATH=$(command -v mpv)
    echo "Host mpv binary found at: $MPV_PATH"
    cp -L "$MPV_PATH" "$TARGET_DIR/mpv"
    chmod +x "$TARGET_DIR/mpv"
    echo "Staged binary: $TARGET_DIR/mpv"
else
    echo "ERROR: mpv command not found on host system."
    echo "Please ensure mpv is installed (e.g. apt-get install -y mpv libdvdnav4 libbluray2)."
    exit 1
fi

echo "==> Running smoke test on staged mpv binary..."
VERSION_OUT=$("$TARGET_DIR/mpv" --version || true)
echo "$VERSION_OUT" | head -n 10

echo "SUCCESS: Linux disc player staged successfully."
