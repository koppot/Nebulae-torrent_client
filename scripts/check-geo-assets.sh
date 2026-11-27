#!/usr/bin/env bash
# scripts/check-geo-assets.sh [--dir DIR]
#
# GEO GATE: verify that the GeoLite2 databases in DIR (default
# src-tauri/resources/geo) match scripts/geo-assets.lock.json exactly
# (presence, byte size, SHA-256). Exit 0 only when both files pass.
# Used after fetching, after staging, and before packaging on every platform.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
GEO_DIR="$REPO_ROOT/src-tauri/resources/geo"
if [ "$1" = "--dir" ] && [ -n "${2:-}" ]; then GEO_DIR="$2"; shift 2; fi
[ -d "$GEO_DIR" ] || { echo "REFUSED (geo): directory $GEO_DIR does not exist" >&2; exit 1; }

python3 - "$GEO_DIR" "$SCRIPT_DIR/geo-assets.lock.json" <<'EOF'
import hashlib, json, os, sys

geo_dir, lock_path = sys.argv[1], sys.argv[2]
lock = json.load(open(lock_path))
failed = 0
for f in lock["files"]:
    dest = os.path.join(geo_dir, f["dest"])
    if not os.path.isfile(dest):
        print(f"REFUSED (geo): missing {f['dest']} in {geo_dir}", file=sys.stderr)
        failed = 1
        continue
    size = os.path.getsize(dest)
    if size != f["size"]:
        print(f"REFUSED (geo): {f['dest']} is {size} bytes, lock says {f['size']}", file=sys.stderr)
        failed = 1
        continue
    h = hashlib.sha256()
    with open(dest, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    if h.hexdigest() != f["sha256"]:
        print(f"REFUSED (geo): {f['dest']} SHA-256 mismatch (got {h.hexdigest()}, lock {f['sha256']})", file=sys.stderr)
        failed = 1
        continue
    print(f"==> geo OK: {f['dest']} ({size} bytes, sha256 {f['sha256'][:12]}...)")
sys.exit(failed)
EOF
