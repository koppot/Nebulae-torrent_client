import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  ChevronsLeft,
  ChevronsRight,
  Disc,
  List,
  Pause,
  Play,
  RotateCcw,
  Volume2,
  VolumeX,
  X,
} from 'lucide-react';
import { closePlayer, openInPlayer, savePositionMs, usePlayerStore } from '../../lib/playerStore';
import { DiscStructureInfo } from '../../types';
import { formatBytes } from '../../lib/formatters';

interface Poll {
  time_ms: number;
  length_ms: number;
  playing: boolean;
  title: number;
  title_count: number;
  chapter: number;
  chapter_count: number;
}

/** One disc of the playing release, for the switcher popup. */
interface DiscChoice {
  label: string;
  detail: string;
  path: string;
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
  const [poll, setPoll] = useState<Poll>({
    time_ms: 0,
    length_ms: 0,
    playing: false,
    title: 0,
    title_count: 0,
    chapter: 0,
    chapter_count: 0,
  });
  const [muted, setMuted] = useState(false);
  const [volume, setVolume] = useState(80);
  const [ended, setEnded] = useState(false);
  const [seeking, setSeeking] = useState<number | null>(null);
  const [showChapters, setShowChapters] = useState(false);
  const [showDiscs, setShowDiscs] = useState(false);
  const [discChoices, setDiscChoices] = useState<DiscChoice[] | null>(null);
  const lastSaveRef = useRef(0);

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
    setShowChapters(false);
    setShowDiscs(false);
    setDiscChoices(null);

    const apply = (p: Poll) => {
      setPoll(p);
      // Persist the playback position so Resume can skip the disc menu and
      // jump straight back into the feature (throttled; menus/idle excluded).
      const now = Date.now();
      if (p.playing && p.time_ms > 30_000 && now - lastSaveRef.current > 5000) {
        lastSaveRef.current = now;
        savePositionMs(player.resolvedPath, p.time_ms);
      }
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

  // Disc set of the playing release: names the current disc ("Disc 1") in
  // the control bar and feeds the switcher popup. Refetched when the resolved
  // path changes (after a disc switch) so the label follows the playback.
  useEffect(() => {
    if (!player.open || !player.isDisc) {
      setDiscChoices(null);
      return;
    }
    let alive = true;
    const dir = player.resolvedPath.split('/').slice(0, -1).join('/');
    invoke<DiscStructureInfo[]>('detect_disc_set', { filePath: dir })
      .then((set) => {
        if (!alive) return;
        setDiscChoices(
          set.map((d, idx) => ({
            label: `Disc ${d.disc_number || idx + 1}`,
            detail: `${d.play_target.split('/').pop() || d.play_target} · ${formatBytes(d.total_bytes)}`,
            path: d.play_target,
          })),
        );
      })
      .catch(() => {
        if (alive) setDiscChoices(null);
      });
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [player.open, player.isDisc, player.resolvedPath]);

  if (!player.open) return null;

  const dur = poll.length_ms;
  const pos = seeking ?? poll.time_ms;

  // The current disc by number (owner: "Disc 1", never a filename). Matched
  // against the resolved playback target; resolve_playback may hand back the
  // disc FOLDER while play_target is the ISO inside it, so match either way.
  const currentDisc = discChoices?.find(
    (d) => d.path.startsWith(player.resolvedPath) || player.resolvedPath.startsWith(d.path),
  );
  const discLabel = player.isDisc ? currentDisc?.label || 'Disc 1' : player.title;

  const switchDisc = async (path: string) => {
    setShowDiscs(false);
    try {
      await openInPlayer(path);
    } catch {
      // The overlay stays; the failed open is logged engine-side.
    }
  };

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

  const chapters: number[] = [];
  if (poll.chapter_count > 1) {
    for (let i = 0; i < Math.min(poll.chapter_count, 80); i++) chapters.push(i);
  }

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

      {/* Nebulae control bar. Popups render INSIDE the bar, as a row above
          the buttons: the native video overlay covers the whole webview area
          above this bar, so absolutely-positioned popovers there render
          BEHIND the video (invisible, unclickable). When a popup opens the
          bar grows, the video hole shrinks, and the ResizeObserver re-syncs
          the native view to the smaller rect. */}
      <div className="shrink-0 bg-[#111111] border-t border-[#262626]">
        {/* Chapters row. */}
        {showChapters && chapters.length > 0 && (
          <div className="px-4 py-3 border-b border-[#262626] max-h-56 overflow-y-auto">
            <div className="flex items-center justify-between pb-2 border-b border-[#262626]">
              <span className="text-[11px] font-semibold text-white flex items-center gap-1.5">
                <List className="w-3.5 h-3.5 text-[#e50914]" />
                Chapters — Title {poll.title}
              </span>
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  onClick={() => invoke('player_set_chapter', { chapter: Math.max(poll.chapter - 1, 0) }).catch(() => {})}
                  className="p-1 rounded hover:bg-[#262626] text-[#a3a3a3] hover:text-white transition"
                  title="Previous chapter"
                >
                  <ChevronsLeft className="w-3.5 h-3.5" />
                </button>
                <button
                  type="button"
                  onClick={() => invoke('player_set_chapter', { chapter: Math.min(poll.chapter + 1, poll.chapter_count - 1) }).catch(() => {})}
                  className="p-1 rounded hover:bg-[#262626] text-[#a3a3a3] hover:text-white transition"
                  title="Next chapter"
                >
                  <ChevronsRight className="w-3.5 h-3.5" />
                </button>
              </div>
            </div>
            <div className="grid grid-cols-8 gap-1.5 pt-2">
              {chapters.map((c) => (
                <button
                  key={c}
                  type="button"
                  onClick={() => {
                    invoke('player_set_chapter', { chapter: c }).catch(() => {});
                    setShowChapters(false);
                  }}
                  className={`text-[10px] font-mono py-1 rounded transition ${
                    c === poll.chapter
                      ? 'bg-[#e50914] text-white font-bold'
                      : 'bg-[#1f1f1f] border border-[#333333] text-[#a3a3a3] hover:text-white'
                  }`}
                >
                  {c + 1}
                </button>
              ))}
            </div>
          </div>
        )}

        {/* Disc switcher row: every disc of the playing release, by number. */}
        {showDiscs && discChoices && discChoices.length > 1 && (
          <div className="px-4 py-3 border-b border-[#262626] max-h-56 overflow-y-auto space-y-1">
            <span className="text-[11px] font-semibold text-white flex items-center gap-1.5 pb-1 border-b border-[#262626]">
              <Disc className="w-3.5 h-3.5 text-[#e50914]" />
              Select disc
            </span>
            {discChoices.map((d) => (
              <button
                key={d.path}
                type="button"
                onClick={() => switchDisc(d.path)}
                className={`w-full text-left px-2 py-1.5 rounded transition ${
                  d === currentDisc
                    ? 'bg-[#e50914]/15 text-white'
                    : 'text-[#a3a3a3] hover:text-white hover:bg-[#262626]'
                }`}
              >
                <span className="text-[12px] font-semibold">{d.label}</span>
                <span className="block text-[9px] font-mono text-[#737373]">{d.detail}</span>
              </button>
            ))}
          </div>
        )}

        <div className="px-4 py-3 flex items-center gap-3">
          {/* The current disc, by number (owner: "Disc 1", never a filename).
              Clicking it opens the disc list (when there is more than one). */}
          <button
            type="button"
            onClick={() => setShowDiscs(!showDiscs)}
            className="text-xs text-[#a3a3a3] font-medium truncate max-w-[280px] hover:text-white transition text-left"
            title={
              discChoices && discChoices.length > 1
                ? `${discLabel} — click to choose another disc`
                : discLabel
            }
          >
            {discLabel}
          </button>

          {/* Disc Menu: a real button — leaves the feature and re-opens the
              interactive menu. Always available while a disc plays: dvdnav
              builds report title_count 0, so the button must not depend on
              it; the engine falls back to restarting the disc when
              set_title(0) is ignored. */}
          {player.isDisc && (
            <button
              type="button"
              onClick={() => invoke('player_return_to_menu').catch(() => {})}
              className="shrink-0 text-[10px] uppercase tracking-wider px-2 py-1 rounded border border-adc-accent/50 text-adc-accent hover:bg-adc-accent/10 transition"
              title="Return to the disc's interactive menu"
            >
              Disc Menu
            </button>
          )}

          {poll.chapter_count > 1 && (
            <button
              type="button"
              onClick={() => setShowChapters(!showChapters)}
              className={`shrink-0 p-1.5 rounded transition ${
                showChapters ? 'bg-[#262626] text-white' : 'text-[#a3a3a3] hover:text-white hover:bg-[#262626]'
              }`}
              title={`Chapters (${poll.chapter_count})`}
            >
              <List className="w-4 h-4" />
            </button>
          )}

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
    </div>
  );
}
