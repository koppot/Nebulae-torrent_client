#!/usr/bin/env bash
# hls-cast-dryrun.sh — measure the HLS disc-casting chain end to end.
#
# 1. VLC (dvdread/file -> transcode H.264/AAC -> livehttp keyframe segments,
#    the exact chain of player/hls_cast.rs build_sout_chain) runs for a few
#    seconds and is stopped with SIGINT (clean exit: segments flushed).
# 2. The app's packager/LAN server (hls_cast.rs, run through the ignored
#    test real_segment_dir_gives_a_valid_playlist) builds the playlist from
#    the PTS inside the segments and serves it on loopback.
# 3. ffmpeg decodes the SERVED playlist (independent of our code) and the
#    decoded duration is compared with the playlist sum.
#
# Usage: scripts/tests/hls-cast-dryrun.sh <mrl-or-path> [seconds=30]
#   e.g. hls-cast-dryrun.sh "dvdread:///Volumes/D/GLASS_FLEET_D1.ISO#1" 30
#        hls-cast-dryrun.sh /tmp/test.mp4 15
# Needs: VLC.app, ffmpeg, a Rust toolchain. Writes only under /tmp.
set -euo pipefail
SRC="${1:?usage: hls-cast-dryrun.sh <mrl-or-path> [seconds]}"
SECS="${2:-30}"
VLC_BIN="/Applications/VLC.app/Contents/MacOS/VLC"
CARGO="${CARGO:-cargo}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$(mktemp -d /tmp/nebulae-hls-dryrun.XXXXXX)"
trap 'rm -rf "$OUT"' EXIT
[ -x "$VLC_BIN" ] || { echo "FAIL: VLC.app not found" >&2; exit 1; }
command -v ffmpeg >/dev/null || { echo "FAIL: ffmpeg not found" >&2; exit 1; }
case "$SRC" in *://*) MRL="$SRC" ;; *) MRL="file://$SRC" ;; esac

CHAIN=":sout=#transcode{vcodec=h264,vb=2500,venc=x264{profile=high,level=4.0,preset=veryfast,keyint=50,min-keyint=25},acodec=mp4a,ab=192,channels=2,samplerate=44100}:std{access=livehttp{seglen=4,delsegs=false,numsegs=0,index=\"$OUT/vlc-index.m3u8\",index-url=seg-########.ts},mux=ts{use-key-frames},dst=\"$OUT/seg-########.ts\"}"
"$VLC_BIN" -I dummy --vout none --aout none --no-interact "$MRL" "$CHAIN" :no-dvdnav-menu >"$OUT/vlc.log" 2>&1 &
PID=$!
sleep "$SECS"
kill -INT "$PID"
for _ in $(seq 1 30); do kill -0 "$PID" 2>/dev/null || break; sleep 1; done
if kill -0 "$PID" 2>/dev/null; then echo "FAIL: VLC did not exit cleanly after SIGINT" >&2; kill -9 "$PID"; exit 1; fi
NSEG=$(ls "$OUT"/seg-*.ts | wc -l | tr -d ' ')
ZERO=$(find "$OUT" -name 'seg-*.ts' -size 0 | wc -l | tr -d ' ')
echo "==> VLC wrote $NSEG segments ($ZERO zero-byte); VLC's own index durations (for comparison):"
grep EXTINF "$OUT/vlc-index.m3u8" | sed 's/#EXTINF://; s/,//' | awk '{n++; if($1<0)neg++; s+=$1} END{printf "    entries=%d negative=%d sum=%.1fs\n", n, neg+0, s}'
[ "$ZERO" = 0 ] || { echo "FAIL: zero-byte segments after a clean exit" >&2; exit 1; }

LINE=$(cd "$ROOT/src-tauri" && NEBULAE_HLS_DIR="$OUT" "$CARGO" test --lib real_segment_dir -- --ignored --nocapture 2>&1 | grep HLS_REAL)
echo "==> $LINE"
SUM=$(echo "$LINE" | sed 's/.*total_s=\([0-9.]*\).*/\1/')
DEC=$(cd "$OUT" && ffmpeg -v error -allowed_extensions ALL -i served.m3u8 -f null - -stats 2>&1 | tr '\r' '\n' | grep -o 'time=[0-9:.]*' | tail -1 | sed 's/time=//')
echo "==> ffmpeg decoded the served playlist to $DEC (playlist sum ${SUM}s)"
# ffmpeg's own clock follows the stream PTS, which is itself unreliable on
# discs with broken timestamps (measured: Glass Fleet D1), so allow 5%.
echo "$DEC" | awk -F: -v sum="$SUM" '{t=$1*3600+$2*60+$3; d=t-sum; if(d<0)d=-d; tol=sum*0.05; if(tol<5)tol=5; if(d>tol){print "FAIL: decoded duration differs from playlist by " d "s (tolerance " tol "s)"; exit 1}}'
echo "==> HLS DRY RUN PASS"
