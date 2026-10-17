import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Disc,
  Maximize,
  Minimize,
  Pause,
  Play,
  RotateCcw,
  Volume2,
  VolumeX,
  X,
} from 'lucide-react';
import { closePlayer, openInPlayer, savePositionMs, usePlayerStore } from '../../lib/playerStore';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
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

/**
 * Language-name lookup for the ISO codes stored in the media player
 * preferences (owner: "select languages in advance by using appropriate
 * language codes").
 */
const LANG_NAMES: Record<string, string> = {
  en: 'english', ja: 'japanese', jp: 'japanese', fr: 'french', de: 'german',
  es: 'spanish', it: 'italian', pt: 'portuguese', ru: 'russian', ko: 'korean',
  zh: 'chinese', th: 'thai', nl: 'dutch', sv: 'swedish', no: 'norwegian',
  da: 'danish', fi: 'finnish', pl: 'polish', hi: 'hindi', ar: 'arabic',
};

/** Read the preferences saved by the media screen's preferences modal. */
function readLanguagePrefs(): { audio: string | null; subs: string | null } {
  try {
    return {
      audio: localStorage.getItem('nebulae_pref_audio_langs') || null,
      subs: localStorage.getItem('nebulae_pref_subs_langs') || null,
    };
  } catch {
    return { audio: null, subs: null };
  }
}

/**
 * Best track whose name matches one of the preferred codes (either the raw
 * code, e.g. "en", or the full language name, e.g. "English 5.1"). Returns
 * the track id or null when nothing matches.
 */
function bestTrackFor(tracks: { id: number; name: string }[], prefCsv: string): number | null {
  const codes = prefCsv
    .split(',')
    .map((c) => c.trim().toLowerCase())
    .filter(Boolean);
  if (codes.length === 0) return null;
  for (const code of codes) {
    const name = LANG_NAMES[code] ?? code;
    const hit = tracks.find((t) => {
      const n = (t.name || '').toLowerCase();
      return n.includes(code) || n.includes(name);
    });
    if (hit) return hit.id;
  }
  return null;
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
  // Track popovers (owner): the volume slider and the track lists appear on
  // click on their pill, not permanently in the bar.
  const [showVolume, setShowVolume] = useState(false);
  const [showSubs, setShowSubs] = useState(false);
  const [showAudio, setShowAudio] = useState(false);
  const [subTracks, setSubTracks] = useState<{ id: number; name: string; active: boolean }[]>([]);
  const [audioTracks, setAudioTracks] = useState<{ id: number; name: string; active: boolean }[]>([]);
  const [isFullscreen, setIsFullscreen] = useState(false);
  // Chapter marks: real per-chapter time offsets for the current title.
  const [chapterMarks, setChapterMarks] = useState<{ offset_ms: number; duration_ms: number; name: string }[]>([]);
  const [discChoices, setDiscChoices] = useState<DiscChoice[] | null>(null);
  const lastSaveRef = useRef(0);
  const dragArmedRef = useRef(false);
  // Latest poll/state mirrors for the unified keyboard handler (it must not
  // re-register on every poll tick).
  const pollRef = useRef(poll);
  pollRef.current = poll;
  const volumeRef = useRef(volume);
  volumeRef.current = volume;
  const mutedRef = useRef(muted);
  mutedRef.current = muted;

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

  // Unified keyboard control. Mouse AND keyboard events over the video are
  // captured by the native overlay NSView, which forwards key presses as
  // "player-native-key" events; when the webview itself has focus the same
  // handling runs from the window keydown listener. One handler covers
  // both: disc-menu navigation while the menu domain is active (title 0),
  // and standard playback shortcuts otherwise (space pause, arrows
  // seek/volume, F fullscreen, M mute).
  useEffect(() => {
    if (!player.open) return;
    const nav = (mode: number) => invoke('player_menu_navigate', { mode }).catch(() => {});
    const handle = (key: string) => {
      if (player.isDisc && pollRef.current.title === 0) {
        switch (key) {
          case 'Enter': nav(0); break;
          case 'ArrowUp': nav(1); break;
          case 'ArrowDown': nav(2); break;
          case 'ArrowLeft': nav(3); break;
          case 'ArrowRight': nav(4); break;
        }
        return;
      }
      const t = pollRef.current.time_ms;
      const len = pollRef.current.length_ms;
      switch (key) {
        case ' ':
          invoke('player_toggle_pause').catch(() => {});
          break;
        case 'ArrowLeft':
          invoke('player_seek', { positionMs: Math.max(t - 10000, 0) }).catch(() => {});
          break;
        case 'ArrowRight':
          invoke('player_seek', { positionMs: len > 0 ? Math.min(t + 10000, len) : t + 10000 }).catch(() => {});
          break;
        case 'ArrowUp':
        case 'ArrowDown': {
          const delta = key === 'ArrowUp' ? 10 : -10;
          setMuted(false);
          setVolume((v) => {
            const next = Math.max(0, Math.min(100, v + delta));
            invoke('player_set_volume', { volume: next }).catch(() => {});
            return next;
          });
          break;
        }
        case 'f':
        case 'F': {
          const win = getCurrentWindow();
          win.isFullscreen().then((fs) => win.setFullscreen(!fs)).catch(() => {});
          break;
        }
        case 'm':
        case 'M': {
          if (mutedRef.current) {
            setMuted(false);
            invoke('player_set_volume', { volume: volumeRef.current }).catch(() => {});
          } else {
            setMuted(true);
            invoke('player_set_volume', { volume: 0 }).catch(() => {});
          }
          break;
        }
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const keys = new Set([' ', 'Enter', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'f', 'F', 'm', 'M']);
      if (!keys.has(e.key)) return;
      e.preventDefault();
      handle(e.key);
    };
    window.addEventListener('keydown', onKey, true);
    let unlisten: (() => void) | undefined;
    listen<string>('player-native-key', (e) => handle(e.payload)).then((u) => { unlisten = u; }).catch(() => {});
    return () => {
      window.removeEventListener('keydown', onKey, true);
      unlisten?.();
    };
  }, [player.open, player.isDisc]);

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

  // Chapter marks of the current title, refreshed whenever the title
  // changes (each disc feature has its own chapter layout).
  useEffect(() => {
    if (!player.open) return;
    invoke<{ offset_ms: number; duration_ms: number; name: string }[]>('player_chapter_marks')
      .then((marks) => {
        setChapterMarks(marks);
        if (marks.length > 0) {
          console.log(`[player] chapter marks: ${marks.length} on title ${poll.title}`);
        }
      })
      .catch(() => {});
     
  }, [player.open, poll.title, poll.title_count]);

  // Fullscreen state mirror for the button's icon.
  useEffect(() => {
    const win = getCurrentWindow();
    win.isFullscreen().then(setIsFullscreen).catch(() => {});
    const un = win.onResized(() => {
      win.isFullscreen().then(setIsFullscreen).catch(() => {});
    });
    return () => {
      un.then((f) => f()).catch(() => {});
    };
  }, []);

  // Preferred audio/subtitle languages (owner, "media player preferences"
  // on the media screen): stored by SimpleMediaView, applied automatically
  // once the feature title engages. Codes like "en,ja"; track names carry
  // either the code or the language name.
  useEffect(() => {
    if (!player.open || poll.title === 0) return;
    const prefs = readLanguagePrefs();
    if (!prefs.audio && !prefs.subs) return;
    let cancelled = false;
    const apply = async () => {
      const deadline = Date.now() + 30_000;
      while (!cancelled && Date.now() < deadline) {
        try {
          if (prefs.audio) {
            const tracks = await invoke<{ id: number; name: string }[]>('player_list_tracks', { kind: 'audio' });
            const pick = bestTrackFor(tracks, prefs.audio);
            if (pick != null) await invoke('player_set_track', { kind: 'audio', id: pick });
          }
          if (prefs.subs) {
            const tracks = await invoke<{ id: number; name: string }[]>('player_list_tracks', { kind: 'subtitle' });
            const pick = bestTrackFor(tracks, prefs.subs);
            if (pick != null) await invoke('player_set_track', { kind: 'subtitle', id: pick });
          }
          return;
        } catch {
          await new Promise((r) => setTimeout(r, 1500));
        }
      }
    };
    void apply();
    return () => {
      cancelled = true;
    };
     
  }, [player.open, poll.title]);


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
    console.log('[player] switchDisc click:', path);
    setShowDiscs(false);
    try {
      await openInPlayer(path);
    } catch (err) {
      console.error('[player] switchDisc failed:', err);
    }
  };

  const togglePause = () => {
    invoke('player_toggle_pause').catch(() => {});
  };

  const replay = () => {
    setEnded(false);
    invoke('player_seek', { positionMs: 0 }).catch(() => {});
    if (!poll.playing) invoke('player_toggle_pause').catch(() => {});
  };

  // Chapter jumping (owner): seek to the chapter's REAL time offset taken
  // from the chapter-marks list. set_chapter is unreliable on Blu-ray, but
  // the marks carry the exact offsets. Falls back to set_chapter when the
  // demuxer exposes no offsets (plain files).
  const jumpChapter = (dir: number) => {
    const ch = poll.chapter;
    if (chapterMarks.length > 1) {
      let idx = chapterMarks.findIndex((m) => m.offset_ms > Math.max(poll.time_ms - 2000, 0));
      if (idx < 0) idx = chapterMarks.length;
      const target = dir > 0 ? Math.min(idx, chapterMarks.length - 1) : Math.max(idx - 2, 0);
      invoke('player_seek', { positionMs: Math.max(chapterMarks[target].offset_ms + 1, 0) }).catch(() => {});
    } else {
      const next = dir > 0 ? ch + 1 : Math.max(ch - 1, 0);
      invoke('player_set_chapter', { chapter: next }).catch(() => {});
    }
  };

  // Seek from a pointer position on the custom thin bar.
  const seekFromPointer = (clientX: number) => {
    const el = document.querySelector('[data-testid="seek-track"]') as HTMLElement | null;
    if (!el || dur === 0) return;
    const rect = el.getBoundingClientRect();
    const frac = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    const v = Math.round(frac * dur);
    setSeeking(v);
    invoke('player_seek', { positionMs: v }).catch(() => {});
    window.setTimeout(() => setSeeking(null), 400);
  };

  // Window drag + double-click fullscreen (owner requests): the overlay
  // covers the whole undecorated window, so nothing else offers a drag
  // region. (Superseded by the native NSView subclass in embed_macos.rs,
  // which now performs the drag and fullscreen toggle itself; the webview
  // handlers remain as a fallback for clicks that reach the webview.)
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
      <div className="shrink-0 bg-transparent">
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

          {/* Skip controls: single arrows switch chapters (seeking to the
              chapter's real time offset — set_chapter is unreliable on
              Blu-ray), double arrows switch feature titles with menu and
              short entries skipped (engine-side). */}
          {poll.chapter_count > 1 && (
            <button
              type="button"
              onClick={() => jumpChapter(-1)}
              className="shrink-0 p-1.5 rounded text-[#a3a3a3] hover:text-white hover:bg-[#262626] transition"
              title="Previous chapter"
            >
              <ChevronLeft className="w-4 h-4" />
            </button>
          )}
          {player.isDisc && (
            <button
              type="button"
              onClick={() => invoke('player_step_title', { dir: -1 }).catch(() => {})}
              className="shrink-0 p-1.5 rounded text-[#a3a3a3] hover:text-white hover:bg-[#262626] transition"
              title="Previous episode (feature title)"
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

          {player.isDisc && (
            <button
              type="button"
              onClick={() => invoke('player_step_title', { dir: 1 }).catch(() => {})}
              className="shrink-0 p-1.5 rounded text-[#a3a3a3] hover:text-white hover:bg-[#262626] transition"
              title="Next episode (feature title)"
            >
              <ChevronsRight className="w-4 h-4" />
            </button>
          )}
          {poll.chapter_count > 1 && (
            <button
              type="button"
              onClick={() => jumpChapter(1)}
              className="shrink-0 p-1.5 rounded text-[#a3a3a3] hover:text-white hover:bg-[#262626] transition"
              title="Next chapter"
            >
              <ChevronRight className="w-4 h-4" />
            </button>
          )}

        <span className="text-[11px] font-mono text-[#8c8c8c] tabular-nums">
          {fmt(pos)}
        </span>

        {/* Seek bar (owner): a thin bar that fills gradually as the title
            progresses, with real chapter offsets drawn as vertical ticks.
            Click / drag anywhere on the track to seek. */}
        <div
          className="relative flex-1 flex items-center h-5 group/bar cursor-pointer"
          onPointerDown={(e) => {
            e.currentTarget.setPointerCapture(e.pointerId);
            seekFromPointer(e.clientX);
          }}
          onPointerMove={(e) => {
            if (e.buttons & 1) seekFromPointer(e.clientX);
          }}
        >
          <div
            data-testid="seek-track"
            className="relative w-full h-[3px] rounded-full bg-white/25 overflow-visible"
          >
            {/* Gradual fill */}
            <div
              className="absolute left-0 top-0 h-full rounded-full bg-[#e50914]"
              style={{ width: `${dur > 0 ? Math.min(100, (pos / dur) * 100) : 0}%` }}
            />
            {/* Chapter marks: vertical ticks at the real chapter offsets */}
            {chapterMarks.slice(1).map((mk, i) => (
              <span
                key={i}
                className="absolute top-1/2 -translate-y-1/2 w-[2px] h-[10px] rounded bg-white/80 pointer-events-none"
                style={{ left: `${dur > 0 ? Math.min(100, (mk.offset_ms / dur) * 100) : 0}%` }}
                title={mk.name}
              />
            ))}
            {/* Playhead */}
            <span
              className="absolute top-1/2 -translate-y-1/2 -translate-x-1/2 w-2.5 h-2.5 rounded-full bg-white opacity-0 group-hover/bar:opacity-100 transition"
              style={{ left: `${dur > 0 ? Math.min(100, (pos / dur) * 100) : 0}%` }}
            />
          </div>
        </div>
        <span className="text-[11px] font-mono text-[#8c8c8c] tabular-nums">
          {fmt(dur)}
        </span>

        {/* Volume (owner): only the speaker sits in the bar; the slider
            appears on click. */}
        <div className="relative shrink-0">
          <button
            type="button"
            onClick={() => setShowVolume((v) => !v)}
            className="p-1.5 rounded hover:bg-[#262626] text-white transition"
            title="Volume"
          >
            {muted ? <VolumeX className="w-4 h-4" /> : <Volume2 className="w-4 h-4" />}
          </button>
          {showVolume && (
            <div className="absolute bottom-9 right-0 p-2 rounded-lg bg-[#1a1a1a] border border-[#333333] shadow-2xl shadow-black/60 flex items-center justify-center h-32">
              <input
                type="range"
                min={0}
                max={100}
                value={volume}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  setVolume(v);
                  setMuted(false);
                  invoke('player_set_volume', { volume: v }).catch(() => {});
                }}
                className="accent-[#e50914] w-1 h-24 [writing-mode:vertical-lr] [direction:rtl]"
              />
            </div>
          )}
        </div>

        {/* Subtitles / Audio (owner): pill buttons styled like the disc
            button, with the popover listing the disc's tracks. */}
        <div className="relative shrink-0">
          <button
            type="button"
            onClick={async () => {
              setShowAudio(false);
              if (!showSubs) {
                try { setSubTracks(await invoke('player_list_tracks', { kind: 'subtitle' })); } catch {}
              }
              setShowSubs((v) => !v);
            }}
            className="shrink-0 text-xs text-[#a3a3a3] font-medium truncate border border-[#a3a3a3] rounded-md px-2 py-1 hover:text-white hover:border-white transition text-left"
            title="Subtitles"
          >
            Subs
          </button>
          {showSubs && (
            <div className="absolute bottom-9 right-0 w-56 max-h-52 overflow-y-auto p-1.5 rounded-lg bg-[#1a1a1a] border border-[#333333] shadow-2xl shadow-black/60 space-y-0.5">
              {subTracks.length === 0 && (
                <span className="block px-2 py-1 text-[11px] text-[#666666]">No subtitle tracks</span>
              )}
              {subTracks.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  onClick={() => {
                    invoke('player_set_track', { kind: 'subtitle', id: t.id }).catch(() => {});
                    setSubTracks((ts) => ts.map((x) => ({ ...x, active: x.id === t.id })));
                    setShowSubs(false);
                  }}
                  className={`w-full text-left px-2 py-1 rounded text-[12px] transition ${
                    t.active ? 'bg-[#e50914]/15 text-white' : 'text-[#a3a3a3] hover:text-white hover:bg-[#262626]'
                  }`}
                >
                  {t.name || `Track ${t.id}`}
                </button>
              ))}
            </div>
          )}
        </div>
        <div className="relative shrink-0">
          <button
            type="button"
            onClick={async () => {
              setShowSubs(false);
              if (!showAudio) {
                try { setAudioTracks(await invoke('player_list_tracks', { kind: 'audio' })); } catch {}
              }
              setShowAudio((v) => !v);
            }}
            className="shrink-0 text-xs text-[#a3a3a3] font-medium truncate border border-[#a3a3a3] rounded-md px-2 py-1 hover:text-white hover:border-white transition text-left"
            title="Audio tracks"
          >
            Audio
          </button>
          {showAudio && (
            <div className="absolute bottom-9 right-0 w-56 max-h-52 overflow-y-auto p-1.5 rounded-lg bg-[#1a1a1a] border border-[#333333] shadow-2xl shadow-black/60 space-y-0.5">
              {audioTracks.length === 0 && (
                <span className="block px-2 py-1 text-[11px] text-[#666666]">No audio tracks</span>
              )}
              {audioTracks.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  onClick={() => {
                    invoke('player_set_track', { kind: 'audio', id: t.id }).catch(() => {});
                    setAudioTracks((ts) => ts.map((x) => ({ ...x, active: x.id === t.id })));
                    setShowAudio(false);
                  }}
                  className={`w-full text-left px-2 py-1 rounded text-[12px] transition ${
                    t.active ? 'bg-[#e50914]/15 text-white' : 'text-[#a3a3a3] hover:text-white hover:bg-[#262626]'
                  }`}
                >
                  {t.name || `Track ${t.id}`}
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Fullscreen (owner): explicit button in the bar, right-hand side. */}
        <button
          type="button"
          onClick={() => {
            const win = getCurrentWindow();
            win.setFullscreen(!isFullscreen).catch(() => {});
            setIsFullscreen(!isFullscreen);
          }}
          className="shrink-0 p-1.5 rounded hover:bg-[#262626] text-white transition"
          title={isFullscreen ? 'Exit fullscreen' : 'Fullscreen'}
        >
          {isFullscreen ? <Minimize className="w-4 h-4" /> : <Maximize className="w-4 h-4" />}
        </button>

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
