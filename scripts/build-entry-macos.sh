#!/usr/bin/env bash
# Nebulae macOS build entry script (round 4: matches .github/workflows/release.yml).
#
# The ONLY inputs are an approved commit SHA, the expected version and the target.
# It refuses a dirty tree or a different HEAD, checks the tools (it installs
# nothing), runs the compile gates, then runs the exact macOS packaging steps of
# release.yml in the same order:
#   pnpm install --frozen-lockfile  ->  gates (cargo check/clippy, tsc)  ->
#   bundle-macos-transmission.sh  ->  bundle-macos-vlc.sh <target>  ->
#   bundle-jre.sh (PINNED Temurin 21.0.13+9, size + SHA-256 in scripts/jre-pins.json;
#   release.yml still downloads "latest Temurin 21")  ->  bundle-tailscale.sh <target>
#   ->  delete .DS_Store  ->  pnpm tauri build --target <target>  ->
#   verify-macos-app.sh  ->  package-macos-dmg.sh (+ app zip)  ->  read-only dmg mount check.
#
# Usage (on the macOS build machine, from the repository root):
#   ./scripts/build-entry-macos.sh --commit <FULL SHA> --version 0.2.9 \
#       --target aarch64-apple-darwin|x86_64-apple-darwin [--out DIR]
#
# --verify-only: no rebuild. Runs only the commit gate, the version gate and the
# post-build steps (artifact checksums, read-only mount check of the existing
# dmg, baseline-diff gate, BUILD-RECORD.txt) on an existing --out directory, for
# replaying the end of a build on real artifacts. The record says VERIFY-ONLY.
#
# Outputs go to --out (default: ../nebulae-mac-out next to the checkout), never
# into the repository. Nothing is pushed, tagged, released or installed.
#
# Updater artifacts: tauri.conf.json enables createUpdaterArtifacts, which needs
# TAURI_SIGNING_PRIVATE_KEY. If it is not set the build is an UNSIGNED TEST
# CANDIDATE: the script passes --config '{"bundle":{"createUpdaterArtifacts":false}}'
# (tauri.conf.json stays unmodified). It never creates or imports a key.
set -euo pipefail

COMMIT=""; VERSION=""; TARGET="aarch64-apple-darwin"; OUT=""; SIGN_IDENTITY="${APPLE_SIGNING_IDENTITY:-}"
BASELINE="${NEBULAE_BASELINE_DMG:-}"
VERIFY_ONLY=0
while [ $# -gt 0 ]; do
    case "$1" in
        --commit) COMMIT="$2"; shift 2 ;;
        --version) VERSION="$2"; shift 2 ;;
        --target) TARGET="$2"; shift 2 ;;
        --out) OUT="$2"; shift 2 ;;
        --sign-identity) SIGN_IDENTITY="$2"; shift 2 ;;
        --baseline) BASELINE="$2"; shift 2 ;;
        --verify-only) VERIFY_ONLY=1; shift ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done
[ -n "$COMMIT" ] || { echo "--commit is required" >&2; exit 2; }
[ -n "$VERSION" ] || { echo "--version is required" >&2; exit 2; }
case "$TARGET" in
    aarch64-apple-darwin) DMG_NAME="Nebulae-macOS-arm64.dmg"; JRE_KEY="mac-aarch64" ;;
    x86_64-apple-darwin)  DMG_NAME="Nebulae-macOS-x64.dmg";   JRE_KEY="mac-x64" ;;
    *) echo "bad --target: $TARGET" >&2; exit 2 ;;
esac

# --- 1. Commit gate -------------------------------------------------------
[ -d .git ] || { echo "run from the repository root" >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "REFUSED: dirty worktree" >&2; exit 1; }
HEAD_SHA=$(git rev-parse HEAD)
[ "$HEAD_SHA" = "$COMMIT" ] || { echo "REFUSED: HEAD $HEAD_SHA != approved $COMMIT" >&2; exit 1; }
echo "==> Commit gate passed: HEAD = $HEAD_SHA, worktree clean."
[ -n "$OUT" ] || OUT="$(dirname "$PWD")/nebulae-mac-out"
case "$OUT" in "$PWD"|"$PWD"/*) echo "REFUSED: --out must be outside the repository" >&2; exit 1 ;; esac

# --verify-only skips everything from here to the end of step 7 (geo, tools,
# compile, staging, tauri build, signing, packaging). The block is not
# re-indented to keep the diff of the build path reviewable.
if [ "$VERIFY_ONLY" = 1 ]; then
    [ -f "$OUT/$DMG_NAME" ] || { echo "REFUSED: --verify-only needs an existing $OUT/$DMG_NAME" >&2; exit 1; }
    for f in "$DMG_NAME" "$DMG_NAME-app.zip"; do
        [ -f "$OUT/$f" ] && [ -f "$OUT/$f.sha256" ] || { echo "REFUSED: --verify-only needs $OUT/$f and $OUT/$f.sha256" >&2; exit 1; }
        (cd "$OUT" && shasum -a 256 -c "$f.sha256") || { echo "FAIL: checksum of $OUT/$f does not match its .sha256" >&2; exit 1; }
    done
    CONF_VERSION=$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")
    [ "$CONF_VERSION" = "$VERSION" ] || { echo "REFUSED: tauri.conf.json version $CONF_VERSION != expected $VERSION" >&2; exit 1; }
    echo "==> VERIFY-ONLY: no rebuild; checking the existing artifacts in $OUT"
fi
if [ "$VERIFY_ONLY" != 1 ]; then
# --- 1b. Geo assets (locked input; never committed, MaxMind terms) ---------
# Fetched and hash-verified after the commit gate and BEFORE the compile
# gates; the GEO gate refuses missing or mismatched files.
./scripts/fetch-geo-assets.sh
./scripts/check-geo-assets.sh

# --- 2. Tool checks (nothing is installed) --------------------------------
for tool in git node pnpm rustc cargo rustup python3 go brew curl tar ditto hdiutil shasum; do
    command -v "$tool" >/dev/null 2>&1 || { echo "REFUSED: missing tool: $tool" >&2; exit 1; }
done
# The packages release.yml installs with brew before bundling. Report every
# missing one; installing them is a user-approved step, never done here.
MISSING=""
for f in cmake meson ninja pkgconf bison autoconf automake libtool libevent libdeflate libpsl miniupnpc gettext libudfread fontconfig freetype; do
    brew list --versions "$f" >/dev/null 2>&1 || MISSING="$MISSING $f"
done
[ -z "$MISSING" ] || { echo "REFUSED: missing Homebrew packages:$MISSING (release.yml installs them; installing needs user approval)" >&2; exit 1; }
GO_MINOR=$(go version | sed -E 's/.*go1\.([0-9]+).*/\1/')
[ "$GO_MINOR" -ge 24 ] || { echo "REFUSED: go 1.24 or newer required for the tailscale build ($(go version))" >&2; exit 1; }
rustup target list --installed | grep -qx "$TARGET" || { echo "REFUSED: rust target $TARGET is not installed (rustup target add needs user approval)" >&2; exit 1; }
if [ "$TARGET" = "aarch64-apple-darwin" ] && [ "$(uname -m)" != "arm64" ]; then
    echo "REFUSED: the patched arm64 VLC runtime needs an arm64 Mac" >&2; exit 1
fi
echo "node $(node --version), pnpm $(pnpm --version), $(rustc --version), $(python3 --version), $(go version)"

# --- 3. Free disk space (need >= 30 GB for target/) -----------------------
AVAIL=$(df -g . | awk 'NR==2{print $4}')
[ "$AVAIL" -ge 30 ] || { echo "REFUSED: only ${AVAIL}GB free (need >= 30GB)" >&2; exit 1; }

# --- 4. Version gate ------------------------------------------------------
CONF_VERSION=$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")
[ "$CONF_VERSION" = "$VERSION" ] || { echo "REFUSED: tauri.conf.json version $CONF_VERSION != expected $VERSION" >&2; exit 1; }
echo "==> Version gate passed: $CONF_VERSION."

# --- 5. Dependencies, then compile gates ----------------------------------
# release.yml installs the frontend dependencies before anything else; tsc
# needs node_modules, so this must come BEFORE the gates.
pnpm install --frozen-lockfile
# generate_context! needs the gitignored frontendDist folder even for cargo check
mkdir -p dist
(cd src-tauri && cargo check && cargo clippy)
pnpm exec tsc --noEmit
echo "==> Compile gates passed (cargo check, cargo clippy, tsc --noEmit)."

# --- 6. Packaging (the release.yml macOS steps, in order) -----------------
./scripts/bundle-macos-transmission.sh
./scripts/bundle-macos-vlc.sh "$TARGET"
./scripts/bundle-jre.sh "$JRE_KEY"
./scripts/bundle-tailscale.sh "$TARGET"
# A .DS_Store in src-tauri/resources would be copied into the app by the
# "resources/**/*" glob; verify-macos-app.sh proves none is left.
find src-tauri/resources -name .DS_Store -type f -print -delete

# Rehearsal hook: stops after staging, before any installer is built. The real
# flow is unchanged when it is unset.
if [ "${NEBULAE_STAGING_ONLY:-}" = "1" ]; then
    echo "==> NEBULAE_STAGING_ONLY=1: staging complete, stopping before 'tauri build' (no installer built)."
    exit 0
fi

# --- 6b. Clean-tree re-check after staging (the 2026-11-27 -dirty incident) --
# Staging writes only into gitignored paths; a tracked file modified here is a
# staging-script bug and would make the binary report "...-dirty".
if [ -n "$(git status --porcelain)" ]; then
    echo "REFUSED: the worktree is dirty after staging (staging must never modify tracked files):" >&2
    git status --porcelain >&2
    exit 1
fi

BUILD_ARGS=(--target "$TARGET")
if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && [ -z "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]; then
    echo "==> NOTE: no Tauri signing key in the environment: UNSIGNED TEST CANDIDATE (updater artifacts off, tauri.conf.json unmodified)."
    BUILD_ARGS+=(--config '{"bundle":{"createUpdaterArtifacts":false}}')
fi
pnpm tauri build "${BUILD_ARGS[@]}"

APP_DIR=$(find "src-tauri/target/$TARGET" -name "Nebulae.app" -type d | head -n 1)
[ -n "$APP_DIR" ] || { echo "FAIL: Nebulae.app was not produced" >&2; exit 1; }

# --- 6c. The built app must report a CLEAN source (Part 2 gate) --------------
# build.rs stamps NEBULAE_BUILD_ID="<sha>-clean|dirty" from git status at
# compile time. A -dirty binary is never delivered; it is quarantined.
# Search for the EXPECTED full "<commit>-clean" id first: a generic first
# match could be a stale string embedded by a dependency, which would make
# the provenance decision on the wrong commit. The generic pattern is only
# used to report what IS embedded when the expected id is absent.
BUILD_ID_IN_BIN=$(strings -a "$APP_DIR/Contents/MacOS/nebulae" | grep -m1 -o "$HEAD_SHA-\(clean\|dirty\)" || true)
if [ -z "$BUILD_ID_IN_BIN" ]; then
    BUILD_ID_IN_BIN=$(strings -a "$APP_DIR/Contents/MacOS/nebulae" | grep -m1 -o '[0-9a-f]\{40\}-\(clean\|dirty\)' || true)
fi
if [ "$BUILD_ID_IN_BIN" = "$HEAD_SHA-clean" ]; then
    echo "==> Built provenance: $BUILD_ID_IN_BIN"
elif [ "$BUILD_ID_IN_BIN" = "$HEAD_SHA-dirty" ]; then
    QUAR="$OUT/quarantine-$(date +%Y%m%d-%H%M%S)"
    mkdir -p "$QUAR"
    mv "$APP_DIR" "$QUAR/" 2>/dev/null || true
    echo "FAIL: the built app reports build_id '$BUILD_ID_IN_BIN' (-dirty). Quarantined in $QUAR." >&2
    echo "      A dirty build is never delivered. Fix the staging scripts and rebuild." >&2
    exit 1
else
    echo "FAIL: embedded build_id '$BUILD_ID_IN_BIN' does not match the approved commit $HEAD_SHA (expected $HEAD_SHA-clean)" >&2
    exit 1
fi
# The worktree must still be clean after the build.
if [ -n "$(git status --porcelain)" ]; then
    echo "REFUSED: the worktree is dirty after the build:" >&2
    git status --porcelain >&2
    exit 1
fi
# release.yml: a failed verify stops the arm64 leg; the x86_64 leg runs on an
# arm64 machine with helpers built for it, so its findings are reported only.
if ! ./scripts/verify-macos-app.sh "$APP_DIR"; then
    if [ "$TARGET" = "aarch64-apple-darwin" ]; then echo "FAIL: verify-macos-app.sh" >&2; exit 1; fi
    echo "WARNING: verify-macos-app.sh reported findings on the x86_64 leg (reported, not fatal, as in release.yml)"
fi

# --- 6d. Signing: ad-hoc seal by default (release policy, 2026-11-27) --------
# PUBLIC RELEASES ARE AD-HOC SIGNED ("-" signingIdentity) and NOT notarized:
# no paid Developer ID certificate exists or is planned. Users approve the
# first launch once (Open Anyway, or control-click Open on macOS 14 and
# earlier — see READ ME FIRST.txt inside the dmg). The existing Apple
# Development identity (fingerprint E3A26A990308798F8DE4B7C49A3E27BFEF192623,
# team 2YJ32CEY87) may sign TEST builds only: pass --sign-identity
# <fingerprint> (or set APPLE_SIGNING_IDENTITY). Set NEBULAE_NO_SIGN=1 to
# skip sealing entirely for a staging-only rehearsal; the BUILD RECORD then
# says UNSIGNED TEST.
if [ -z "$SIGN_IDENTITY" ] && [ "${NEBULAE_NO_SIGN:-0}" != "1" ]; then
    SIGN_IDENTITY="-"
fi
if [ -n "$SIGN_IDENTITY" ]; then
    command -v codesign >/dev/null 2>&1 || { echo "REFUSED: codesign not found" >&2; exit 1; }
    ./scripts/sign-macos-app.sh "$APP_DIR" "$SIGN_IDENTITY"
    # Verify AFTER signing and BEFORE the dmg is packaged (twice: script
    # internal check above, and the strict check here).
    codesign --verify --deep --strict "$APP_DIR" || { echo "FAIL: codesign --verify --deep --strict after signing" >&2; exit 1; }
    echo "==> Signed with '$SIGN_IDENTITY' and verified (deep, strict)."
    if ! ./scripts/verify-macos-app.sh "$APP_DIR"; then
        if [ "$TARGET" = "aarch64-apple-darwin" ]; then echo "FAIL: verify-macos-app.sh after signing" >&2; exit 1; fi
        echo "WARNING: verify-macos-app.sh reported findings after signing on the x86_64 leg (reported, not fatal)"
    fi
fi

# --- 7. DMG + app zip (release.yml "Prepare and upload named artifact") ----
mkdir -p "$OUT"
TAURI_DMG=$(find "src-tauri/target/$TARGET" -name "*.dmg" -type f | head -n 1)
LAYOUT_ARGS=()
[ -z "$TAURI_DMG" ] || LAYOUT_ARGS+=(--layout-from "$TAURI_DMG")
./scripts/package-macos-dmg.sh --app "$APP_DIR" --output "$OUT/$DMG_NAME" \
    --icon "$(pwd)/src-tauri/icons/icon.icns" ${LAYOUT_ARGS[@]+"${LAYOUT_ARGS[@]}"}
[ -f "$OUT/$DMG_NAME" ] || { echo "FAIL: dmg was not produced" >&2; exit 1; }
(cd "$OUT" && shasum -a 256 "$DMG_NAME" > "$DMG_NAME.sha256")
ditto -c -k --sequesterRsrc --keepParent "$APP_DIR" "$OUT/$DMG_NAME-app.zip"
(cd "$OUT" && shasum -a 256 "$DMG_NAME-app.zip" > "$DMG_NAME-app.zip.sha256")

fi  # end of the build-only steps (1b to 7)

# --- 8. Read-only mount check of the final dmg (nothing is installed) -------
# hdiutil refuses a second attach of an image that is already mounted
# ("Resource busy"), and step 9 (baseline_diff.py) mounts the same dmg itself.
# So this mount MUST be detached before step 9; the EXIT trap only covers the
# failure paths in between.
MNT=$(mktemp -d "${TMPDIR:-/tmp}/nebulae-dmg-check.XXXXXX")
hdiutil attach -readonly -nobrowse -noverify -mountpoint "$MNT" "$OUT/$DMG_NAME" >/dev/null
trap 'hdiutil detach "$MNT" >/dev/null 2>&1 || true; rmdir "$MNT" 2>/dev/null || true' EXIT
MOUNTED_APP=$(find "$MNT" -maxdepth 2 -name "Nebulae.app" -type d | head -n 1)
[ -n "$MOUNTED_APP" ] || { echo "FAIL: no Nebulae.app inside the mounted dmg" >&2; exit 1; }
# The DELIVERED image must also carry the clean provenance.
MNT_BUILD_ID=$(strings -a "$MOUNTED_APP/Contents/MacOS/nebulae" | grep -m1 -o '[0-9a-f]\{40\}-\(clean\|dirty\)' || true)
case "$MNT_BUILD_ID" in
    *-clean) echo "==> Delivered app provenance: $MNT_BUILD_ID" ;;
    *) echo "FAIL: the app inside the final dmg reports build_id '$MNT_BUILD_ID' (expected the approved SHA with -clean)" >&2
       [ "$TARGET" != "aarch64-apple-darwin" ] || exit 1 ;;
esac
./scripts/verify-macos-app.sh "$MOUNTED_APP" || { [ "$TARGET" != "aarch64-apple-darwin" ] || { echo "FAIL: verify-macos-app.sh on the mounted dmg" >&2; exit 1; }; }
echo "==> Mounted dmg check passed (read-only; nothing installed)."
# Detach now (retry once, then force: Spotlight or a scanner can hold it for a
# moment). A mount that stays attached would make step 9 fail, so this is fatal.
if ! hdiutil detach "$MNT" >/dev/null 2>&1; then
    sleep 2
    hdiutil detach -force "$MNT" >/dev/null 2>&1 || { echo "FAIL: could not detach $MNT after the mounted dmg check (step 9 would find the image busy)" >&2; exit 1; }
fi
rmdir "$MNT" 2>/dev/null || true
trap - EXIT

# --- 9. Baseline-diff gate (completeness rules item 1) ----------------------
# Compares the delivered dmg against the accepted baseline dmg by RELATIVE
# path (hdiutil-mounted read-only by the script itself). Every difference
# must be classified in scripts/baseline-allow/macos.tsv. Pass --baseline
# <accepted.dmg> (or set NEBULAE_BASELINE_DMG); without a baseline the gate
# CANNOT run and the BUILD RECORD says so — the release checklist then keeps
# the candidate blocked until the comparison is run.
BASELINE_DIFF_STATUS="NOT RUN (no baseline passed)"
if [ -n "$BASELINE" ]; then
    [ -f "$BASELINE" ] || { echo "FAIL: baseline dmg not found: $BASELINE" >&2; exit 1; }
    # Run through the interpreter so a lost exec bit cannot break the gate.
    # baseline_diff.py exits 3 for "differences not classified" and 4 for
    # "could not run"; anything else (126/127, a Python crash) is also
    # "could not run" and must not be reported as a classification verdict.
    GATE_RC=0
    python3 scripts/baseline_diff.py \
        --baseline "$BASELINE" \
        --candidate "$OUT/$DMG_NAME" \
        --allow scripts/baseline-allow/macos.tsv \
        --report "$OUT/baseline-diff.md" || GATE_RC=$?
    case "$GATE_RC" in
        0) ;;
        3) echo "FAIL: baseline-diff gate — differences not classified in scripts/baseline-allow/macos.tsv (full report: $OUT/baseline-diff.md)" >&2; exit 1 ;;
        *) echo "FAIL: baseline-diff gate COULD NOT RUN (exit $GATE_RC; 126/127 = not executable or not found, 4 = cannot mount or read an input). This is not a classification result; see the messages above." >&2; exit 1 ;;
    esac
    BASELINE_DIFF_STATUS="PASS vs $(basename "$BASELINE") (report: $OUT/baseline-diff.md)"
fi

# A VERIFY-ONLY record must never replace the record of a real build.
RECORD_NAME="BUILD-RECORD.txt"
[ "$VERIFY_ONLY" != 1 ] || RECORD_NAME="BUILD-RECORD-VERIFY-ONLY.txt"
SIZE=$(stat -f%z "$OUT/$DMG_NAME")
SHA=$(shasum -a 256 "$OUT/$DMG_NAME" | awk '{print $1}')
{
echo "==> BUILD RECORD"
echo "installer: $OUT/$DMG_NAME"
echo "size: $SIZE bytes"
echo "sha256: $SHA"
echo "app zip: $OUT/$DMG_NAME-app.zip"
if [ "$VERIFY_ONLY" = 1 ]; then
echo "mode: VERIFY-ONLY (no rebuild; these artifacts were NOT produced by this run)"
echo "checked at commit: $HEAD_SHA  target: $TARGET  version: $VERSION"
echo "built from (id embedded in the delivered app): $MNT_BUILD_ID"
else
echo "commit: $HEAD_SHA  target: $TARGET  version: $VERSION"
fi
echo "jre: Temurin 21.0.13+9 (pinned, scripts/jre-pins.json key $JRE_KEY)"
echo "dmg root gate: Nebulae.app, Applications, READ ME FIRST.txt (hash verified), .background/background.png + background@2x.png (hash verified), .VolumeIcon.icns (package-macos-dmg.sh verified all of these on the final image)"
if [ "$VERIFY_ONLY" = 1 ]; then
echo "signing: not re-derived in VERIFY-ONLY mode (the signing step belongs to the build run)"
else
case "$SIGN_IDENTITY" in
"")
echo "signing: UNSIGNED TEST (NEBULAE_NO_SIGN=1 — staging rehearsal only, NOT a candidate)"
;;
"-")
echo "signing: ad-hoc seal (release policy: no Developer ID, not notarized; codesign --verify --deep --strict passed before packaging)"
;;
*)
echo "signing: TEST build, identity '$SIGN_IDENTITY' (codesign --verify --deep --strict passed before packaging)"
;;
esac
fi
echo "baseline-diff gate: $BASELINE_DIFF_STATUS"
} | tee "$OUT/$RECORD_NAME"
echo "Record installer name, size, SHA-256, gate results and duration on the Task 5 report page."
