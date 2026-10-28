#!/usr/bin/env bash
# scripts/bundle-macos-transmission.sh
#
# Stages transmission-daemon 4.1.3 (with the Nebulae identity-override patch) and
# every non-system dynamic library it needs, so that the app bundle never reads a
# library from the build machine's Homebrew prefix.
#
#   1. Locate a patched daemon (staging dir, or the host) or build one from the
#      pinned, SHA-256 verified source tarball with
#      scripts/patches/transmission-4.1.3-identity-override.patch.
#   2. scripts/relocate-macos-dylibs.py copies the dependency closure into lib/,
#      rewrites every load command using the exact string stored in the binary,
#      and signs what it changed (ad-hoc, or APPLE_SIGNING_IDENTITY when set).
#   3. scripts/audit-macos-bundle.py --strict --strict-strings must find nothing
#      that points outside the bundle. The script fails here otherwise.
#   4. Licence texts, the identity patch and bundled-libraries.json are written
#      next to the daemon.
#   5. Smoke test in an isolated temporary profile (loopback only, no port
#      forwarding, its own RPC and peer port).
#
# Pinned artifact (Transmission 4.1.3, 2026-06-30):
#   Source: https://github.com/transmission/transmission/releases/download/4.1.3/transmission-4.1.3.tar.xz
#   SHA-256: ce7d2d8b101f7eb54bc3cf0bc55f52f7ebd4a25fa48e00bdca9a7e0fc02617da
#   Peer-id prefix: -TR4130-
#
# AGENT-COORDINATION.md records the pinned hash above.
#
# Run from the repository root. Usage: bundle-macos-transmission.sh [TARGET_DIR]

set -euo pipefail

VERSION="4.1.3"
TARBALL_URL="https://github.com/transmission/transmission/releases/download/${VERSION}/transmission-${VERSION}.tar.xz"
EXPECTED_SHA256="ce7d2d8b101f7eb54bc3cf0bc55f52f7ebd4a25fa48e00bdca9a7e0fc02617da"

TARGET_DIR="${1:-src-tauri/resources/engine/transmission}"
SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:-}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PATCH_FILE="$SCRIPT_DIR/patches/transmission-${VERSION}-identity-override.patch"

# Staging dir OUTSIDE target/ (rust-cache nesting precedent).
STAGING_DIR="src-tauri/transmission-staging"
TARBALL="$STAGING_DIR/transmission-${VERSION}.tar.xz"

echo "=========================================================="
echo " Nebulae macOS Transmission-Daemon Bundling (v${VERSION}) "
echo "=========================================================="
echo "Target directory: $TARGET_DIR"

mkdir -p "$STAGING_DIR"

# The source tarball is needed to build the daemon and, either way, for the
# licence texts that ship with it. Never use an unverified copy.
ensure_verified_tarball() {
    if [ ! -f "$TARBALL" ]; then
        curl -fsSL "$TARBALL_URL" -o "$TARBALL"
    fi
    local actual
    actual=$(shasum -a 256 "$TARBALL" | awk '{print $1}')
    if [ "$actual" != "$EXPECTED_SHA256" ]; then
        echo "ERROR: SHA-256 MISMATCH on source tarball"
        echo "  Expected: $EXPECTED_SHA256"
        echo "  Actual:   $actual"
        exit 1
    fi
    echo "    Source SHA-256 verified: $actual"
}

# --- 1. Locate or obtain transmission-daemon ---
is_patched_daemon() {
    local bin="$1"
    [ -n "$bin" ] && [ -x "$bin" ] && \
        grep -q -a "TRANSMISSION_PEER_ID_PREFIX" "$bin" && \
        grep -q -a "TRANSMISSION_USER_AGENT" "$bin"
}

DAEMON_BIN=""

# Check if a previously-built patched daemon is available in staging
if [ -x "$STAGING_DIR/install/bin/transmission-daemon" ] && is_patched_daemon "$STAGING_DIR/install/bin/transmission-daemon"; then
    echo "==> Reusing verified patched daemon from $STAGING_DIR/install"
    DAEMON_BIN="$STAGING_DIR/install/bin/transmission-daemon"
elif [ -x "$STAGING_DIR/build/daemon/transmission-daemon" ] && is_patched_daemon "$STAGING_DIR/build/daemon/transmission-daemon"; then
    echo "==> Reusing verified patched daemon from $STAGING_DIR/build"
    DAEMON_BIN="$STAGING_DIR/build/daemon/transmission-daemon"
fi

# Reject unpatched host/Homebrew binaries that bypass identity hooks
if [ -z "$DAEMON_BIN" ]; then
    HOST_BIN=""
    if command -v transmission-daemon >/dev/null 2>&1; then
        HOST_BIN=$(command -v transmission-daemon)
    elif [ -f "/opt/homebrew/bin/transmission-daemon" ]; then
        HOST_BIN="/opt/homebrew/bin/transmission-daemon"
    elif [ -f "/usr/local/bin/transmission-daemon" ]; then
        HOST_BIN="/usr/local/bin/transmission-daemon"
    fi

    if [ -n "$HOST_BIN" ]; then
        if is_patched_daemon "$HOST_BIN"; then
            echo "==> Using verified patched transmission-daemon from host: $HOST_BIN"
            DAEMON_BIN="$HOST_BIN"
        else
            echo "==> Host transmission-daemon at $HOST_BIN lacks identity override patch; building patched daemon from source..."
        fi
    fi
fi

if [ -z "$DAEMON_BIN" ]; then
    echo "==> Building transmission-daemon v${VERSION} from pinned source tarball with identity patch..."
    ensure_verified_tarball
    BUILD_ROOT="$STAGING_DIR/build"
    INSTALL_ROOT="$STAGING_DIR/install"
    SOURCE_ROOT="$STAGING_DIR/transmission-${VERSION}"
    # A previously patched tree would make patch(1) ask whether to reverse the
    # patch, so always start from a pristine extraction.
    rm -rf "$BUILD_ROOT" "$INSTALL_ROOT" "$SOURCE_ROOT"
    mkdir -p "$BUILD_ROOT" "$INSTALL_ROOT"
    tar -xJf "$TARBALL" -C "$STAGING_DIR"
    if [ ! -f "$PATCH_FILE" ]; then
        echo "ERROR: Identity override patch not found at: $PATCH_FILE"
        exit 1
    fi
    echo "==> Applying identity override patch..."
    patch -d "$SOURCE_ROOT" -p1 < "$PATCH_FILE"
    cmake -S "$SOURCE_ROOT" -B "$BUILD_ROOT" \
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
    cmake --install "$BUILD_ROOT" --component daemon 2>/dev/null || true
    if [ -x "$BUILD_ROOT/daemon/transmission-daemon" ]; then
        DAEMON_BIN="$BUILD_ROOT/daemon/transmission-daemon"
    elif [ -x "$INSTALL_ROOT/bin/transmission-daemon" ]; then
        DAEMON_BIN="$INSTALL_ROOT/bin/transmission-daemon"
    fi
fi

if [ ! -x "$DAEMON_BIN" ]; then
    echo "ERROR: Failed to locate or build transmission-daemon."
    exit 1
fi

if ! is_patched_daemon "$DAEMON_BIN"; then
    echo "ERROR: transmission-daemon at $DAEMON_BIN is NOT patched with required identity overrides!"
    exit 1
fi
echo "Source transmission-daemon binary (verified patched): $DAEMON_BIN"

# Remove what an earlier run produced (for example libraries the previous
# bundling logic copied but never linked) so nothing stale ends up in the app.
# The binary is removed before it is copied: overwriting a signed executable in
# place keeps the old code-signature cache entry and the kernel then kills the
# new file at launch.
mkdir -p "$TARGET_DIR"
rm -rf "$TARGET_DIR/lib" "$TARGET_DIR/licenses"
rm -f "$TARGET_DIR/transmission-daemon" "$TARGET_DIR/bundled-libraries.json"
mkdir -p "$TARGET_DIR/lib"
cp -L "$DAEMON_BIN" "$TARGET_DIR/transmission-daemon"
chmod 755 "$TARGET_DIR/transmission-daemon"

# --- 2. Bundle every non-system dependency and rewrite the load commands ---
ARCH="$(uname -m)"
RELOCATE_ARGS=(
    --lib-dir "$TARGET_DIR/lib"
    --add-rpath "@executable_path/lib"
    --arch "$ARCH"
    --manifest "$TARGET_DIR/bundled-libraries.json"
    --licenses-dir "$TARGET_DIR/licenses"
)
if [ -n "$SIGNING_IDENTITY" ]; then
    RELOCATE_ARGS+=(--sign "$SIGNING_IDENTITY")
fi
python3 "$SCRIPT_DIR/relocate-macos-dylibs.py" "${RELOCATE_ARGS[@]}" "$TARGET_DIR/transmission-daemon"

# macOS Finder litter must never reach the app bundle.
find "$TARGET_DIR" -name .DS_Store -delete

# --- 3. Recursive closure audit: nothing may point outside the bundle ---
echo "==> Auditing the staged component (strict)..."
python3 "$SCRIPT_DIR/audit-macos-bundle.py" --strict --strict-strings --arch "$ARCH" "$TARGET_DIR"

# A bundled library keeps the deployment target it was built with. Homebrew
# bottles target the macOS of the machine that built them, which can be newer
# than the minimum the app declares, and the daemon then cannot start on the
# older systems the app claims to support. Report it; NEBULAE_ENFORCE_MINOS=1
# turns it into a failure.
DECLARED_MIN_MACOS="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["bundle"]["macOS"]["minimumSystemVersion"])' "$SCRIPT_DIR/../src-tauri/tauri.conf.json" 2>/dev/null || true)"
if [ -n "$DECLARED_MIN_MACOS" ]; then
    MINOS_REPORT="$(python3 "$SCRIPT_DIR/audit-macos-bundle.py" --arch "$ARCH" --max-minos "$DECLARED_MIN_MACOS" "$TARGET_DIR" | grep -E '^MINOS' || true)"
    if [ -n "$MINOS_REPORT" ]; then
        echo "WARNING: some bundled images need a newer macOS than the declared minimum (${DECLARED_MIN_MACOS}):"
        printf '%s\n' "$MINOS_REPORT"
        if [ "${NEBULAE_ENFORCE_MINOS:-0}" = "1" ]; then
            echo "ERROR: NEBULAE_ENFORCE_MINOS=1"
            exit 1
        fi
    else
        echo "    Every bundled image runs on the declared minimum macOS (${DECLARED_MIN_MACOS})."
    fi
fi

# --- 4. Licences and corresponding-source information ---
ensure_verified_tarball
LICENSE_DIR="$TARGET_DIR/licenses/transmission-${VERSION}"
mkdir -p "$LICENSE_DIR"
python3 - "$TARBALL" "$LICENSE_DIR" "$VERSION" <<'PY'
import os
import re
import sys
import tarfile

tarball, dest, version = sys.argv[1:4]
top = "transmission-%s/" % version
# googletest and the CMake modules are test / build-time only and never ship.
skipped_third_party = {"googletest", "rpavlik-cmake-modules"}
notice = re.compile(r"^(COPYING|LICEN[CS]E|AUTHORS|COPYRIGHT)([._-].*)?$", re.I)

copied = []
with tarfile.open(tarball) as tf:
    for member in tf.getmembers():
        if not member.isfile() or not member.name.startswith(top):
            continue
        rel = member.name[len(top):]
        parts = rel.split("/")
        if len(parts) == 1:
            wanted = parts[0] in ("COPYING", "AUTHORS")
        elif len(parts) == 2 and parts[0] == "licenses":
            wanted = parts[1].endswith(".txt")
        elif len(parts) == 3 and parts[0] == "third-party":
            wanted = parts[1] not in skipped_third_party and bool(notice.match(parts[2]))
        else:
            wanted = False
        if not wanted:
            continue
        out = os.path.join(dest, rel)
        os.makedirs(os.path.dirname(out), exist_ok=True)
        with tf.extractfile(member) as src, open(out, "wb") as dst:
            dst.write(src.read())
        copied.append(rel)

if "COPYING" not in copied or not any(r.startswith("licenses/") for r in copied):
    sys.exit("ERROR: the Transmission licence files were not found in the source tarball")
print("    Transmission licence files copied: %d" % len(copied))
PY
cp "$PATCH_FILE" "$LICENSE_DIR/"
cat > "$LICENSE_DIR/SOURCE.txt" <<SOURCE_TXT
Transmission ${VERSION} as shipped in Nebulae
==============================================

Upstream source : ${TARBALL_URL}
SHA-256         : ${EXPECTED_SHA256}

Local change    : transmission-${VERSION}-identity-override.patch (in this directory)
                  is applied to the unpacked upstream source before building. It lets
                  the host application set the peer-id prefix and user agent through
                  TRANSMISSION_PEER_ID_PREFIX and TRANSMISSION_USER_AGENT.

Licence         : COPYING (GPLv2, GPLv3, or a later licence endorsed by Mnemosyne LLC)
                  with the full texts in licenses/. Third-party code that Transmission
                  carries in its source tree is covered by third-party/*.

Libraries       : the dynamic libraries in ../lib are listed, with their versions and
                  SHA-256 sums, in ../bundled-libraries.json. Their licence files are in
                  the sibling directories of this one.
SOURCE_TXT

# --- 5. Smoke test (isolated: own profile, loopback only, no port forwarding) ---
ABS_TARGET_DIR="$(cd "$TARGET_DIR" && pwd -P)"
echo "==> Checking which libraries the daemon loads..."
DYLD_TRACE="$(DYLD_PRINT_LIBRARIES=1 "$ABS_TARGET_DIR/transmission-daemon" --version 2>&1 || true)"
LOADED_IMAGES="$(printf '%s\n' "$DYLD_TRACE" | sed -n -E 's/^dyld.*<[0-9A-Fa-f-]{36}> +(\/.*)$/\1/p' | sort -u)"
if [ -z "$LOADED_IMAGES" ]; then
    echo "    dyld tracing is not available for this binary (hardened runtime); relying on the static audit."
else
    STRAY_IMAGES=""
    BUNDLED_LOADED=0
    while IFS= read -r image; do
        case "$image" in
            "$ABS_TARGET_DIR"/lib/*) BUNDLED_LOADED=$((BUNDLED_LOADED + 1)) ;;
            "$ABS_TARGET_DIR"/*|/usr/lib/*|/System/*|/Library/Apple/*) ;;
            *) STRAY_IMAGES="${STRAY_IMAGES}${image}"$'\n' ;;
        esac
    done <<EOF_IMAGES
$LOADED_IMAGES
EOF_IMAGES
    if [ -n "$STRAY_IMAGES" ]; then
        echo "ERROR: the daemon loaded libraries from outside the bundle:"
        printf '%s' "$STRAY_IMAGES"
        exit 1
    fi
    echo "    ${BUNDLED_LOADED} bundled libraries loaded, none from outside the bundle."
fi

echo "==> Running smoke test: spawn daemon in an isolated profile, verify RPC, stop it..."
SMOKE_PORT=19092
SMOKE_PEER_PORT=19093
if lsof -nP -iTCP:"${SMOKE_PORT}" -sTCP:LISTEN >/dev/null 2>&1; then
    echo "ERROR: port ${SMOKE_PORT} is already in use; refusing to talk to a different daemon."
    exit 1
fi

SMOKE_CONFIG="$(mktemp -d)"
SMOKE_PID=""
cleanup_smoke() {
    if [ -n "$SMOKE_PID" ]; then
        kill "$SMOKE_PID" 2>/dev/null || true
        wait "$SMOKE_PID" 2>/dev/null || true
    fi
    rm -rf "$SMOKE_CONFIG"
}
trap cleanup_smoke EXIT

cat > "$SMOKE_CONFIG/settings.json" <<SETTINGS_JSON
{
    "rpc-enabled": true,
    "rpc-port": ${SMOKE_PORT},
    "rpc-bind-address": "127.0.0.1",
    "rpc-whitelist-enabled": true,
    "rpc-whitelist": "127.0.0.1",
    "rpc-authentication-required": false,
    "peer-port": ${SMOKE_PEER_PORT},
    "peer-port-random-on-start": false,
    "bind-address-ipv4": "127.0.0.1",
    "bind-address-ipv6": "::1",
    "port-forwarding-enabled": false,
    "dht-enabled": false,
    "pex-enabled": false,
    "lpd-enabled": false,
    "download-dir": "${SMOKE_CONFIG}"
}
SETTINGS_JSON

"$ABS_TARGET_DIR/transmission-daemon" --config-dir "$SMOKE_CONFIG" --foreground >"$SMOKE_CONFIG/daemon.log" 2>&1 &
SMOKE_PID=$!

RPC_URL="http://127.0.0.1:${SMOKE_PORT}/transmission/rpc"
HTTP_STATUS=""
for _ in $(seq 1 30); do
    HTTP_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$RPC_URL" 2>/dev/null || true)
    [ "$HTTP_STATUS" = "409" ] && break
    kill -0 "$SMOKE_PID" 2>/dev/null || break
    sleep 0.5
done

if [ "$HTTP_STATUS" != "409" ]; then
    echo "ERROR: Smoke test FAILED — expected HTTP 409, got: ${HTTP_STATUS:-none}"
    sed -n '1,20p' "$SMOKE_CONFIG/daemon.log" || true
    exit 1
fi
echo "    Smoke: received 409 + Session-Id header — daemon is live."

SESSION_ID=$(curl -s -D - -o /dev/null "$RPC_URL" | awk 'tolower($1)=="x-transmission-session-id:" {print $2}' | tr -d '\r')
SESSION_JSON=$(curl -s -H "X-Transmission-Session-Id: ${SESSION_ID}" \
    -d '{"method":"session-get","arguments":{"fields":["version"]}}' "$RPC_URL" || true)
if ! printf '%s' "$SESSION_JSON" | grep -q "\"version\":\"${VERSION}"; then
    echo "ERROR: Smoke test FAILED — session-get did not report version ${VERSION}: ${SESSION_JSON}"
    exit 1
fi
echo "    Smoke: session-get reports ${VERSION}."

cleanup_smoke
SMOKE_PID=""
trap - EXIT

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
