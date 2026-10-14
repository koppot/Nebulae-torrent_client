#!/bin/sh
# Nebulae Server container entrypoint: start rtorrent detached, wait for its
# SCGI socket, then exec the Nebulae bridge as PID 1's process (under tini).
set -e

DATA_DIR="${DATA_DIR:-/data}"
SOCKET="/run/rtorrent/scgi.sock"

mkdir -p "$DATA_DIR/downloads" "$DATA_DIR/session" "$DATA_DIR/watch" /run/rtorrent
rm -f "$SOCKET" /run/rtorrent/dtach.sock

echo "[entrypoint] starting rtorrent (session: $DATA_DIR/session)"
dtach -n /run/rtorrent/dtach.sock rtorrent

# Wait for the SCGI socket rtorrent creates on startup.
i=0
while [ ! -S "$SOCKET" ] && [ "$i" -lt 50 ]; do
    sleep 0.2
    i=$((i + 1))
done
if [ ! -S "$SOCKET" ]; then
    echo "[entrypoint] FATAL: rtorrent SCGI socket did not appear at $SOCKET" >&2
    exit 1
fi

echo "[entrypoint] rtorrent ready; starting nebulae-server bridge"
exec nebulae-server \
    --engine rtorrent \
    --scgi "$SOCKET" \
    --listen "${NEBULAE_LISTEN:-0.0.0.0:9174}"
