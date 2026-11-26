#!/bin/sh
# Nebulae macOS build entry script (task order 2026-11-26 §6.1).
#
# The ONLY inputs are an approved commit SHA and the expected version.
# It refuses a dirty tree or a different HEAD, checks the tools, runs the
# compile gates, then hands off to the exact packaging steps the release
# workflow (.github/workflows/release.yml, macOS matrix) uses.
#
# Usage (on the macOS build machine):
#   ./scripts/build-entry-macos.sh --commit <SHA> --version 0.2.9 \
#       --target aarch64-apple-darwin|x86_64-apple-darwin
#
# Factory wrote this script and built nothing with it. The builder records
# the build (installer name, size, SHA-256, gate results, model+effort) to
# the Task 5 report page.
set -eu

COMMIT=""; VERSION=""; TARGET="aarch64-apple-darwin"
while [ $# -gt 0 ]; do
    case "$1" in
        --commit) COMMIT="$2"; shift 2 ;;
        --version) VERSION="$2"; shift 2 ;;
        --target) TARGET="$2"; shift 2 ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done
[ -n "$COMMIT" ] || { echo "--commit is required" >&2; exit 2; }
[ -n "$VERSION" ] || { echo "--version is required" >&2; exit 2; }
case "$TARGET" in
    aarch64-apple-darwin|x86_64-apple-darwin) ;;
    *) echo "bad --target: $TARGET" >&2; exit 2 ;;
esac

# --- 1. Commit gate -------------------------------------------------------
[ -d .git ] || { echo "run from the repository root" >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "REFUSED: dirty worktree" >&2; exit 1; }
HEAD_SHA=$(git rev-parse HEAD)
[ "$HEAD_SHA" = "$COMMIT" ] || { echo "REFUSED: HEAD $HEAD_SHA != approved $COMMIT" >&2; exit 1; }
echo "==> Commit gate passed: HEAD = $HEAD_SHA, worktree clean."

# --- 2. Tool checks -------------------------------------------------------
for tool in git node pnpm rustc cargo rustup python3; do
    command -v "$tool" >/dev/null 2>&1 || { echo "REFUSED: missing tool: $tool" >&2; exit 1; }
done
echo "node $(node --version), pnpm $(pnpm --version), $(rustc --version), python3 $(python3 --version)"

# --- 3. Free disk space (need >= 30 GB for target/) -----------------------
AVAIL=$(df -g . | awk 'NR==2{print $4}')
[ "$AVAIL" -ge 30 ] || { echo "REFUSED: only ${AVAIL}GB free (need >= 30GB)" >&2; exit 1; }

# --- 4. Version gate ------------------------------------------------------
CONF_VERSION=$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")
[ "$CONF_VERSION" = "$VERSION" ] || { echo "REFUSED: tauri.conf.json version $CONF_VERSION != expected $VERSION" >&2; exit 1; }
echo "==> Version gate passed: $CONF_VERSION."

# --- 5. Compile gates (must pass BEFORE packaging) ------------------------
(cd src-tauri && cargo check && cargo clippy)
pnpm exec tsc --noEmit
echo "==> Compile gates passed (cargo check, cargo clippy, tsc --noEmit)."

# --- 6. Packaging (the release.yml macOS steps, in order) -----------------
pnpm install --frozen-lockfile
./scripts/bundle-macos-transmission.sh
./scripts/bundle-macos-vlc.sh "$TARGET"
pnpm tauri build --target "$TARGET" --bundles app,dmg

APP_DIR=$(find "src-tauri/target/$TARGET" -name "Nebulae.app" -type d | head -n 1)
[ -n "$APP_DIR" ] || { echo "FAIL: Nebulae.app was not produced" >&2; exit 1; }
./scripts/verify-macos-app.sh "$APP_DIR"

DMG=$(find "src-tauri/target/$TARGET" -name "*.dmg" -type f | head -n 1)
[ -n "$DMG" ] || { echo "FAIL: dmg was not produced" >&2; exit 1; }
SIZE=$(stat -f%z "$DMG")
SHA=$(shasum -a 256 "$DMG" | awk '{print $1}')
echo "==> BUILD RECORD"
echo "installer: $DMG"
echo "size: $SIZE bytes"
echo "sha256: $SHA"
echo "commit: $HEAD_SHA  target: $TARGET  version: $VERSION"
echo "Record installer name, size, SHA-256, gate results and duration on the Task 5 report page."
