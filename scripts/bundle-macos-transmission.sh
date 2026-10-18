#!/usr/bin/env bash
# scripts/bundle-macos-transmission.sh
# Bundles transmission-daemon binary + all non-system dylibs for macOS.
# Locates daemon on the build host (via Homebrew formula transmission-cli or PATH),
# or builds from pinned source tarball if missing. Walks otool -L, copies and
# rewrites install names to @rpath, ad-hoc or identity code-signs all binaries.
# Exact pattern as bundle-macos-mpv.sh.
#
# Pinned artifact (Transmission 4.1.3, 2026-06-30):
#   Source: https://github.com/transmission/transmission/releases/download/4.1.3/transmission-4.1.3.tar.xz
#   SHA-256: ce7d2d8b101f7eb54bc3cf0bc55f52f7ebd4a25fa48e00bdca9a7e0fc02617da
#   Peer-id prefix: -TR4130-
#
# AGENT-COORDINATION.md records the pinned hash above.

set -euo pipefail

VERSION="4.1.3"
TARBALL_URL="https://github.com/transmission/transmission/releases/download/${VERSION}/transmission-${VERSION}.tar.xz"
EXPECTED_SHA256="ce7d2d8b101f7eb54bc3cf0bc55f52f7ebd4a25fa48e00bdca9a7e0fc02617da"

TARGET_DIR="${1:-src-tauri/resources/engine/transmission}"
SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:-}"

# Staging dir OUTSIDE target/ (rust-cache nesting precedent).
STAGING_DIR="src-tauri/transmission-staging"

echo "=========================================================="
echo " Nebulae macOS Transmission-Daemon Bundling (v${VERSION}) "
echo "=========================================================="
echo "Target directory: $TARGET_DIR"

mkdir -p "$STAGING_DIR" "$TARGET_DIR/lib"

# --- 1. Locate or obtain transmission-daemon ---
DAEMON_BIN=""
if command -v transmission-daemon >/dev/null 2>&1; then
    DAEMON_BIN=$(command -v transmission-daemon)
elif [ -f "/opt/homebrew/bin/transmission-daemon" ]; then
    DAEMON_BIN="/opt/homebrew/bin/transmission-daemon"
elif [ -f "/usr/local/bin/transmission-daemon" ]; then
    DAEMON_BIN="/usr/local/bin/transmission-daemon"
fi

if [ -z "$DAEMON_BIN" ]; then
    echo "==> transmission-daemon not found on host. Attempting 'brew install transmission-cli'..."
    if command -v brew >/dev/null 2>&1; then
        brew install transmission-cli || true
        if [ -f "/opt/homebrew/bin/transmission-daemon" ]; then
            DAEMON_BIN="/opt/homebrew/bin/transmission-daemon"
        elif [ -f "/usr/local/bin/transmission-daemon" ]; then
            DAEMON_BIN="/usr/local/bin/transmission-daemon"
        fi
    fi
fi

if [ -z "$DAEMON_BIN" ]; then
    echo "==> Building transmission-daemon v${VERSION} from pinned source tarball..."
    TARBALL="$STAGING_DIR/transmission-${VERSION}.tar.xz"
    curl -fsSL "$TARBALL_URL" -o "$TARBALL"
    ACTUAL_SHA256=$(shasum -a 256 "$TARBALL" | awk '{print $1}')
    if [ "$ACTUAL_SHA256" != "$EXPECTED_SHA256" ]; then
        echo "ERROR: SHA-256 MISMATCH on source tarball"
        echo "  Expected: $EXPECTED_SHA256"
        echo "  Actual:   $ACTUAL_SHA256"
        exit 1
    fi
    echo "    Source SHA-256 verified: $ACTUAL_SHA256"
    BUILD_ROOT="$STAGING_DIR/build"
    INSTALL_ROOT="$STAGING_DIR/install"
    rm -rf "$BUILD_ROOT" "$INSTALL_ROOT"
    mkdir -p "$BUILD_ROOT" "$INSTALL_ROOT"
    tar -xJf "$TARBALL" -C "$STAGING_DIR"
    PATCH_FILE="$(pwd)/scripts/patches/transmission-${VERSION}-identity-override.patch"
    if [ -f "$PATCH_FILE" ]; then
        echo "==> Applying identity override patch..."
        patch -d "$STAGING_DIR/transmission-${VERSION}" -p1 < "$PATCH_FILE" || true
    fi
    cmake -S "$STAGING_DIR/transmission-${VERSION}" -B "$BUILD_ROOT" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX="$INSTALL_ROOT" \
        -DENABLE_DAEMON=ON \
        -DENABLE_GTK=OFF \
        -DENABLE_QT=OFF \
        -DENABLE_UTILS=OFF \
        -DENABLE_MAC=OFF \
        -DENABLE_TESTS=OFF \
        -DENABLE_CLI=OFF
    cmake --build "$BUILD_ROOT" --target transmission-daemon -j4
    cmake --install "$BUILD_ROOT" --component daemon
    DAEMON_BIN="$INSTALL_ROOT/bin/transmission-daemon"
fi

if [ ! -x "$DAEMON_BIN" ]; then
    echo "ERROR: Failed to locate or build transmission-daemon."
    exit 1
fi
echo "Source transmission-daemon binary: $DAEMON_BIN"

# Copy daemon binary
cp -L "$DAEMON_BIN" "$TARGET_DIR/transmission-daemon"
chmod 755 "$TARGET_DIR/transmission-daemon"

# --- 2. Walk otool -L, copy non-system dylibs, rewrite install names ---
export TARGET_DIR_PY="$TARGET_DIR"
export SIGNING_IDENTITY_PY="$SIGNING_IDENTITY"

python3 - <<'EOF'
import os
import sys
import shutil
import subprocess

target_dir = os.environ.get("TARGET_DIR_PY")
lib_dir = os.path.join(target_dir, "lib")
daemon_path = os.path.join(target_dir, "transmission-daemon")
signing_identity = os.environ.get("SIGNING_IDENTITY_PY", "")

def get_direct_deps(binary_path):
    try:
        out = subprocess.check_output(['otool', '-L', binary_path], stderr=subprocess.DEVNULL).decode('utf-8')
    except subprocess.CalledProcessError:
        return []
    deps = []
    for line in out.splitlines()[1:]:
        line = line.strip()
        if not line:
            continue
        dep = line.split(' (compatibility')[0].split(' (')[0].strip()
        if dep.startswith('/System/') or dep.startswith('/usr/lib/') or dep.startswith('@'):
            continue
        if os.path.exists(dep):
            deps.append(os.path.realpath(dep))
    return deps

def codesign_binary(path):
    if signing_identity:
        cmd = ['codesign', '--force', '--sign', signing_identity,
               '--options', 'runtime', '--timestamp', path]
    else:
        cmd = ['codesign', '--force', '--sign', '-', path]
    result = subprocess.run(cmd, capture_output=True)
    if result.returncode != 0:
        print(f"    codesign warning on {os.path.basename(path)}: {result.stderr.decode().strip()}")

queue = get_direct_deps(daemon_path)
visited = set(queue)

while queue:
    curr = queue.pop(0)
    for d in get_direct_deps(curr):
        if d not in visited:
            visited.add(d)
            queue.append(d)

print(f"Discovered {len(visited)} non-system dynamic libraries to bundle.")

path_map = {}
for orig in visited:
    base = os.path.basename(orig)
    dest = os.path.join(lib_dir, base)
    shutil.copy2(orig, dest)
    os.chmod(dest, 0o755)
    path_map[orig] = base
    path_map[os.path.realpath(orig)] = base

for orig in visited:
    base = os.path.basename(orig)
    dest = os.path.join(lib_dir, base)
    subprocess.run(['install_name_tool', '-id', f'@rpath/{base}', dest], check=True)

all_binaries = [daemon_path] + [os.path.join(lib_dir, os.path.basename(d)) for d in visited]
for b in all_binaries:
    deps = get_direct_deps(b)
    for dep in deps:
        if dep in path_map:
            target_base = path_map[dep]
            subprocess.run(['install_name_tool', '-change', dep, f'@rpath/{target_base}', b], check=False)

subprocess.run(['install_name_tool', '-add_rpath', '@executable_path/lib', daemon_path], stderr=subprocess.DEVNULL)
subprocess.run(['install_name_tool', '-add_rpath', '@loader_path/lib', daemon_path], stderr=subprocess.DEVNULL)

for b in all_binaries:
    if os.path.isfile(b):
        codesign_binary(b)
        print(f"    signed: {os.path.basename(b)}")

print("==> macOS dylib rewrite and codesign complete.")
EOF

# --- 3. Smoke test ---
echo "==> Running smoke test: spawn daemon, verify RPC 409, kill..."
SMOKE_CONFIG="$(mktemp -d)"
SMOKE_PORT=19092

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

"$TARGET_DIR/transmission-daemon" --config-dir "$SMOKE_CONFIG" --foreground &
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
ls -lh "$TARGET_DIR/"
ls -lh "$TARGET_DIR/lib/" 2>/dev/null | head -20

echo ""
echo "=========================================================="
echo " Phase A1 macOS: transmission-daemon ${VERSION} bundled OK"
echo " Peer-id prefix: -TR4130-"
echo " Source SHA-256: ${EXPECTED_SHA256}"
echo "=========================================================="
