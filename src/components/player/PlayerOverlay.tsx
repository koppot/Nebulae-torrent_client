import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Pause, Play, RotateCcw, Volume2, VolumeX, X } from 'lucide-react';
import { closePlayer, usePlayerStore } from '../../lib/playerStore';

interface Poll {
  time_ms: number;
  length_ms: number;
  playing: boolean;
}

function fmt(ms: number): string {
  if (!ms || ms < 0) return '0:00:00';
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = h > 0 ? String(m).padStart(2, '0') : String(m);
  return `${h > 0 ? `${h}:` : ''}${mm}:${String(s).padStart(2, '0')}`;
}

/**
 * In-app theater: the native video overlay (created by the Rust side) shows
 * through a transparent hole in this layer; everything around it - the
 * backdrop, the control bar - is Nebulae's own UI. DVD/Blu-ray menus are
 * clickable directly on the video (the overlay receives the clicks natively).
 */
export default function PlayerOverlay() {
  const player = usePlayerStore();
  const holderRef = useRef<HTMLDivElement>(null);
  const [poll, setPoll] = useState<Poll>({ time_ms: 0, length_ms: 0, playing: false });
  const [muted, setMuted] = useState(false);
  const [volume, setVolume] = useState(80);
  const [ended, setEnded] = useState(false);
  const [seeking, setSeeking] = useState<number | null>(null);

  const syncViewport = useCallback(() => {
    const el = holderRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    if (r.width < 4 || r.height < 4) return;
    invoke('player_sync_viewport', {
      x: r.left,
      y: r.top,
      width: r.width,
      height: r.height,
    }).catch(() => {});
  }, []);

  useEffect(() => {
    if (!player.open) return;
    setEnded(false);
    setSeeking(null);

    const apply = (p: Poll) => {
      setPoll(p);
      if (p.length_ms > 0 && !p.playing && p.time_ms >= p.length_ms - 1500) {
        setEnded(true);
      }
    };

    invoke<Poll>('player_poll').then(apply).catch(() => {});
    const iv = window.setInterval(() => {
      invoke<Poll>('player_poll').then(apply).catch(() => {});
    }, 500);
    syncViewport();
    const ro = new ResizeObserver(syncViewport);
    if (holderRef.current) ro.observe(holderRef.current);
    window.addEventListener('resize', syncViewport);
    return () => {
      window.clearInterval(iv);
      ro.disconnect();
      window.removeEventListener('resize', syncViewport);
    };
  }, [player.open, player.resolvedPath, syncViewport]);

  // Track the volume slider once playback is running.
  useEffect(() => {
    if (player.open) {
      invoke('player_set_volume', { volume }).catch(() => {});
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [player.open]);

  if (!player.open) return null;

  const dur = poll.length_ms;
  const pos = seeking ?? poll.time_ms;

  const onSeek = (v: number) => {
    setSeeking(v);
    invoke('player_seek', { positionMs: Math.round(v) }).catch(() => {});
    window.setTimeout(() => setSeeking(null), 400);
  };

  const togglePause = () => {
    invoke('player_toggle_pause').catch(() => {});
  };

  const replay = () => {
    setEnded(false);
    invoke('player_seek', { positionMs: 0 }).catch(() => {});
    if (!poll.playing) invoke('player_toggle_pause').catch(() => {});
  };

  const toggleMute = () => {
    const next = !muted;
    setMuted(next);
    invoke('player_set_volume', { volume: next ? 0 : volume }).catch(() => {});
  };

  return (
    <div className="fixed inset-0 z-[100] bg-black flex flex-col select-none">
      {/* Video hole: the native overlay shows through this exact rect. */}
      <div ref={holderRef} className="flex-1 min-h-0" />

      {ended && (
        <div className="absolute inset-x-0 top-0 flex-1 flex items-center justify-center pointer-events-none">
          <button
            type="button"
            onClick={replay}
            className="pointer-events-auto px-5 py-3 rounded-lg bg-adc-accent hover:bg-adc-accent/90 text-white font-semibold flex items-center gap-2 shadow-2xl transition"
          >
            <RotateCcw className="w-4 h-4" />
            <span>Replay</span>
          </button>
        </div>
      )}

      {/* Nebulae control bar */}
      <div className="shrink-0 px-4 py-3 bg-[#111111] border-t border-[#262626] flex items-center gap-3">
        <span
          className="text-xs text-[#a3a3a3] font-medium truncate max-w-[280px]"
          title={player.title}
        >
          {player.title}
          {player.isDisc && (
            <span className="ml-2 text-[10px] uppercase tracking-wider text-adc-accent">
              Disc Menu — click the video to navigate
            </span>
          )}
        </span>

        <button
          type="button"
          onClick={togglePause}
          className="p-1.5 rounded hover:bg-[#262626] text-white transition"
          title={poll.playing ? 'Pause' : 'Play'}
        >
          {poll.playing ? <Pause className="w-4 h-4" /> : <Play className="w-4 h-4 fill-white" />}
        </button>

        <span className="text-[11px] font-mono text-[#8c8c8c] tabular-nums">
          {fmt(pos)}
        </span>
        <input
          type="range"
          min={0}
          max={dur || 1}
          step={250}
          value={Math.min(pos, dur || 1)}
          onChange={(e) => setSeeking(Number(e.target.value))}
          onMouseUp={(e) => onSeek(Number((e.target as HTMLInputElement).value))}
          onTouchEnd={(e) => onSeek(Number((e.target as HTMLInputElement).value))}
          className="flex-1 accent-[#e50914] h-1"
          disabled={dur === 0}
        />
        <span className="text-[11px] font-mono text-[#8c8c8c] tabular-nums">
          {fmt(dur)}
        </span>

        <button
          type="button"
          onClick={toggleMute}
          className="p-1.5 rounded hover:bg-[#262626] text-white transition"
          title={muted ? 'Unmute' : 'Mute'}
        >
          {muted ? <VolumeX className="w-4 h-4" /> : <Volume2 className="w-4 h-4" />}
        </button>
        <input
          type="range"
          min={0}
          max={100}
          value={volume}
          onChange={(e) => {
            const v = Number(e.target.value);
            setVolume(v);
            if (!muted) invoke('player_set_volume', { volume: v }).catch(() => {});
          }}
          className="w-24 accent-[#e50914] h-1"
        />

        <button
          type="button"
          onClick={() => closePlayer()}
          className="p-1.5 rounded hover:bg-[#3a1010] text-[#e50914] transition"
          title="Stop and close"
        >
          <X className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}
