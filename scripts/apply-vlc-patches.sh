#!/usr/bin/env bash
# scripts/apply-vlc-patches.sh SOURCE_DIR
#
# Applies the Nebulae patches in patches/vlc-3.0.23/ to a pristine, unpacked
# VLC 3.0.23 source tree, in the one order they are written for:
#
#   vt-305, navigate-menu, hdr-tonemap, bluray-playlist-id
#
# bluray-playlist-id.patch builds on navigate-menu.patch (it edits the same
# header, symbol list and media_player.c), so the order is part of the contract.
#
# The patches are applied with no fuzz, because a fuzzy match can land a hunk in
# the wrong place and still report success (that is how navigate-menu.patch once
# put its header declaration above the typedef it needs). After the last patch
# every patched file is compared with patches/vlc-3.0.23/patched-sources.sha256,
# so an application that is not byte-for-byte the reviewed tree fails here.
#
# Any failure stops the build. Nothing falls back to an unpatched VLC.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PATCH_DIR="$SCRIPT_DIR/../patches/vlc-3.0.23"
PATCHES=(vt-305 navigate-menu hdr-tonemap bluray-playlist-id)

fail() { echo "ERROR: $*" >&2; exit 1; }

[ $# -eq 1 ] || fail "usage: $0 SOURCE_DIR"
SRC="$1"
[ -f "$SRC/configure" ] && [ -f "$SRC/lib/media_player.c" ] || fail "$SRC is not an unpacked VLC source tree"
[ -f "$PATCH_DIR/patched-sources.sha256" ] || fail "missing $PATCH_DIR/patched-sources.sha256"

# A tree that was patched before would make every hunk fail or, worse, apply twice.
if grep -q "libvlc_media_player_navigate_menu" "$SRC/lib/libvlc.sym"; then
    fail "$SRC already contains the Nebulae patches; unpack a fresh VLC 3.0.23 tree"
fi

PATCH_BIN="${PATCH:-/usr/bin/patch}"
[ -x "$PATCH_BIN" ] || PATCH_BIN="$(command -v patch)" || fail "patch(1) not found"

for name in "${PATCHES[@]}"; do
    file="$PATCH_DIR/$name.patch"
    [ -f "$file" ] || fail "missing patch $file"
    echo "==> $name.patch"
    (cd "$SRC" && "$PATCH_BIN" -p1 -F0 --batch --dry-run < "$file" >/dev/null) \
        || fail "$name.patch does not apply cleanly (zero fuzz) to $SRC"
    (cd "$SRC" && "$PATCH_BIN" -p1 -F0 --batch < "$file") \
        || fail "$name.patch failed to apply to $SRC"
done

stray="$(find "$SRC" \( -name '*.rej' -o -name '*.orig' \) -print | head -5)"
[ -z "$stray" ] || fail "patch left reject/backup files: $stray"

echo "==> Verifying the patched sources"
(cd "$SRC" && shasum -a 256 -c "$PATCH_DIR/patched-sources.sha256") \
    || fail "patched sources differ from patches/vlc-3.0.23/patched-sources.sha256"

echo "OK: ${#PATCHES[@]} patches applied and verified in $SRC"
