import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Maximize,
  Minimize,
  Pause,
  Play,
  RotateCcw,
  RotateCw,
  Volume2,
  VolumeX,
  X,
} from 'lucide-react';
import { closePlayer, openInPlayer, resumeFromMenu, savePositionMs, savedPositionMs, usePlayerStore } from '../../lib/playerStore';
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
  // Track popovers (owner): the volume slider and the track lists appear on
  // click on their pill, not permanently in the bar.
  const [showVolume, setShowVolume] = useState(false);
  const [showSubs, setShowSubs] = useState(false);
  const [showAudio, setShowAudio] = useState(false);
  const [subTracks, setSubTracks] = useState<{ id: number; name: string; active: boolean }[]>([]);
  const [audioTracks, setAudioTracks] = useState<{ id: number; name: string; active: boolean }[]>([]);

  // Per-disc cache of the libbluray language probe (module level so it
  // survives menu→feature transitions within a session).
  const trackLangsCacheRef = useMemo(() => new Map<string, { audio: string[]; subs: string[] }>(), []);
  const trackLangsCache = trackLangsCacheRef;
  const [isFullscreen, setIsFullscreen] = useState(false);
  // Auto-hide (owner): controls fade out during playback and return on any
  // mouse movement. Kept visible while paused, at the disc menu, on the
  // end screen, or while a popover is open.
  const [controlsVisible, setControlsVisible] = useState(true);
  const hideTimerRef = useRef<number | null>(null);
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
    // The floating control bar must be excluded from the video overlay so
    // it stays visible and clickable over the video.
    let barHeight = 0;
    const bar = document.querySelector('[data-player-bar]') as HTMLElement | null;
    if (bar) barHeight = Math.max(0, window.innerHeight - bar.getBoundingClientRect().top);
    invoke('player_sync_viewport', {
      x: r.left,
      y: r.top,
      width: r.width,
      height: r.height,
      barHeight,
    }).catch(() => {});
  }, []);

  useEffect(() => {
    if (!player.open) return;
    setEnded(false);
    setSeeking(null);
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
      // Keep the control-bar cutout in sync (bar grows when a popover
      // opens, shrinks when the disc list closes).
      syncViewport();
    }, 500);
    syncViewport();
    const ro = new ResizeObserver(syncViewport);
    if (holderRef.current) ro.observe(holderRef.current);
    const bar = document.querySelector('[data-player-bar]');
    if (bar) ro.observe(bar);
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
          invoke('player_toggle_fullscreen').catch(() => {});
          break;
        }
        case 'a':
        case 'A': {
          void cycleTrack('audio');
          break;
        }
        case 's':
        case 'S': {
          void cycleTrack('subtitle');
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
      const keys = new Set([' ', 'Enter', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'f', 'F', 'm', 'M', 'a', 'A', 's', 'S']);
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

  // True stream languages from the disc playlists (libbluray): VLC labels
  // audio/subtitle tracks from CLIPINFO metadata, which some discs author
  // incorrectly (every stream "English"). Falls back to the libvlc names.
  const [trackLangs, setTrackLangs] = useState<{ audio: string[]; subs: string[] }>({
    audio: [],
    subs: [],
  });
  useEffect(() => {
    if (!player.open || !player.isDisc || poll.title === 0 || poll.length_ms <= 0) return;
    let alive = true;
    const tryFetch = (retriesLeft: number) => {
      invoke<[string[], string[]]>('player_track_languages', {
        durationMs: Math.round(poll.length_ms),
        title: poll.title,
      })
        .then(([a, s]) => {
          if (!alive) return;
          // Only overwrite when the probe actually identified the title;
          // keep the previous (correct) labels otherwise.
          if (a.length > 0 || s.length > 0) {
            trackLangsCache.set(player.resolvedPath, { audio: a, subs: s });
            setTrackLangs({ audio: a, subs: s });
          } else if (retriesLeft > 0) {
            window.setTimeout(() => tryFetch(retriesLeft - 1), 2000);
          }
        })
        .catch(() => {});
    };
    tryFetch(2);
    return () => {
      alive = false;
    };
  }, [player.open, player.isDisc, poll.title, poll.length_ms, player.resolvedPath]);

  // Popover labels fall back to the last successful probe for this disc, so
  // the Audio menu doesn't flash wrong VLC names while a fresh probe runs.
  useEffect(() => {
    if (!player.open || !player.isDisc) return;
    const cached = trackLangsCache.get(player.resolvedPath);
    if (cached) setTrackLangs(cached);
    else setTrackLangs({ audio: [], subs: [] });
  }, [player.open, player.isDisc, player.resolvedPath]);

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

  // Control-bar auto-hide (owner): any mouse movement shows the controls;
  // they fade after 3 s once playback is running. Paused playback, the
  // disc menu, the end screen and open popovers keep them visible.
  const popoverOpen = showSubs || showAudio || showVolume;
  useEffect(() => {
    if (!player.open) return;
    const bump = () => {
      setControlsVisible(true);
      if (hideTimerRef.current) window.clearTimeout(hideTimerRef.current);
      hideTimerRef.current = window.setTimeout(() => setControlsVisible(false), 3000);
    };
    bump();
    window.addEventListener('mousemove', bump);
    return () => {
      window.removeEventListener('mousemove', bump);
      if (hideTimerRef.current) window.clearTimeout(hideTimerRef.current);
    };
  }, [player.open]);
  useEffect(() => {
    if (popoverOpen || !poll.playing || poll.title === 0 || ended) {
      setControlsVisible(true);
      if (hideTimerRef.current) window.clearTimeout(hideTimerRef.current);
    }
  }, [popoverOpen, poll.playing, poll.title, ended]);

  // Subtitle track list for the current title: fetched as soon as the
  // feature engages so the bar can hide the Subtitles pill entirely when
  // the title has no subtitle streams (owner).
  useEffect(() => {
    if (!player.open || poll.title === 0) return;
    let alive = true;
    invoke<{ id: number; name: string }[]>('player_list_tracks', { kind: 'subtitle' })
      .then((tracks) => {
        if (alive) setSubTracks(tracks.map((t) => ({ ...t, active: false })));
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [player.open, poll.title]);

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
    try {
      await openInPlayer(path);
    } catch (err) {
      console.error('[player] switchDisc failed:', err);
    }
  };

  // Track display name: prefer the language read from the disc playlists
  // (libbluray) — VLC's own label can be wrong on poorly-authored discs.
  const trackLabel = (kind: 'audio' | 'subtitle', index: number, t: { id: number; name: string }) => {
    const langs = kind === 'audio' ? trackLangs.audio : trackLangs.subs;
    const lang = index >= 0 && index < langs.length ? langs[index] : null;
    const base = lang ? `Track ${index + 1} (${lang})` : t.name || `Track ${t.id}`;
    return base;
  };

  const togglePause = () => {
    // At the disc menu the button is PLAY (owner: a pause button has no
    // meaning on a menu): resume from the last save point when one exists,
    // otherwise start the first feature title from the beginning.
    if (player.isDisc && pollRef.current.title === 0) {
      // The menu demuxer idles until an entry is activated (set_title is
      // ignored in the menu domain): activate the highlighted entry (Play
      // All), then seek to the save point if one exists.
      const savedMs = savedPositionMs(player.resolvedPath);
      invoke('player_cancel_menu_watch').catch(() => {});
      invoke('player_menu_navigate', { mode: 0 }).catch(() => {});
      if (savedMs >= 10000) {
        resumeFromMenu(player.resolvedPath, savedMs);
      }
      return;
    }
    invoke('player_toggle_pause').catch(() => {});
  };

  // Cycle to the next audio / subtitle track (owner: keys A and S). Wraps
  // around; the fresh track lists also refresh the popover contents.
  const cycleTrack = async (kind: 'audio' | 'subtitle') => {
    try {
      const tracks = await invoke<{ id: number; name: string; active: boolean }[]>(
        'player_list_tracks',
        { kind },
      );
      if (tracks.length === 0) return;
      const idx = tracks.findIndex((t) => t.active);
      const next = tracks[(idx + 1) % tracks.length];
      await invoke('player_set_track', { kind, id: next.id });
      const refreshed = await invoke<{ id: number; name: string; active: boolean }[]>(
        'player_list_tracks',
        { kind },
      );
      if (kind === 'audio') setAudioTracks(refreshed);
      else setSubTracks(refreshed);
    } catch {
      // Track cycling is best-effort.
    }
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

  // Volume from a pointer position on the custom vertical track.
  const volumeFromPointer = (clientY: number) => {
    const el = document.querySelector('[data-testid="volume-track"]') as HTMLElement | null;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const frac = Math.max(0, Math.min(1, 1 - (clientY - rect.top) / rect.height));
    const v = Math.round(frac * 100);
    setVolume(v);
    setMuted(false);
    invoke('player_set_volume', { volume: v }).catch(() => {});
  };

  // Window drag + double-click fullscreen (owner requests): the overlay
  // covers the whole undecorated window, so nothing else offers a drag
  // region. (Superseded by the native NSView subclass in embed_macos.rs,
  // which now performs the drag and fullscreen toggle itself; the webview
  // handlers remain as a fallback for clicks that reach the webview.)
  const onVideoPointerDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    dragArmedRef.current = true;
    invoke('window_begin_drag').catch(() => {});
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
    <div className="fixed inset-0 z-[100] flex flex-col select-none" style={{ background: 'transparent' }}>
      {/* Video hole: the native video view sits BELOW this transparent
          webview, so the video shows through and the control bar floats
          OVER the video (owner: no playbar background). The hole spans the
          entire window; the webview paints only the controls. */}
      <div
        ref={holderRef}
        className="absolute inset-0"
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

      {/* Nebulae control bar: floats over the video — the native video
          view sits BELOW the transparent webview, so the bar, its popovers
          and track menus all render above the video and stay clickable. */}
      <div
        data-player-bar
        className={`absolute inset-x-0 bottom-0 z-10 transition-opacity duration-300 ${
          controlsVisible ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
        // Black glow so the bar stays readable over bright video (the bar
        // itself is background-less and floats over the native video).
        style={{
          filter:
            'drop-shadow(0 1px 2px rgba(0,0,0,0.95)) drop-shadow(0 0 6px rgba(0,0,0,0.8))',
        }}
      >
        <div className="px-4 py-3 flex items-center gap-2.5">
          {/* The current disc, by number (owner: "Disc 1", never a filename),
              bordered in the same color as its text. Clicking it cycles to
              the next disc of the release (owner: no disc-list popup). */}
          <button
            type="button"
            onClick={() => {
              if (discChoices && discChoices.length > 1) {
                const idx = discChoices.findIndex((d) => d === currentDisc);
                const next = discChoices[(idx + 1 + discChoices.length) % discChoices.length];
                void switchDisc(next.path);
              }
            }}
            className="shrink-0 text-xs text-white font-medium truncate max-w-[240px] border border-white rounded-md px-2 py-1 hover:text-white hover:border-white transition text-left"
            title={
              discChoices && discChoices.length > 1
                ? `${discLabel} — click to switch to the next disc`
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
              className="shrink-0 text-xs text-white font-medium px-2 py-1 rounded-md border border-white hover:text-white hover:border-white transition text-left"
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
                  className="w-7 h-7 rounded-md border border-white/70 text-white hover:bg-[#262626] hover:text-white transition text-[11px] leading-none"
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
              {/* (The old "Play All" pill was removed per owner: the regular
                  Play button below starts the feature straight from the
                  menu.) */}
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
              className="shrink-0 p-1.5 rounded text-white hover:text-white hover:bg-[#262626] transition"
              title="Previous chapter"
            >
              <ChevronLeft className="w-4 h-4" />
            </button>
          )}
          {player.isDisc && (
            <button
              type="button"
              onClick={() => invoke('player_step_title', { dir: -1 }).catch(() => {})}
              className="shrink-0 p-1.5 rounded text-white hover:text-white hover:bg-[#262626] transition"
              title="Previous episode (feature title)"
            >
              <ChevronsLeft className="w-4 h-4" />
            </button>
          )}

        <button
          type="button"
          onClick={togglePause}
          className="p-1.5 rounded hover:bg-[#262626] text-white transition"
          title={
            player.isDisc && poll.title === 0
              ? 'Play — resume from the last save point, or start the feature from the beginning'
              : poll.playing
                ? 'Pause'
                : 'Play'
          }
        >
          {poll.playing && !(player.isDisc && poll.title === 0) ? (
            <Pause className="w-4 h-4" />
          ) : (
            <Play className="w-4 h-4 fill-white" />
          )}
        </button>

          {player.isDisc && (
            <button
              type="button"
              onClick={() => invoke('player_step_title', { dir: 1 }).catch(() => {})}
              className="shrink-0 p-1.5 rounded text-white hover:text-white hover:bg-[#262626] transition"
              title="Next episode (feature title)"
            >
              <ChevronsRight className="w-4 h-4" />
            </button>
          )}
          {poll.chapter_count > 1 && (
            <button
              type="button"
              onClick={() => jumpChapter(1)}
              className="shrink-0 p-1.5 rounded text-white hover:text-white hover:bg-[#262626] transition"
              title="Next chapter"
            >
              <ChevronRight className="w-4 h-4" />
            </button>
          )}

        <span className="text-[11px] font-mono text-white tabular-nums">
          {fmt(pos)}
        </span>

        {/* Seek bar (owner): a thin bar that fills gradually as the title
            progresses, with real chapter offsets drawn as vertical ticks.
            Click / drag anywhere on the track to seek. */}
        <div className="relative flex-1 flex items-center group/bar">
          <div
            className="relative flex items-center h-3 cursor-pointer"
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
            className="relative w-full h-[5px] rounded-full bg-white/40 border border-white overflow-visible"
          >
            {/* Gradual fill (owner: grey, not red) */}
            <div
              className="absolute left-0 top-0 h-full rounded-full bg-white"
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
          {/* Skip-10 buttons (owner): sit on the right side of the progress
              bar, on the same midline. */}
          <div className="shrink-0 flex items-center gap-1">
            <button
              type="button"
              onClick={() => invoke('player_seek', { positionMs: Math.max(pos - 10000, 0) }).catch(() => {})}
              className="relative p-1 rounded-full text-white hover:bg-white/10 transition"
              title="Back 10 seconds"
            >
              <RotateCcw className="w-4 h-4" />
              <span className="absolute inset-0 flex items-center justify-center text-[6px] font-bold">10</span>
            </button>
            <button
              type="button"
              onClick={() => invoke('player_seek', { positionMs: dur > 0 ? Math.min(pos + 10000, dur) : pos + 10000 }).catch(() => {})}
              className="relative p-1 rounded-full text-white hover:bg-white/10 transition"
              title="Forward 10 seconds"
            >
              <RotateCw className="w-4 h-4" />
              <span className="absolute inset-0 flex items-center justify-center text-[6px] font-bold">10</span>
            </button>
          </div>
        </div>
        <span className="text-[11px] font-mono text-white tabular-nums">
          {fmt(dur)}
        </span>

        {/* Volume (owner): hover the speaker to reveal the slider; clicking
            the speaker toggles mute. */}
        <div
          className="relative shrink-0"
          onMouseEnter={() => setShowVolume(true)}
          onMouseLeave={() => setShowVolume(false)}
        >
          <button
            type="button"
            onClick={() => {
              // Mute toggle: mute saves the current level, unmute restores it.
              if (mutedRef.current) {
                setMuted(false);
                invoke('player_set_volume', { volume: volumeRef.current }).catch(() => {});
              } else {
                setMuted(true);
                invoke('player_set_volume', { volume: 0 }).catch(() => {});
              }
            }}
            className="p-1.5 rounded hover:bg-[#262626] text-white transition"
            title={muted ? 'Unmute' : 'Mute'}
          >
            {muted ? <VolumeX className="w-4 h-4" /> : <Volume2 className="w-4 h-4" />}
          </button>
          {showVolume && (
            <div className="absolute bottom-9 right-0 p-2 rounded-lg bg-[#1a1a1a] border border-[#333333] shadow-2xl shadow-black/60 flex items-center justify-center h-24">
              {/* Thin vertical track (owner): white line filled with grey,
                  same styling as the seek bar; one horizontal tick marks the
                  halfway level. */}
              <div
                data-testid="volume-track"
                className="relative w-[3px] h-16 rounded-full bg-white/40 cursor-pointer"
                onPointerDown={(e) => {
                  e.currentTarget.setPointerCapture(e.pointerId);
                  volumeFromPointer(e.clientY);
                }}
                onPointerMove={(e) => {
                  if (e.buttons & 1) volumeFromPointer(e.clientY);
                }}
              >
                <span className="absolute left-1/2 -translate-x-1/2 w-[9px] h-[2px] rounded bg-white/80 pointer-events-none" style={{ bottom: '50%' }} />
                <div
                  className="absolute bottom-0 left-0 w-full rounded-full bg-white"
                  style={{ height: `${volume}%` }}
                />
              </div>
            </div>
          )}
        </div>

        {/* Subtitles / Audio (owner): pill buttons styled like the disc
            button, with the popover listing the disc's tracks. The
            Subtitles pill is hidden entirely when the current title has
            no subtitle streams. */}
        {subTracks.length > 0 && (
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
            className="shrink-0 text-xs text-white font-medium truncate w-[86px] text-center border border-white rounded-md px-2 py-1 hover:text-white hover:border-white transition"
            title="Subtitles"
          >
            Subtitles
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
                    t.active ? 'bg-[#e50914]/15 text-white' : 'text-white hover:text-white hover:bg-[#262626]'
                  }`}
                >
                  {trackLabel('audio', audioTracks.indexOf(t), t)}
                </button>
              ))}
            </div>
          )}
        </div>
        )}
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
            className="shrink-0 text-xs text-white font-medium truncate w-[86px] text-center border border-white rounded-md px-2 py-1 hover:text-white hover:border-white transition"
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
                    t.active ? 'bg-[#e50914]/15 text-white' : 'text-white hover:text-white hover:bg-[#262626]'
                  }`}
                >
                  {trackLabel('subtitle', subTracks.filter((x) => x.id >= 0).indexOf(t), t)}
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Fullscreen (owner): explicit button in the bar, right-hand side. */}
        <button
          type="button"
          onClick={() => invoke('player_toggle_fullscreen').catch(() => {})}
          className="shrink-0 p-1.5 rounded hover:bg-[#262626] text-white transition"
          title={isFullscreen ? 'Exit fullscreen' : 'Fullscreen'}
        >
          {isFullscreen ? <Minimize className="w-4 h-4" /> : <Maximize className="w-4 h-4" />}
        </button>
        </div>
      </div>

      {/* Now playing (owner): current title in the top-left corner, fading
          with the controls. */}
      <div
        className={`absolute top-3 left-4 z-20 max-w-[60%] truncate text-sm text-white font-medium transition ${
          controlsVisible ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
        style={{
          filter: 'drop-shadow(0 1px 2px rgba(0,0,0,0.95)) drop-shadow(0 0 6px rgba(0,0,0,0.8))',
        }}
        title={player.title}
      >
        {player.title}
      </div>

      {/* Close (owner): white X, pinned to the top-right corner of the
          player window, above the control bar's auto-hide. */}
      <button
        type="button"
        onClick={() => closePlayer()}
        className={`absolute top-3 right-4 z-20 p-1.5 rounded-md text-white hover:bg-white/15 transition ${
          controlsVisible ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
        title="Stop and close"
      >
        <X className="w-5 h-5" />
      </button>
    </div>
  );
}
