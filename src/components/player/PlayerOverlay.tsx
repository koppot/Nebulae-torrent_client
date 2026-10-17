import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Disc,
  Pause,
  Play,
  RotateCcw,
  Volume2,
  VolumeX,
  X,
} from 'lucide-react';
import { closePlayer, openInPlayer, savePositionMs, usePlayerStore } from '../../lib/playerStore';
import { getCurrentWindow } from '@tauri-apps/api/window';
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

  // DVD menu interaction (keyboard). Mouse clicks on disc menus do NOT
  // reach dvdnav when the video is embedded into the native overlay view
  // (reproduced and measured engine-side), so while the menu domain is
  // active (title 0) the keyboard and the on-screen D-pad drive
  // libvlc_media_player_navigate instead. Enter activates the highlighted
  // menu entry ("Play all", etc.) on the FIRST press; the arrow keys move
  // the highlight.
  useEffect(() => {
    if (!player.open || !player.isDisc || poll.title !== 0) return;
    const nav = (mode: number) => invoke('player_menu_navigate', { mode }).catch(() => {});
    const onKey = (e: KeyboardEvent) => {
      switch (e.key) {
        case 'Enter': e.preventDefault(); nav(0); break;
        case 'ArrowUp': e.preventDefault(); nav(1); break;
        case 'ArrowDown': e.preventDefault(); nav(2); break;
        case 'ArrowLeft': e.preventDefault(); nav(3); break;
        case 'ArrowRight': e.preventDefault(); nav(4); break;
        default: return;
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [player.open, player.isDisc, poll.title]);

  // Disc set of the playing release: names the current disc ("Disc 1") in
  // the control bar and feeds the switcher popup. The set may live several
  // levels above the playing file (e.g. the feature stream sits at
  // <release>/FUUKA_CS_D1/BDMV/STREAM/00015.m2ts, while its sibling disc
  // FUUKA_CS_D2 hangs off the release root) — so walk up the ancestors
  // until a folder reports more than one disc. A single immediate-parent
  // probe only worked for flat multi-ISO layouts (Banner of the Stars).
  useEffect(() => {
    if (!player.open || !player.isDisc) {
      setDiscChoices(null);
      return;
    }
    let alive = true;
    const parts = player.resolvedPath.split('/');
    const tryDir = async (dir: string) => {
      const set = await invoke<DiscStructureInfo[]>('detect_disc_set', { filePath: dir });
      if (!alive) return;
      if (set.length > 1) {
        setDiscChoices(
          set.map((d, idx) => ({
            label: `Disc ${d.disc_number || idx + 1}`,
            detail: `${d.play_target.split('/').pop() || d.play_target} · ${formatBytes(d.total_bytes)}`,
            path: d.play_target,
          })),
        );
        return true;
      }
      return false;
    };
    (async () => {
      for (let i = parts.length - 1; i >= 3; i--) {
        const dir = parts.slice(0, i).join('/');
        try {
          if (await tryDir(dir)) return;
        } catch {
          /* keep walking up */
        }
      }
      if (alive) setDiscChoices(null);
    })();
    return () => {
      alive = false;
    };
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

  // Window drag + double-click fullscreen (owner requests): the overlay
  // covers the whole undecorated window, so nothing else offers a drag
  // region. Mousedown on the video hole starts a native window drag;
  // double-click toggles macOS fullscreen. A drag gesture suppresses the
  // following dblclick so a move is never read as a fullscreen toggle.
  const dragArmedRef = useRef(false);
  const onVideoPointerDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    dragArmedRef.current = true;
    getCurrentWindow().startDragging().catch(() => {});
  };
  const onVideoDoubleClick = () => {
    if (dragArmedRef.current) {
      dragArmedRef.current = false;
      return;
    }
    const win = getCurrentWindow();
    win
      .isFullscreen()
      .then((fs) => win.setFullscreen(!fs))
      .catch(() => {});
  };

  return (
    <div className="fixed inset-0 z-[100] bg-black flex flex-col select-none">
      {/* Video hole: the native overlay shows through this exact rect. */}
      <div
        ref={holderRef}
        className="flex-1 min-h-0"
        onMouseDown={onVideoPointerDown}
        onDoubleClick={onVideoDoubleClick}
      />

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

        <div className="px-4 py-3 flex items-center gap-2.5">
          {/* The current disc, by number (owner: "Disc 1", never a filename),
              bordered in the same color as its text. Clicking it opens the
              disc list (when there is more than one). */}
          <button
            type="button"
            onClick={() => setShowDiscs(!showDiscs)}
            className="shrink-0 text-xs text-[#a3a3a3] font-medium truncate max-w-[240px] border border-[#a3a3a3] rounded-md px-2 py-1 hover:text-white hover:border-white transition text-left"
            title={
              discChoices && discChoices.length > 1
                ? `${discLabel} — click to choose another disc`
                : discLabel
            }
          >
            {discLabel}
          </button>

          {/* Menu: leaves the feature and re-opens the disc's interactive
              menu, in the same gray as the rest of the bar. Always available
              while a disc plays: dvdnav builds report title_count 0, so the
              button must not depend on it; the engine falls back to
              restarting the disc when set_title(0) is ignored. */}
          {player.isDisc && (
            <button
              type="button"
              onClick={() => invoke('player_return_to_menu').catch(() => {})}
              className="shrink-0 text-[10px] uppercase tracking-wider px-2 py-1 rounded-md border border-[#333333] text-[#a3a3a3] hover:bg-[#262626] hover:text-white transition"
              title="Return to the disc's interactive menu"
            >
              Menu
            </button>
          )}

          {/* Disc-menu D-pad. Mouse clicks on the menu itself are not
              delivered to dvdnav in the embedded vout, so while the menu
              domain is active these buttons (and the keyboard) navigate it.
              Enter activates the highlighted entry on the first press. */}
          {player.isDisc && poll.title === 0 && (
            <div className="shrink-0 flex items-center gap-0.5" title="Disc menu navigation — arrow keys work too, Enter selects">
              {([
                ['◀', 3, 'Menu: left'],
                ['▲', 1, 'Menu: up'],
                ['▼', 2, 'Menu: down'],
                ['▶', 4, 'Menu: right'],
              ] as const).map(([glyph, mode, tip]) => (
                <button
                  key={mode}
                  type="button"
                  onClick={() => invoke('player_menu_navigate', { mode }).catch(() => {})}
                  className="w-7 h-7 rounded-md border border-[#333333] text-[#a3a3a3] hover:bg-[#262626] hover:text-white transition text-[11px] leading-none"
                  title={tip}
                >
                  {glyph}
                </button>
              ))}
              <button
                type="button"
                onClick={() => invoke('player_menu_navigate', { mode: 0 }).catch(() => {})}
                className="h-7 px-2.5 rounded-md border border-[#e50914]/60 text-[10px] font-semibold uppercase tracking-wider text-white bg-[#e50914]/20 hover:bg-[#e50914]/40 transition"
                title="Activate the highlighted menu entry (Enter)"
              >
                Select
              </button>
              {/* Play All: jumps straight to the first feature title, bypassing
                  menu navigation entirely — one click, deterministic. */}
              {poll.title_count > 1 && (
                <button
                  type="button"
                  onClick={() => invoke('player_set_title', { title: 1 }).catch(() => {})}
                  className="h-7 px-2.5 rounded-md border border-[#e50914]/60 text-[10px] font-semibold uppercase tracking-wider text-white bg-[#e50914]/20 hover:bg-[#e50914]/40 transition"
                  title="Start playing the feature (title 1) without navigating the menu"
                >
                  Play All
                </button>
              )}
            </div>
          )}

          {/* Skip controls: single arrows switch chapters, double arrows
              switch episodes (DVD titles), owner directive. */}
          {poll.chapter_count > 1 && (
            <button
              type="button"
              onClick={() => invoke('player_set_chapter', { chapter: Math.max(poll.chapter - 1, 0) }).catch(() => {})}
              className="shrink-0 p-1.5 rounded text-[#a3a3a3] hover:text-white hover:bg-[#262626] transition"
              title="Previous chapter"
            >
              <ChevronLeft className="w-4 h-4" />
            </button>
          )}
          {(player.isDisc && (poll.title_count > 1 || poll.title > 1)) && (
            <button
              type="button"
              onClick={() => invoke('player_set_title', { title: Math.max(poll.title - 1, 1) }).catch(() => {})}
              className="shrink-0 p-1.5 rounded text-[#a3a3a3] hover:text-white hover:bg-[#262626] transition"
              title="Previous episode (title)"
            >
              <ChevronsLeft className="w-4 h-4" />
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

          {(player.isDisc && (poll.title_count > 1 || poll.title > 0)) && (
            <button
              type="button"
              onClick={() => invoke('player_set_title', { title: poll.title + 1 }).catch(() => {})}
              className="shrink-0 p-1.5 rounded text-[#a3a3a3] hover:text-white hover:bg-[#262626] transition"
              title="Next episode (title)"
            >
              <ChevronsRight className="w-4 h-4" />
            </button>
          )}
          {poll.chapter_count > 1 && (
            <button
              type="button"
              onClick={() => invoke('player_set_chapter', { chapter: poll.chapter + 1 }).catch(() => {})}
              className="shrink-0 p-1.5 rounded text-[#a3a3a3] hover:text-white hover:bg-[#262626] transition"
              title="Next chapter"
            >
              <ChevronRight className="w-4 h-4" />
            </button>
          )}

        <span className="text-[11px] font-mono text-[#8c8c8c] tabular-nums">
          {fmt(pos)}
        </span>

        {/* Seek bar with chapter ticks (owner: chapters as ticks, not a
            menu). The engine exposes the chapter count but not per-chapter
            offsets, so ticks sit at even intervals of the title length. */}
        <div className="relative flex-1 flex items-center h-4">
          {poll.chapter_count > 1 &&
            Array.from({ length: poll.chapter_count - 1 }, (_, i) => (
              <span
                key={i}
                className="absolute top-1/2 -translate-y-1/2 w-[2px] h-2.5 rounded bg-[#8c8c8c]/70 pointer-events-none"
                style={{ left: `${((i + 1) / poll.chapter_count) * 100}%` }}
              />
            ))}
          <input
            type="range"
            min={0}
            max={dur || 1}
            step={250}
            value={Math.min(pos, dur || 1)}
            onChange={(e) => setSeeking(Number(e.target.value))}
            onMouseUp={(e) => onSeek(Number((e.target as HTMLInputElement).value))}
            onTouchEnd={(e) => onSeek(Number((e.target as HTMLInputElement).value))}
            className="relative w-full accent-[#e50914] h-1"
            disabled={dur === 0}
          />
        </div>
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
