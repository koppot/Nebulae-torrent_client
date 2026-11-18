#!/bin/bash
set -euo pipefail

# ── Validate required environment ──────────────────────────────────────────

if [ -z "${NEBULAE_TOKEN:-}" ]; then
    echo "ERROR: NEBULAE_TOKEN environment variable is required."
    echo "Usage: docker run -e NEBULAE_TOKEN=your-secret-token nebulae-server"
    exit 1
fi

# ── Configure Transmission ─────────────────────────────────────────────────

TRANSMISSION_HOME="/data/config"
SETTINGS_FILE="${TRANSMISSION_HOME}/settings.json"

# Generate Transmission settings if not present.
if [ ! -f "$SETTINGS_FILE" ]; then
    mkdir -p "$TRANSMISSION_HOME"
    cat > "$SETTINGS_FILE" <<EOF
{
    "download-dir": "/data/downloads",
    "incomplete-dir": "/data/downloads/.incomplete",
    "incomplete-dir-enabled": true,
    "watch-dir": "/data/watch",
    "watch-dir-enabled": true,
    "rpc-enabled": true,
    "rpc-bind-address": "127.0.0.1",
    "rpc-port": 9091,
    "rpc-authentication-required": false,
    "rpc-whitelist-enabled": false,
    "peer-port": 51413,
    "port-forwarding-enabled": true,
    "dht-enabled": true,
    "pex-enabled": true,
    "lpd-enabled": false,
    "encryption": 1,
    "speed-limit-down-enabled": false,
    "speed-limit-up-enabled": false,
    "ratio-limit-enabled": false,
    "umask": 2,
    "cache-size-mb": 16,
    "preallocation": 1,
    "script-torrent-done-enabled": false,
    "start-added-torrents": true,
    "trash-original-torrent-files": false
}
EOF
    echo "Generated default Transmission settings at ${SETTINGS_FILE}"
fi

# Fix permissions.
chown -R nebulae:nebulae /data

# ── Start Transmission daemon ──────────────────────────────────────────────

TRANS_CONFIG_DIR="${TRANSMISSION_HOME}"
TRANS_CMD="transmission-daemon --config-dir ${TRANS_CONFIG_DIR} --foreground --log-info"

echo "Starting Transmission daemon..."
su -s /bin/sh nebulae -c "${TRANS_CMD}" &

TRANSMISSION_PID=$!

# Wait for Transmission RPC to be ready.
echo "Waiting for Transmission RPC..."
for i in $(seq 1 30); do
    if curl -s -o /dev/null http://127.0.0.1:9091/transmission/rpc 2>/dev/null; then
        echo "Transmission RPC is ready."
        break
    fi
    if [ "$i" -eq 30 ]; then
        echo "ERROR: Transmission RPC did not become ready within 30 seconds."
        exit 1
    fi
    sleep 1
done

# ── Start nebulae-server bridge ────────────────────────────────────────────

echo "Starting nebulae-server bridge..."
export NEBULAE_TOKEN NEBULAE_PORT NEBULAE_BIND TRANSMISSION_RPC_URL
exec su -s /bin/sh nebulae -c \
    "exec /usr/local/bin/nebulae-server"
