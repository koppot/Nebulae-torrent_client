#!/usr/bin/env bash
# Bundle PINNED tailscale binaries into src-tauri/resources/engine/tailscale.
#
# Usage: bundle-tailscale.sh <target-triple>
#   x86_64-unknown-linux-gnu | aarch64-apple-darwin | x86_64-apple-darwin
#   | x86_64-pc-windows-msvc
#
# Rules:
#  - Checksums are PINNED (official *.sha256 from pkgs.tailscale.com).
#  - Private temp dirs (mktemp), cleaned on exit; no shared /tmp names.
#  - The smoke test runs ONLY a same-architecture binary (never executes a
#    foreign-arch build).
#  - macOS: no official daemon tarball exists; build from the pinned tag
#    with go (available on GH runners). Clone failures are fatal.
set -euo pipefail

TS_VERSION="1.104.1"
TARGET="${1:?usage: bundle-tailscale.sh <target-triple>}"
DEST="src-tauri/resources/engine/tailscale"

# Official pinned checksums (pkgs.tailscale.com/stable/<file>.sha256)
SHA_LINUX_AMD64="108d1d96ecf410d305571e173516f27038f870919a9b89c914a167c6d33a4528"
SHA_WIN_AMD64="bca8ce0c845cede64b6f61f636c013c6501ce9e6c3f5c42e013d8f703d672e86"  # tailscale-setup-1.104.1.exe.sha256

TMP="$(mktemp -d "${TMPDIR:-/tmp}/nebulae-ts-bundle.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$DEST"

version_of() { "$1" --version 2>/dev/null | head -1 || "$1" version 2>/dev/null | head -1 || true; }

# Require a matching native binary before declaring success.
verify_native() {
    local exe="$1" arch="$2"
    [[ -x "$exe" ]] || { echo "ERROR: $exe missing after bundling"; exit 1; }
    case "$(uname -m)" in
        arm64|aarch64) NATIVE=arm64 ;;
        x86_64) NATIVE=amd64 ;;
    esac
    if [[ "$arch" == "$NATIVE" ]]; then
        version_of "$exe" | grep -q "$TS_VERSION" ||
            { echo "ERROR: bundled binary reports wrong version: $(version_of "$exe")"; exit 1; }
    else
        # Foreign-arch target: verify the file format instead of executing.
        file "$exe" | grep -qiE "ELF|PE32|Mach-O" ||
            { echo "ERROR: $exe is not a valid executable for the target"; exit 1; }
    fi
}

case "$TARGET" in
  x86_64-unknown-linux-gnu)
    A="tailscale_${TS_VERSION}_amd64.tgz"; D="tailscale_${TS_VERSION}_amd64"
    curl -fsSL "https://pkgs.tailscale.com/stable/${A}" -o "$TMP/ts.tgz"
    echo "${SHA_LINUX_AMD64}  $TMP/ts.tgz" | sha256sum -c - ||
        { echo "ERROR: linux tarball checksum mismatch"; exit 1; }
    tar xzf "$TMP/ts.tgz" -C "$TMP"
    cp "$TMP/$D/tailscale" "$TMP/$D/tailscaled" "$DEST/"
    verify_native "$DEST/tailscaled" amd64 ;;

  aarch64-apple-darwin|x86_64-apple-darwin)
    [[ $TARGET == x86_64-* ]] && GOARCH=amd64 || GOARCH=arm64
    # Build from the pinned tag; clone/build failures are FATAL.
    SRC="$TMP/tailscale-src"
    git clone -q --depth 1 --branch "v${TS_VERSION}" \
        https://github.com/tailscale/tailscale.git "$SRC"
    ( cd "$SRC" \
      && CGO_ENABLED=0 GOOS=darwin GOARCH=${GOARCH} go build -trimpath -o "$TMP/tailscaled" ./cmd/tailscaled \
      && CGO_ENABLED=0 GOOS=darwin GOARCH=${GOARCH} go build -trimpath -o "$TMP/tailscale-cli" ./cmd/tailscale )
    cp "$TMP/tailscaled" "$DEST/tailscaled"
    cp "$TMP/tailscale-cli" "$DEST/tailscale"
    verify_native "$DEST/tailscaled" "$GOARCH" ;;

  x86_64-pc-windows-msvc)
    A="tailscale-setup-${TS_VERSION}.exe"
    curl -fsSL "https://pkgs.tailscale.com/stable/${A}" -o "$TMP/ts.exe"
    echo "${SHA_WIN_AMD64}  $TMP/ts.exe" | sha256sum -c - ||
        { echo "ERROR: windows installer checksum mismatch"; exit 1; }
    7z x -y -o"$TMP/win" "$TMP/ts.exe" >/dev/null
    find "$TMP/win" -name "tailscaled.exe" -exec cp {} "$DEST/" \;
    find "$TMP/win" -name "tailscale.exe"  -exec cp {} "$DEST/" \;
    [[ -f "$DEST/tailscaled.exe" && -f "$DEST/tailscale.exe" ]] ||
        { echo "ERROR: tailscaled.exe/tailscale.exe not found in the NSIS payload"; exit 1; } ;;

  *) echo "ERROR: unsupported target: $TARGET"; exit 1 ;;
esac

echo "Bundled tailscale ${TS_VERSION} for ${TARGET} into $DEST"
ls -la "$DEST"
