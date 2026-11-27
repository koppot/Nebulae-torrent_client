#!/usr/bin/env bash
# disc-cast-dryrun.sh — reproduce the measured disc-restream chain (Part 6).
#
# Streams a DVD's main-feature title through VLC's own stream output
# (dvdread access -> transcode H.264/AAC -> MPEG-TS over HTTP) and fetches
# bytes from the local HTTP receiver with curl. This is the chain the app's
# disc restream (player_disc_restream_start) uses.
#
# Measured 2026-11-27 on the Glass Fleet D1 ISO (the disc that misreports
# length): a receiver fetched 4,568,964 bytes of valid MPEG-TS carrying
# h264 720x480 + 3 aac audio streams (ffprobe).
#
# Usage:
#   scripts/tests/disc-cast-dryrun.sh <path-to-disc-iso-or-VIDEO_TS-dir> [title#] [port]
#
# Requirements on this Mac: VLC.app, curl, ffprobe (brew). Nothing is
# installed; the receiver is local (127.0.0.1) and the stream is stopped
# after the probe.
set -euo pipefail

DISC="${1:?usage: disc-cast-dryrun.sh <disc-path> [title#] [port]}"
TITLE="${2:-1}"
PORT="${3:-18080}"
VLC_BIN="/Applications/VLC.app/Contents/MacOS/VLC"
OUT="$(mktemp -d /tmp/nebulae-cast-dryrun.XXXXXX)"

[ -x "$VLC_BIN" ] || { echo "FAIL: VLC.app not found at $VLC_BIN" >&2; exit 1; }
command -v curl >/dev/null || { echo "FAIL: curl not found" >&2; exit 1; }
command -v ffprobe >/dev/null || { echo "FAIL: ffprobe not found" >&2; exit 1; }
[ -e "$DISC" ] || { echo "FAIL: disc not found: $DISC" >&2; exit 1; }

echo "==> Streaming '$DISC' title #$TITLE -> http://127.0.0.1:$PORT/nebulae-cast"
"$VLC_BIN" -I dummy --vout none --aout none --no-interact --no-plugins-cache \
    --play-and-exit \
    "dvdread://$DISC#$TITLE" \
    ":sout=#transcode{vcodec=h264,vb=2500,venc=x264{profile=high,level=4.0,preset=veryfast},acodec=mp4a,ab=192,channels=2,samplerate=44100}:std{access=http,mux=ts,dst=127.0.0.1:$PORT/nebulae-cast}" \
    >"$OUT/vlc.log" 2>&1 &
VLC_PID=$!
trap 'kill "$VLC_PID" 2>/dev/null || true; rm -rf "$OUT"' EXIT

sleep 12
PROBE="$OUT/probe.ts"
BYTES=$(curl -s -o "$PROBE" --max-time 20 "http://127.0.0.1:$PORT/nebulae-cast" | wc -c || true)
SIZE=$(stat -f%z "$PROBE" 2>/dev/null || echo 0)
[ "$SIZE" -gt 100000 ] || { echo "FAIL: receiver got only $SIZE bytes" >&2; exit 1; }
echo "==> Receiver fetched $SIZE bytes"
ffprobe -v warning -analyzeduration 20M -probesize 20M \
    -show_entries stream=codec_name,codec_type,width -of csv=p=0 "$PROBE" | sort -u
echo "==> DRY RUN PASS: valid MPEG-TS reached the local receiver"
