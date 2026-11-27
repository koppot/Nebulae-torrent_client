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

COMMIT=""; VERSION=""; BASELINE="${NEBULAE_BASELINE_DEB:-}"
while [ $# -gt 0 ]; do
    case "$1" in
        --commit) COMMIT="$2"; shift 2 ;;
        --version) VERSION="$2"; shift 2 ;;
        --baseline) BASELINE="$2"; shift 2 ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done
[ -n "$COMMIT" ] && [ -n "$VERSION" ] || { echo "--commit and --version are required" >&2; exit 2; }

[ -d .git ] || { echo "run from the repository root" >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "REFUSED: dirty worktree" >&2; exit 1; }
HEAD_SHA=$(git rev-parse HEAD)
[ "$HEAD_SHA" = "$COMMIT" ] || { echo "REFUSED: HEAD $HEAD_SHA != approved $COMMIT" >&2; exit 1; }
echo "==> Commit gate passed: HEAD = $HEAD_SHA, worktree clean."

# Geo assets (locked input; fetched and hash-verified before the compile gates).
./scripts/fetch-geo-assets.sh
./scripts/check-geo-assets.sh

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

# Clean-tree re-check after staging (staging must never modify tracked files).
[ -z "$(git status --porcelain)" ] || { echo "REFUSED: the worktree is dirty after staging:" >&2; git status --porcelain >&2; exit 1; }

pnpm tauri build --bundles deb,rpm

# The built binary must report a CLEAN source (build.rs stamps -clean/-dirty).
BUILD_ID_IN_BIN=$(strings -a src-tauri/target/release/nebulae 2>/dev/null | grep -m1 -o '[0-9a-f]\{40\}-\(clean\|dirty\)' || true)
case "$BUILD_ID_IN_BIN" in
    *-clean) echo "==> Built provenance: $BUILD_ID_IN_BIN" ;;
    *) echo "FAIL: the built Linux binary reports build_id '$BUILD_ID_IN_BIN' (expected <sha>-clean); not delivered" >&2; exit 1 ;;
esac
[ -z "$(git status --porcelain)" ] || { echo "REFUSED: the worktree is dirty after the build:" >&2; git status --porcelain >&2; exit 1; }

for f in src-tauri/target/release/bundle/deb/*.deb src-tauri/target/release/bundle/rpm/*.rpm; do
    [ -f "$f" ] || continue
    echo "==> artifact: $f  size $(stat -c%s "$f")  sha256 $(sha256sum "$f" | awk '{print $1}')"
done

# Baseline-diff gate (completeness rules item 1). --baseline <accepted.deb>
# (or NEBULAE_BASELINE_DEB): the deb payload is extracted with dpkg-deb into
# a temp dir and compared against the new deb payload by relative path; every
# difference must be classified in scripts/baseline-allow/linux.tsv.
BASELINE_STATUS="NOT RUN (no baseline passed)"
if [ -n "$BASELINE" ]; then
    [ -f "$BASELINE" ] || { echo "FAIL: baseline deb not found: $BASELINE" >&2; exit 1; }
    command -v dpkg-deb >/dev/null 2>&1 || { echo "FAIL: dpkg-deb not available" >&2; exit 1; }
    NEW_DEB=$(ls src-tauri/target/release/bundle/deb/*.deb | head -n 1)
    TMPB=$(mktemp -d /tmp/nebulae-baseline.XXXXXX)
    dpkg-deb -x "$BASELINE" "$TMPB/baseline"
    dpkg-deb -x "$NEW_DEB" "$TMPB/candidate"
    if ./scripts/baseline_diff.py \
        --baseline "$TMPB/baseline" \
        --candidate "$TMPB/candidate" \
        --allow scripts/baseline-allow/linux.tsv \
        --report "$(dirname "$NEW_DEB")/baseline-diff.md"; then
        BASELINE_STATUS="PASS vs $(basename "$BASELINE")"
    else
        echo "FAIL: baseline-diff gate — differences not classified in scripts/baseline-allow/linux.tsv (report: $(dirname "$NEW_DEB")/baseline-diff.md)" >&2
        rm -rf "$TMPB"
        exit 1
    fi
    rm -rf "$TMPB"
fi
echo "==> baseline-diff gate: $BASELINE_STATUS"
echo "Record artifact names, sizes, SHA-256s, gate results and duration on the Task 5 report page."
