#!/bin/sh
# Nebulae Linux build entry script (task order 2026-11-26 §6.1).
#
# The ONLY inputs are an approved commit SHA and the expected version.
# Gates: clean tree at the approved HEAD, tools present, version match,
# compile gates — then the release.yml deb+rpm packaging.
#
# The AppImage is NOT built by this script: it needs the CI-only staging
# shuffle and squashfs injection (release.yml "Build Linux AppImage..." and
# "Inject bundled player + engine into AppImage"). Build it from CI or port
# those steps here as a separate approved change.
#
# Usage: ./scripts/build-entry-linux.sh --commit <SHA> --version 0.2.9
set -eu

COMMIT=""; VERSION=""
while [ $# -gt 0 ]; do
    case "$1" in
        --commit) COMMIT="$2"; shift 2 ;;
        --version) VERSION="$2"; shift 2 ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done
[ -n "$COMMIT" ] && [ -n "$VERSION" ] || { echo "--commit and --version are required" >&2; exit 2; }

[ -d .git ] || { echo "run from the repository root" >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "REFUSED: dirty worktree" >&2; exit 1; }
HEAD_SHA=$(git rev-parse HEAD)
[ "$HEAD_SHA" = "$COMMIT" ] || { echo "REFUSED: HEAD $HEAD_SHA != approved $COMMIT" >&2; exit 1; }
echo "==> Commit gate passed: HEAD = $HEAD_SHA, worktree clean."

for tool in git node pnpm rustc cargo python3; do
    command -v "$tool" >/dev/null 2>&1 || { echo "REFUSED: missing tool: $tool" >&2; exit 1; }
done
echo "node $(node --version), pnpm $(pnpm --version), $(rustc --version)"

AVAIL_GB=$(df -BG . | awk 'NR==2{gsub("G","",$4); print $4}')
[ "$AVAIL_GB" -ge 30 ] || { echo "REFUSED: only ${AVAIL_GB}G free (need >= 30G)" >&2; exit 1; }

CONF_VERSION=$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")
[ "$CONF_VERSION" = "$VERSION" ] || { echo "REFUSED: tauri.conf.json version $CONF_VERSION != expected $VERSION" >&2; exit 1; }
echo "==> Version gate passed: $CONF_VERSION."

(cd src-tauri && cargo check && cargo clippy)
pnpm exec tsc --noEmit
echo "==> Compile gates passed (cargo check, cargo clippy, tsc --noEmit)."

pnpm install --frozen-lockfile
./scripts/bundle-linux-transmission.sh
pnpm tauri build --bundles deb,rpm

for f in src-tauri/target/release/bundle/deb/*.deb src-tauri/target/release/bundle/rpm/*.rpm; do
    [ -f "$f" ] || continue
    echo "==> artifact: $f  size $(stat -c%s "$f")  sha256 $(sha256sum "$f" | awk '{print $1}')"
done
echo "Record artifact names, sizes, SHA-256s, gate results and duration on the Task 5 report page."
