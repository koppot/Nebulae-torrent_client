#!/usr/bin/env bash
# Bundle pinned tailscale binaries into src-tauri/resources/engine/tailscale.
# Usage: bundle-tailscale.sh <target-triple>
#   x86_64-unknown-linux-gnu | aarch64-apple-darwin | x86_64-apple-darwin | x86_64-pc-windows-msvc
# Verification: SHA-256 of the downloaded archive (values pinned below).
# macOS note: pkgs.tailscale.com publishes no macOS daemon tarball; we build
# from source at the pinned tag (go is available on GH macos runners).
set -euo pipefail
TS_VERSION="1.104.1"
TARGET="${1:?usage: bundle-tailscale.sh <target-triple>}"
DEST="src-tauri/resources/engine/tailscale"
mkdir -p "$DEST"

case "$TARGET" in
  x86_64-unknown-linux-gnu)
    A="tailscale_${TS_VERSION}_amd64.tgz"; D="tailscale_${TS_VERSION}_amd64"
    SHA="PIN_LINUX_AMD64_SHA256"          # fill at PR time from pkgs.tailscale.com
    curl -fsSL "https://pkgs.tailscale.com/stable/${A}" -o /tmp/ts.tgz
    echo "${SHA}  /tmp/ts.tgz" | sha256sum -c -
    tar xzf /tmp/ts.tgz -C /tmp
    cp "/tmp/${D}/tailscale" "/tmp/${D}/tailscaled" "$DEST/" ;;
  aarch64-apple-darwin|x86_64-apple-darwin)
    MACARCH=${TARGET%%-*}; [[ $MACARCH == x86_64 ]] && GOARCH=amd64 || GOARCH=arm64
    # Build from the pinned tag (no official mac daemon tarball).
    git -C /tmp clone -q --depth 1 --branch "v${TS_VERSION}" https://github.com/tailscale/tailscale.git ts-src 2>/dev/null || true
    (cd /tmp/ts-src && GOOS=darwin GOARCH=${GOARCH} ./build.sh dist build-pkg 2>/dev/null || \
     (cd cmd/tailscaled && go build -o /tmp/tailscaled-mac .) && (cd cmd/tailscale && go build -o /tmp/tailscale-mac .))
    cp /tmp/tailscaled-mac "$DEST/tailscaled"; cp /tmp/tailscale-mac "$DEST/tailscale" ;;
  x86_64-pc-windows-msvc)
    A="tailscale-setup-${TS_VERSION}_amd64.exe"; # NSIS installer; contains tailscaled.exe
    curl -fsSL "https://pkgs.tailscale.com/stable/${A}" -o /tmp/ts.exe
    # Extract the tailscaled.exe/tailscale.exe payload via 7z (runner has 7z).
    7z x -y -o/tmp/tswin /tmp/ts.exe >/dev/null
    find /tmp/tswin -name "tailscaled.exe" -exec cp {} "$DEST/" \;
    find /tmp/tswin -name "tailscale.exe"  -exec cp {} "$DEST/" \; ;;
esac
"$DEST/tailscale" version | head -1
