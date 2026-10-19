#!/bin/sh
# Set the custom-volume-icon flag on a built DMG. Tauri's bundler writes
# .VolumeIcon.icns into the image but the kHasCustomIcon Finder flag on the
# volume root is what makes Finder/Quick Look actually use it - without it
# the generic "disk image" icon is shown. (Not a permissions issue: the
# icns is world-readable.)
set -e
DMG="$1"
[ -n "$DMG" ] || { echo "usage: $0 <dmg>"; exit 1; }
MP=$(mktemp -d)
hdiutil attach -nobrowse -readwrite -mountpoint "$MP" "$DMG" >/dev/null
# Set the custom-icon bit on the volume root.
SetFile -a C "$MP"
hdiutil detach "$MP" -quiet
echo "custom icon flag set on $DMG"
