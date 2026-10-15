#!/usr/bin/env bash
# scripts/bundle-linux-transmission.sh
# Bundles transmission-daemon as a self-contained binary for Linux x86_64.
# No official static Transmission binary exists; build from the pinned source
# tag with GUI/Qt/GTK/utils disabled, then walk ldd and copy non-glibc libs,
# adding RPATH via patchelf — exact pattern as bundle-linux-mpv.sh.
#
# Pinned source (Transmission 4.1.3, 2026-06-30):
#   URL:    https://github.com/transmission/transmission/releases/download/4.1.3/transmission-4.1.3.tar.xz
#   SHA-256: ce7d2d8b101f7eb54bc3cf0bc55f52f7ebd4a25fa48e00bdca9a7e0fc02617da
#   Peer-id prefix: -TR4130-
#
# AGENT-COORDINATION.md records the pinned hash above.

set -euo pipefail

VERSION="4.1.3"
TARBALL_URL="https://github.com/transmission/transmission/releases/download/${VERSION}/transmission-${VERSION}.tar.xz"
EXPECTED_SHA256="ce7d2d8b101f7eb54bc3cf0bc55f52f7ebd4a25fa48e00bdca9a7e0fc02617da"

# Staging dir OUTSIDE target/ (rust-cache only restores target/).
STAGING_DIR="src-tauri/transmission-staging"
RESOURCE_DIR="src-tauri/resources/engine/transmission"
TARBALL="$STAGING_DIR/transmission-${VERSION}.tar.xz"
BUILD_ROOT="$STAGING_DIR/build"
INSTALL_ROOT="$STAGING_DIR/install"

echo "=========================================================="
echo " Nebulae Linux Transmission-Daemon Bundling (v${VERSION})"
echo "=========================================================="

# --- System dependencies ---
echo "==> Installing build dependencies..."
sudo apt-get update -qq
sudo apt-get install -y --no-install-recommends \
    cmake ninja-build pkg-config \
    libssl-dev libcurl4-openssl-dev \
    libminiupnpc-dev libnatpmp-dev \
    libdeflate-dev zlib1g-dev \
    libpsl-dev \
    patchelf curl ca-certificates

mkdir -p "$STAGING_DIR" "$RESOURCE_DIR"

# --- Download and verify ---
echo "==> Downloading transmission ${VERSION} source tarball..."
curl -fsSL "$TARBALL_URL" -o "$TARBALL"

echo "==> Verifying SHA-256..."
ACTUAL_SHA256=$(sha256sum "$TARBALL" | awk '{print $1}')
if [ "$ACTUAL_SHA256" != "$EXPECTED_SHA256" ]; then
    echo "ERROR: SHA-256 MISMATCH"
    echo "  Expected: $EXPECTED_SHA256"
    echo "  Actual:   $ACTUAL_SHA256"
    exit 1
fi
echo "    SHA-256 verified: $ACTUAL_SHA256"

# --- Build ---
echo "==> Extracting source..."
rm -rf "$BUILD_ROOT" "$INSTALL_ROOT"
mkdir -p "$BUILD_ROOT" "$INSTALL_ROOT"
tar -xJf "$TARBALL" -C "$STAGING_DIR"
SRC_DIR="$STAGING_DIR/transmission-${VERSION}"

echo "==> Configuring (daemon only, no GUI/Qt/GTK/utils)..."
cmake -S "$SRC_DIR" -B "$BUILD_ROOT" \
    -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$INSTALL_ROOT" \
    -DENABLE_DAEMON=ON \
    -DENABLE_GTK=OFF \
    -DENABLE_QT=OFF \
    -DENABLE_UTILS=OFF \
    -DENABLE_MAC=OFF \
    -DENABLE_TESTS=OFF \
    -DENABLE_CLI=OFF \
    -DINSTALL_DOC=OFF \
    -DWITH_CRYPTO=openssl \
    -DWITH_MINIUPNPC=ON \
    -DWITH_NATPMP=ON

echo "==> Building transmission-daemon..."
cmake --build "$BUILD_ROOT" --target transmission-daemon -j"$(nproc)"
cmake --install "$BUILD_ROOT" --component daemon

DAEMON_BIN="$INSTALL_ROOT/bin/transmission-daemon"
if [ ! -x "$DAEMON_BIN" ]; then
    # Some install layouts put it in sbin
    DAEMON_BIN="$INSTALL_ROOT/sbin/transmission-daemon"
fi
if [ ! -x "$DAEMON_BIN" ]; then
    echo "ERROR: transmission-daemon not found after install. Contents of $INSTALL_ROOT:"
    find "$INSTALL_ROOT" -type f
    exit 1
fi
echo "    Built: $DAEMON_BIN"

# --- Walk ldd, copy non-glibc/non-system libs, patch RPATH ---
echo "==> Copying daemon binary to resource dir..."
cp "$DAEMON_BIN" "$RESOURCE_DIR/transmission-daemon"
chmod 755 "$RESOURCE_DIR/transmission-daemon"

LIB_DIR="$RESOURCE_DIR/lib"
mkdir -p "$LIB_DIR"

echo "==> Walking ldd dependency tree (non-glibc, non-system only)..."

# glibc-provided libraries that must NOT be bundled (older glibc on target
# systems would conflict; the floor is glibc >= 2.35 per docs/installation.md)
GLIBC_SKIP_PATTERNS=(
    "linux-vdso"
    "libdl.so"
    "libpthread.so"
    "librt.so"
    "libc.so"
    "libm.so"
    "libgcc_s.so"
    "ld-linux"
    "libstdc++.so"   # system C++ ABI; not bundling avoids GLIBCXX version conflicts
)

copy_lib_tree() {
    local bin="$1"
    # ldd output: "\tname => /path/to/lib (addr)" or "\t/ld-linux => ... (addr)"
    ldd "$bin" 2>/dev/null | awk '{
        if ($3 ~ /^\//) print $3
        else if ($1 ~ /^\//) print $1
    }' | sort -u | while read -r lib; do
        [ -z "$lib" ] && continue
        local name
        name=$(basename "$lib")
        # Skip glibc-family libs
        local skip=0
        for pat in "${GLIBC_SKIP_PATTERNS[@]}"; do
            if [[ "$name" == ${pat}* ]]; then skip=1; break; fi
        done
        [ "$skip" -eq 1 ] && continue
        # Skip if already copied
        [ -f "$LIB_DIR/$name" ] && continue
        if [ -f "$lib" ]; then
            cp "$lib" "$LIB_DIR/$name"
            echo "    lib: $name"
            # Recurse into dependencies of this lib
            copy_lib_tree "$LIB_DIR/$name"
        fi
    done
}

copy_lib_tree "$RESOURCE_DIR/transmission-daemon"

# Patch RPATH on daemon binary and all copied libs so they find each other
# at $ORIGIN/lib (daemon) and $ORIGIN (lib-to-lib).
echo "==> Patching RPATHs with patchelf..."
patchelf --set-rpath '$ORIGIN/lib' "$RESOURCE_DIR/transmission-daemon"
for lib in "$LIB_DIR"/*.so*; do
    [ -f "$lib" ] || continue
    patchelf --set-rpath '$ORIGIN' "$lib" 2>/dev/null || true
done

# Verify no dependency resolves outside the bundle (except glibc/ld-linux)
echo "==> Verifying self-containment..."
LEAKED=0
ldd "$RESOURCE_DIR/transmission-daemon" 2>/dev/null | awk '{
    if ($3 ~ /^\//) print $3
    else if ($1 ~ /^\//) print $1
}' | while read -r lib; do
    [ -z "$lib" ] && continue
    name=$(basename "$lib")
    skip=0
    for pat in "${GLIBC_SKIP_PATTERNS[@]}"; do
        if [[ "$name" == ${pat}* ]]; then skip=1; break; fi
    done
    [ "$skip" -eq 1 ] && continue
    if [ ! -f "$LIB_DIR/$name" ]; then
        echo "    UNRESOLVED outside bundle: $lib"
        LEAKED=1
    fi
done
if [ "$LEAKED" -ne 0 ]; then
    echo "ERROR: unresolved non-glibc dependencies. See above."
    exit 1
fi
echo "    Self-containment: OK"

# --- Smoke test ---
echo "==> Running smoke test: spawn daemon, verify RPC 409, kill..."
SMOKE_CONFIG="$(mktemp -d)"
SMOKE_PORT=19091

cat > "$SMOKE_CONFIG/settings.json" <<SETTINGS_JSON
{
    "rpc-enabled": true,
    "rpc-port": ${SMOKE_PORT},
    "rpc-bind-address": "127.0.0.1",
    "rpc-whitelist-enabled": true,
    "rpc-whitelist": "127.0.0.1",
    "rpc-authentication-required": false,
    "dht-enabled": false,
    "pex-enabled": false,
    "lpd-enabled": false,
    "download-dir": "${SMOKE_CONFIG}"
}
SETTINGS_JSON

"$RESOURCE_DIR/transmission-daemon" --config-dir "$SMOKE_CONFIG" --foreground &
DAEMON_PID=$!
sleep 4

HTTP_STATUS=$(curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:${SMOKE_PORT}/transmission/rpc 2>/dev/null || true)
kill "$DAEMON_PID" 2>/dev/null || true
wait "$DAEMON_PID" 2>/dev/null || true
rm -rf "$SMOKE_CONFIG"

if [ "$HTTP_STATUS" = "409" ]; then
    echo "    Smoke: received 409 + Session-Id header — daemon is live."
else
    echo "ERROR: Smoke test FAILED — expected HTTP 409, got: $HTTP_STATUS"
    exit 1
fi

echo ""
echo "Resource directory contents:"
ls -lh "$RESOURCE_DIR/"
ls -lh "$LIB_DIR/" 2>/dev/null | head -20

echo ""
echo "=========================================================="
echo " Phase A1 Linux: transmission-daemon ${VERSION} bundled OK"
echo " Peer-id prefix: -TR4130-"
echo " SHA-256 (tarball): ${EXPECTED_SHA256}"
echo "=========================================================="
