#!/usr/bin/env bash
# scripts/fetch-geo-assets.sh [--geo-dir DIR]
#
# Fetch the locked GeoLite2 databases (scripts/geo-assets.lock.json) from the
# private repository koppot/nebulae-build-assets and stage them into
# src-tauri/resources/geo. Read-only on this repository: the .mmdb files are
# gitignored (MaxMind terms) so the tree stays clean.
#
# Sources, in order:
#   1. NEBULAE_GEO_ASSETS_DIR — a local clone of the private repo. The pinned
#      commit must be present and the files must match the lock (they are
#      verified, not trusted).
#   2. A shallow fetch from GitHub using the CALLER'S existing credentials
#      (git credential helper). A token may be supplied via GEO_ASSETS_TOKEN
#      (e.g. CI); it is passed to git through a credential helper and is never
#      printed, logged, or written to disk. The temp clone is deleted.
#
# After staging: verifies size + SHA-256 again, then confirms the worktree is
# still clean (the geo dir is gitignored; any dirt is a bug and fails here).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
GEO_DIR="$REPO_ROOT/src-tauri/resources/geo"
if [ "${1:-}" = "--geo-dir" ] && [ -n "${2:-}" ]; then GEO_DIR="$2"; shift 2; fi
[ "$#" -eq 0 ] || { echo "usage: fetch-geo-assets.sh [--geo-dir DIR]" >&2; exit 2; }

LOCK="$SCRIPT_DIR/geo-assets.lock.json"
REPO=$(python3 -c "import json;print(json.load(open('$LOCK'))['repo'])")
COMMIT=$(python3 -c "import json;print(json.load(open('$LOCK'))['commit'])")
mkdir -p "$GEO_DIR"

stage_and_verify() {  # $1 = source dir containing geo/
    local src="$1"
    python3 - "$src" "$GEO_DIR" "$LOCK" <<'EOF'
import hashlib, json, os, shutil, sys
src, geo_dir, lock_path = sys.argv[1], sys.argv[2], sys.argv[3]
lock = json.load(open(lock_path))
for f in lock["files"]:
    s = os.path.join(src, f["path"])
    if not os.path.isfile(s):
        sys.exit(f"source is missing {f['path']}")
    h = hashlib.sha256()
    with open(s, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    if h.hexdigest() != f["sha256"] or os.path.getsize(s) != f["size"]:
        sys.exit(f"{f['path']} does not match the lock (size/hash)")
    shutil.copyfile(s, os.path.join(geo_dir, f["dest"]))
    print(f"staged {f['dest']} ({f['size']} bytes)")
EOF
}

if [ -n "${NEBULAE_GEO_ASSETS_DIR:-}" ]; then
    echo "==> Using verified local geo assets from $NEBULAE_GEO_ASSETS_DIR"
    [ -d "$NEBULAE_GEO_ASSETS_DIR/.git" ] || { echo "REFUSED: NEBULAE_GEO_ASSETS_DIR is not a git clone" >&2; exit 1; }
    ACTUAL=$(git -C "$NEBULAE_GEO_ASSETS_DIR" rev-parse HEAD)
    [ "$ACTUAL" = "$COMMIT" ] || { echo "REFUSED: local clone is at $ACTUAL, lock pins $COMMIT" >&2; exit 1; }
    stage_and_verify "$NEBULAE_GEO_ASSETS_DIR"
else
    if ! command -v git >/dev/null 2>&1; then echo "REFUSED: git not found" >&2; exit 1; fi
    TMP=$(mktemp -d "${TMPDIR:-/tmp}/nebulae-geo-assets.XXXXXX")
    trap 'rm -rf "$TMP"' EXIT
    echo "==> Shallow-fetching $REPO at $COMMIT (temp clone, deleted afterwards)"
    if [ -n "${GEO_ASSETS_TOKEN:-}" ]; then
        # Token goes to git via a credential helper only. The helper string
        # is passed SINGLE-QUOTED: argv carries the literal name
        # $GEO_ASSETS_TOKEN, and the helper's sh expands it from the
        # inherited environment at call time. The token value is therefore
        # never part of a command line (invisible in process lists), never
        # printed, and the temp clone is deleted below.
        git -C "$TMP" init -q
        git -C "$TMP" remote add origin "https://github.com/$REPO.git"
        GIT_TERMINAL_PROMPT=0 git -C "$TMP" \
            -c 'credential.helper=!f() { echo "username=x-access-token"; echo "password=$GEO_ASSETS_TOKEN"; }; f' \
            fetch --depth 1 origin "$COMMIT" -q
        git -C "$TMP" checkout -q FETCH_HEAD
        SRC_DIR="$TMP"
    else
        GIT_TERMINAL_PROMPT=0 git clone -q --depth 1 "https://github.com/$REPO.git" "$TMP/repo" 2>/dev/null \
            || { echo "REFUSED: could not clone $REPO (no stored credentials). Provide GEO_ASSETS_TOKEN or NEBULAE_GEO_ASSETS_DIR." >&2; exit 1; }
        if [ "$(git -C "$TMP/repo" rev-parse HEAD)" != "$COMMIT" ]; then
            GIT_TERMINAL_PROMPT=0 git -C "$TMP/repo" fetch --depth 1 origin "$COMMIT" -q
            git -C "$TMP/repo" checkout -q FETCH_HEAD
        fi
        SRC_DIR="$TMP/repo"
    fi
    [ "$(git -C "$SRC_DIR" rev-parse HEAD)" = "$COMMIT" ] \
        || { echo "REFUSED: fetched HEAD is not the pinned commit $COMMIT" >&2; exit 1; }
    stage_and_verify "$SRC_DIR"
    rm -rf "$TMP"; trap - EXIT
fi

"$SCRIPT_DIR/check-geo-assets.sh" --dir "$GEO_DIR"

if [ -n "$(git -C "$REPO_ROOT" status --porcelain)" ]; then
    echo "REFUSED: the worktree is dirty after geo staging (this is a bug; the geo dir is gitignored):" >&2
    git -C "$REPO_ROOT" status --porcelain >&2
    exit 1
fi
echo "==> Geo assets staged and verified; worktree still clean."
