#!/usr/bin/env bash
# scripts/bundle-jre.sh <windows-x64|linux-x64|mac-aarch64|mac-x64>
#
# Stages the PINNED Temurin JRE (BD-J/UHD disc menus; libbluray honours
# JAVA_HOME) into src-tauri/resources/jre. Replaces the old "latest Temurin 21"
# download in the workflows: the exact build, size and SHA-256 are frozen in
# scripts/jre-pins.json (21.0.13+9, the JRE 0.2.7 shipped). A download that
# does not match is deleted and the script fails; nothing falls back.
#
# Environment:
#   NEBULAE_JRE_CACHE   download cache (default: a temp dir removed on exit).
#                       A cached file is reused only when it matches the pin.
set -euo pipefail

KEY="${1:?usage: bundle-jre.sh <windows-x64|linux-x64|mac-aarch64|mac-x64>}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PINS="$SCRIPT_DIR/jre-pins.json"
DEST="$REPO_ROOT/src-tauri/resources/jre"

fail() { echo "ERROR: $*" >&2; exit 1; }
sha256_of() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'; else shasum -a 256 "$1" | awk '{print $1}'; fi; }
size_of() { wc -c < "$1" | tr -d ' '; }

read -r FILE BYTES SHA TAG VERSION < <(python3 - "$PINS" "$KEY" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
e = d["inputs"].get(sys.argv[2])
if e is None:
    sys.exit("unknown JRE key %r (known: %s)" % (sys.argv[2], ", ".join(sorted(d["inputs"]))))
print(e["file"], e["bytes"], e["sha256"], d["release_tag"], d["version"])
PY
)
URL="https://github.com/adoptium/temurin21-binaries/releases/download/$TAG/$FILE"

if [ -n "${NEBULAE_JRE_CACHE:-}" ]; then
    CACHE="$NEBULAE_JRE_CACHE"; mkdir -p "$CACHE"; CLEAN=""
else
    CACHE="$(mktemp -d "${TMPDIR:-/tmp}/nebulae-jre.XXXXXX")"; CLEAN="$CACHE"
fi
ARCHIVE="$CACHE/$FILE"

matches() { [ -f "$1" ] && [ "$(size_of "$1")" = "$BYTES" ] && [ "$(sha256_of "$1")" = "$SHA" ]; }
if ! matches "$ARCHIVE"; then
    rm -f "$ARCHIVE"
    echo "==> Downloading pinned JRE $VERSION ($KEY): $URL"
    curl -fL --retry 3 -o "$ARCHIVE" "$URL" || { rm -f "$ARCHIVE"; fail "download failed: $URL"; }
    matches "$ARCHIVE" || { got="$(sha256_of "$ARCHIVE")"; rm -f "$ARCHIVE"; fail "pin mismatch for $FILE: got sha256 $got, expected $SHA (size expected $BYTES); file deleted"; }
fi
echo "==> JRE archive verified: $FILE $BYTES bytes sha256 $SHA"

EXTRACT="$(mktemp -d "${TMPDIR:-/tmp}/nebulae-jre-x.XXXXXX")"
trap 'rm -rf "$EXTRACT" $CLEAN' EXIT
case "$FILE" in
    *.zip)    unzip -q "$ARCHIVE" -d "$EXTRACT" || tar -xf "$ARCHIVE" -C "$EXTRACT" ;;
    *.tar.gz) tar xzf "$ARCHIVE" -C "$EXTRACT" ;;
    *) fail "unsupported archive type: $FILE" ;;
esac
# macOS tarballs nest the runtime in Contents/Home; the others have one top directory with bin/.
HOME_DIR="$(find "$EXTRACT" -type d -name Home -path '*Contents*' | head -1 || true)"
if [ -z "$HOME_DIR" ]; then
    BIN_DIR="$(find "$EXTRACT" -maxdepth 3 -type d -name bin | head -1)"
    [ -n "$BIN_DIR" ] || fail "no bin/ directory in $FILE"
    HOME_DIR="$(dirname "$BIN_DIR")"
fi

rm -rf "$DEST"; mkdir -p "$DEST"
cp -a "$HOME_DIR"/. "$DEST"/
chmod -R u+rwX,go+rX "$DEST" || true
JAVA="$DEST/bin/java"; [ "$KEY" = "windows-x64" ] && JAVA="$DEST/bin/java.exe"
[ -e "$JAVA" ] || fail "JRE staging failed: $JAVA missing"
grep -q "IMPLEMENTOR_VERSION=\"Temurin-${VERSION}" "$DEST/release" \
    || fail "staged JRE is not Temurin-$VERSION (see $DEST/release)"
echo "==> Staged Temurin $VERSION into $DEST ($(find "$DEST" -type f | wc -l | tr -d ' ') files)"
