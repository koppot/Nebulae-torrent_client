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
  Pin,
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
 * Best track for the preferred languages: VLC's own track names are wrong
 * on poorly-authored discs, so match the libbluray probe languages by
 * index FIRST (langs[i] is the language of the i-th real track), and fall
 * back to name matching only when the probe has no data.
 */
function pickTrackFor(
  tracks: { id: number; name: string }[],
  prefCsv: string,
  langs: string[],
): number | null {
  const real = tracks.filter((t) => t.id >= 0);
  const codes = prefCsv
    .split(',')
    .map((c) => c.trim().toLowerCase())
    .filter(Boolean);
  if (codes.length === 0) return null;
  for (const code of codes) {
    const name = LANG_NAMES[code] ?? code;
    // 1) Probe languages by index (authoritative for discs).
    const idx = langs.findIndex((l) => {
      const l0 = l.toLowerCase();
      return l0 === name || l0.startsWith(name);
    });
    if (idx >= 0 && idx < real.length) return real[idx].id;
    // 2) VLC name fallback (plain files).
    const hit = real.find((t) => {
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
  const lastLangProbeRef = useRef('');
  // Single-click vs double-click disambiguation on the video surface.
  const clickTimerRef = useRef<number | null>(null);
  const [isFullscreen, setIsFullscreen] = useState(false);
  // Always-on-top pin (owner): bottom-right mark, click to engage/disengage.
  const [pinnedTop, setPinnedTop] = useState(false);
  // Auto-hide (owner): controls fade out during playback and return on any
  // mouse movement. Kept visible while paused, at the disc menu, on the
  // end screen, or while a popover is open.
  const [controlsVisible, setControlsVisible] = useState(true);
  const hideTimerRef = useRef<number | null>(null);
  // Chapter marks: real per-chapter time offsets for the current title.
  const [chapterMarks, setChapterMarks] = useState<{ offset_ms: number; duration_ms: number; name: string }[]>([]);
  const [discChoices, setDiscChoices] = useState<DiscChoice[] | null>(null);
  // Authored disc-library title (BDMV/META/DL/bdmt_*.xml). Probed once per
  // disc, non-blocking; null while probing or when the disc carries no
  // usable authored name (the label falls back to library metadata).
  const [authoredTitle, setAuthoredTitle] = useState<string | null>(null);
  const discTitleCacheRef = useMemo(() => new Map<string, string | null>(), []);
  const lastSaveRef = useRef(0);
  const dragArmedRef = useRef(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
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
    setAuthoredTitle(null);
    setErrorMessage(null);

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

  // Listen for playback errors (e.g. AACS decryption failure detected by liveness check).
  useEffect(() => {
    if (!player.open) return;
    const unlisten = listen<string>('nebulae://player-error', (event) => {
      setErrorMessage(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, [player.open]);

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
  // both: disc-menu navigation while the menu domain is active (poll.at_menu),
  // and standard playback shortcuts otherwise (space pause, arrows
  // seek/volume, F fullscreen, M mute).
  useEffect(() => {
    if (!player.open) return;
    const nav = (mode: number) => invoke('player_menu_navigate', { mode }).catch(() => {});
    const handle = (key: string) => {
      if (player.isDisc && pollRef.current.at_menu) {
        // In the menu domain, navigation keys route to dvdnav/libbluray.
        // Only intercept these specific keys; all other shortcuts (Q, F, M,
        // volume) must fall through to the standard handler below.
        switch (key) {
          case 'Enter': nav(0); return;
          case 'ArrowUp': nav(1); return;
          case 'ArrowDown': nav(2); return;
          case 'ArrowLeft': nav(3); return;
          case 'ArrowRight': nav(4); return;
        }
      }
      const t = pollRef.current.time_ms;
      const len = pollRef.current.length_ms;
      switch (key) {
        case ' ':
          invoke('player_toggle_pause').catch(() => {});
          break;
        case 'ArrowLeft':
          // Chapter skip (owner): left arrow = previous chapter.
          jumpChapter(-1);
          break;
        case 'ArrowRight':
          // Chapter skip (owner): right arrow = next chapter.
          jumpChapter(1);
          break;
        case 'ArrowUp':
          // 30s forward (owner).
          invoke('player_seek', { positionMs: len > 0 ? Math.min(t + 30000, len) : t + 30000 }).catch(() => {});
          break;
        case 'ArrowDown':
          // 30s back (owner).
          invoke('player_seek', { positionMs: Math.max(t - 30000, 0) }).catch(() => {});
          break;
        case 'q':
        case 'Q':
          // Close (owner): persist the resume point, then close.
          {
            const p = pollRef.current;
            if (!(player.isDisc && p.title === 0) && p.time_ms > 30_000) {
              savePositionMs(player.resolvedPath, p.time_ms);
            }
            void closePlayer();
          }
          break;
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
        case 'AudioVolumeUp':
        case 'AudioVolumeDown': {
          // Dedicated keyboard volume keys (F11/F12 on macOS keyboards).
          const delta = key === 'AudioVolumeUp' ? 10 : -10;
          setMuted(false);
          setVolume((v) => {
            const next = Math.max(0, Math.min(100, v + delta));
            invoke('player_set_volume', { volume: next }).catch(() => {});
            return next;
          });
          break;
        }
        case 'AudioVolumeMute': {
          if (mutedRef.current) {
            setMuted(false);
            invoke('player_set_volume', { volume: volumeRef.current }).catch(() => {});
          } else {
            setMuted(true);
            invoke('player_set_volume', { volume: 0 }).catch(() => {});
          }
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
      const keys = new Set([' ', 'Enter', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'f', 'F', 'm', 'M', 'a', 'A', 's', 'S', 'q', 'Q', 'AudioVolumeUp', 'AudioVolumeDown', 'AudioVolumeMute']);
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
    // Only probe once playback engages with a known feature title or duration,
    // avoiding heavy libbluray network I/O during initial disc menu startup.
    if (!player.open || (player.isDisc && poll.title === 0 && poll.length_ms <= 0)) return;
    // The poll fires every second; only re-probe when the session, title,
    // or duration state (unknown→known) actually changes. The menu→feature
    // transition changes the answer; a duration transition from 0 to a
    // positive value also needs a fresh probe because the title-matching
    // logic uses duration to identify the correct playlist.
    const durKnown = poll.length_ms > 0 ? 'known' : 'unknown';
    const probeKey = `${player.resolvedPath}:${poll.title}:${durKnown}`;
    if (probeKey === lastLangProbeRef.current) return;
    lastLangProbeRef.current = probeKey;
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
  }, [player.open, player.isDisc, poll.title, poll.length_ms > 0, player.resolvedPath]);

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
      // Bound the walk: probe the resolved disc folder itself, and at most
      // its immediate parent folder if the disc folder is e.g. DISC_1 under a release folder.
      // Never walk above the release root into library directories (e.g. Blu-ray, Media, Movies).
      const libraryNames = new Set([
        'blu-ray', 'bluray', 'dvd', 'movies', 'films', 'media', 'cinema',
        'downloads', 'torrents', 'uhd', '4k', 'volumes'
      ]);
      const isLibraryDir = (p: string) => {
        const seg = p.split(/[/\\]/).filter(Boolean).pop()?.toLowerCase() || '';
        return libraryNames.has(seg) || p === '/' || p === '';
      };

      // 1. Probe the disc path itself
      try {
        if (await tryDir(player.resolvedPath)) return;
      } catch {}

      // 2. Probe immediate parent only if not a library root
      const parentDir = parts.slice(0, parts.length - 1).join('/');
      if (parentDir && !isLibraryDir(parentDir) && parts.length > 2) {
        try {
          if (await tryDir(parentDir)) return;
        } catch {}
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
    if (poll.title === 0) {
      setChapterMarks([]);
      return;
    }
    (async () => {
      // For discs, the playlist chapters (libbluray) are the source of
      // truth: VLC's own chapter marks for Blu-ray titles carry offsets
      // that only cover the head of the timeline. VLC's marks are used
      // only when the disc probe yields nothing (plain video files).
      try {
        const disc = await invoke<{ offset_ms: number; duration_ms: number; name: string }[]>(
          'player_disc_chapters',
          { durationMs: Math.round(poll.length_ms) },
        );
        if (disc.length > 1) {
          setChapterMarks(disc);
          return;
        }
      } catch {
        /* fall through to VLC marks */
      }
      try {
        const marks = await invoke<{ offset_ms: number; duration_ms: number; name: string }[]>(
          'player_chapter_marks',
        );
        setChapterMarks(marks);
      } catch {
        /* ticks are best-effort */
      }
    })();
  }, [player.open, poll.title, poll.title_count, poll.length_ms]);

  // Fullscreen state mirror for the button's icon.
  // On macOS the player uses a manual NSWindow frame resize (not the native
  // NSWindowStyleMaskFullScreen), so win.isFullscreen() always returns false.
  // The backend emits player://fullscreen-changed with the authoritative bool.
  // manualFsRef tracks whether we have received at least one such event; if so
  // we suppress the onResized isFullscreen() poll which would reset the state.
  // Windows/Linux use Tauri's native fullscreen and the onResized poll is the
  // correct mechanism there.
  const manualFsRef = useRef(false);
  useEffect(() => {
    const win = getCurrentWindow();
    win.isFullscreen().then(setIsFullscreen).catch(() => {});
    // macOS manual-resize fullscreen authoritative event
    let unlistenNative: (() => void) | undefined;
    listen<boolean>('player://fullscreen-changed', (e) => {
      manualFsRef.current = true;
      setIsFullscreen(e.payload);
    }).then((u) => { unlistenNative = u; }).catch(() => {});
    // Tauri native fullscreen (Windows/Linux): skip poll when the macOS manual
    // path is active — isFullscreen() returns false there and would clobber state.
    const un = win.onResized(() => {
      if (!manualFsRef.current) {
        win.isFullscreen().then(setIsFullscreen).catch(() => {});
      }
    });
    return () => {
      unlistenNative?.();
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
            const langs = trackLangsCache.get(player.resolvedPath)?.audio ?? [];
            const pick = pickTrackFor(tracks, prefs.audio, langs);
            if (pick != null) await invoke('player_set_track', { kind: 'audio', id: pick });
          }
          if (prefs.subs) {
            const tracks = await invoke<{ id: number; name: string }[]>('player_list_tracks', { kind: 'subtitle' });
            const langs = trackLangsCache.get(player.resolvedPath)?.subs ?? [];
            const pick = pickTrackFor(tracks, prefs.subs, langs);
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

  // Authored display-title probe (owner: the upper-left label uses the
  // movie/disc title from disc metadata when available). Non-blocking and
  // cached per resolved path, so disc switches re-probe cleanly without
  // ever touching playback.
  useEffect(() => {
    if (!player.open || !player.isDisc) return;
    const key = player.resolvedPath;
    if (discTitleCacheRef.has(key)) {
      setAuthoredTitle(discTitleCacheRef.get(key) ?? null);
      return;
    }
    let alive = true;
    invoke<{ name: string; lang: string } | null>('player_disc_display_title')
      .then((r) => {
        const name = r && r.name ? r.name : null;
        discTitleCacheRef.set(key, name);
        if (alive) setAuthoredTitle(name);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [player.open, player.isDisc, player.resolvedPath, discTitleCacheRef]);

  // Top-left now playing title & release year (owner instruction):
  // Format: "Title (Year)" when reliable metadata is available; omit if missing.
  const displayTitleInfo = useMemo(() => {
    // Authored disc-library name wins when the disc supplies a meaningful
    // one (owner: the label uses the movie/disc title from disc metadata).
    // The filename/library title is the fallback; generic authored entries
    // like "FUUKA_CS_D1" are rejected by the backend probe. Year still
    // comes only from reliable metadata (the disc XML carries none).
    let clean = (authoredTitle || player.title || '').trim();
    let year = player.year ? String(player.year) : null;
    if (!year) {
      const match = clean.match(/\s*\((\d{4})\)$/);
      if (match) {
        year = match[1];
        clean = clean.replace(/\s*\(\d{4}\)$/, '').trim();
      }
    } else {
      clean = clean.replace(new RegExp(`\\s*\\(${year}\\)$`), '').trim();
    }
    return { title: clean || player.title, year };
  }, [player.title, player.year]);

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
      // Keep the feature label when cycling discs of the same release.
      await openInPlayer(path, { displayTitle: player.title });
    } catch (err) {
      console.error('[player] switchDisc failed:', err);
    }
  };

  // Track display name: the language read from the disc playlists
  // (libbluray) — VLC's own label can be wrong on poorly-authored discs.
  // Just the language (owner): no "Track N" prefix.
  const trackLabel = (kind: 'audio' | 'subtitle', index: number, t: { id: number; name: string }) => {
    const langs = kind === 'audio' ? trackLangs.audio : trackLangs.subs;
    const lang = index >= 0 && index < langs.length ? langs[index] : null;
    return lang || t.name || `Track ${t.id}`;
  };

  const togglePause = () => {
    // At the disc menu the button is PLAY (owner: a pause button has no
    // meaning on a menu): resume from the last save point when one exists,
    // otherwise start the first feature title from the beginning.
    if (player.isDisc && pollRef.current.at_menu) {
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
    if (chapterMarks.length > 1 && dur > 0) {
      let curr = 0;
      for (let i = 0; i < chapterMarks.length; i++) {
        if (chapterMarks[i].offset_ms <= poll.time_ms) {
          curr = i;
        } else {
          break;
        }
      }
      let target: number;
      if (dir > 0) {
        target = Math.min(curr + 1, chapterMarks.length - 1);
      } else {
        const elapsedInChapter = poll.time_ms - chapterMarks[curr].offset_ms;
        target = elapsedInChapter > 2500 ? curr : Math.max(curr - 1, 0);
      }
      const mk = chapterMarks[target];
      invoke('player_seek', { positionMs: Math.max(mk.offset_ms, 0) }).catch(() => {});
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

  // Volume from a pointer position on the horizontal track.
  const volumeFromPointer = (clientX: number) => {
    const el = document.querySelector('[data-testid="volume-track"]') as HTMLElement | null;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const frac = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
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
  // Single left click on the video pauses/resumes (owner). A double click
  // also fires click, so the pause toggle waits 250 ms and is cancelled by
  // the double-click handler.
  const onVideoClick = () => {    if (clickTimerRef.current !== null) return;
    clickTimerRef.current = window.setTimeout(() => {
      clickTimerRef.current = null;
      if (player.isDisc && pollRef.current.title === 0) return;
      togglePause();
    }, 250);
  };
  const onVideoDoubleClick = () => {
    if (clickTimerRef.current !== null) {
      window.clearTimeout(clickTimerRef.current);
      clickTimerRef.current = null;
    }
    if (dragArmedRef.current) {
      dragArmedRef.current = false;
      return;
    }
    // Toggles both ways: window → fullscreen, fullscreen → window.
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
        onClick={onVideoClick}
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
          {player.isDisc && poll.at_menu && (
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
            className="relative flex-1 flex items-center h-3 cursor-pointer"
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
            {/* Chapter marks: vertical ticks at the real chapter offsets.
                Render dark notches that extend past the track, centered on the offset,
                with a white ring for contrast over both played fill and unplayed track. */}
            {dur > 0 &&
              chapterMarks
                .filter((mk) => mk.offset_ms > 0 && mk.offset_ms < dur)
                .map((mk, i) => {
                  const leftPct = Math.min(100, Math.max(0, (mk.offset_ms / dur) * 100));
                  return (
                    <span
                      key={i}
                      className="absolute top-1/2 -translate-x-1/2 -translate-y-1/2 w-[2px] h-3 rounded-[1px] bg-[#1f1f1f] ring-1 ring-white/70 pointer-events-none"
                      style={{ left: `${leftPct}%` }}
                      title={mk.name}
                    />
                  );
                })}
            {/* Playhead */}
            <span
              className="absolute top-1/2 -translate-y-1/2 -translate-x-1/2 w-2.5 h-2.5 rounded-full bg-white opacity-0 group-hover/bar:opacity-100 transition"
              style={{ left: `${dur > 0 ? Math.min(100, (pos / dur) * 100) : 0}%` }}
            />
          </div>
          </div>
        </div>
        <span className="text-[11px] font-mono text-white tabular-nums">
          {fmt(dur)}
        </span>

        {/* Skip-10 buttons (owner): larger, on the right side of the
            progress bar right after the remaining-time label. */}
        <div className="shrink-0 flex items-center gap-1.5">
          <button
            type="button"
            onClick={() => invoke('player_seek', { positionMs: Math.max(pos - 10000, 0) }).catch(() => {})}
            className="relative p-1.5 rounded-full text-white hover:bg-white/15 transition"
            title="Back 10 seconds"
          >
            <RotateCcw className="w-5 h-5" />
            <span className="absolute inset-0 flex items-center justify-center text-[8px] font-bold">10</span>
          </button>
          <button
            type="button"
            onClick={() => invoke('player_seek', { positionMs: dur > 0 ? Math.min(pos + 10000, dur) : pos + 10000 }).catch(() => {})}
            className="relative p-1.5 rounded-full text-white hover:bg-white/15 transition"
            title="Forward 10 seconds"
          >
            <RotateCw className="w-5 h-5" />
            <span className="absolute inset-0 flex items-center justify-center text-[8px] font-bold">10</span>
          </button>
        </div>

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
            /* Horizontal slider, flush against the button's hover box
               (bottom-full): a gap here fires mouseleave while travelling
               to the slider and closes it before the pointer arrives.
               Same style as the bar: semi-transparent, white hairline,
               black glow; round thumb riding the track. */
            <div className="absolute bottom-full right-0 p-2 rounded-lg bg-black/50 border border-white/40 backdrop-blur-sm flex items-center" style={{ filter: 'drop-shadow(0 1px 2px rgba(0,0,0,0.95)) drop-shadow(0 0 6px rgba(0,0,0,0.8))' }}>
              <div
                data-testid="volume-track"
                className="relative w-24 h-[3px] rounded-full bg-white/40 cursor-pointer"
                onPointerDown={(e) => {
                  e.currentTarget.setPointerCapture(e.pointerId);
                  volumeFromPointer(e.clientX);
                }}
                onPointerMove={(e) => {
                  if (e.buttons & 1) volumeFromPointer(e.clientX);
                }}
              >
                <div
                  className="absolute left-0 top-0 h-full rounded-full bg-white"
                  style={{ width: `${volume}%` }}
                />
                <span
                  className="absolute top-1/2 -translate-y-1/2 -translate-x-1/2 w-3 h-3 rounded-full bg-white pointer-events-none"
                  style={{ left: `${volume}%` }}
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
        <div
          className="relative shrink-0"
          onMouseEnter={async () => {
            setShowAudio(false);
            if (!showSubs) {
              try { setSubTracks(await invoke('player_list_tracks', { kind: 'subtitle' })); } catch {}
            }
            setShowSubs(true);
          }}
          onMouseLeave={() => setShowSubs(false)}
        >
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
            /* Flush against the pill (bottom-full) so hovering from the
                button into the list never crosses a closing gap; styled like
                the play bar: semi-transparent, white hairline, black glow. */
            <div
              className="absolute bottom-full left-1/2 -translate-x-1/2 w-max max-w-[260px] max-h-52 overflow-y-auto flex flex-col items-stretch p-1 rounded-lg bg-black/30 border border-white/40 backdrop-blur-sm space-y-0.5"
              style={{ filter: 'drop-shadow(0 1px 2px rgba(0,0,0,0.95)) drop-shadow(0 0 6px rgba(0,0,0,0.8))' }}
            >
              {subTracks.filter((x) => x.id >= 0).length === 0 && (
                <span className="block px-2 py-1 text-xs text-white/60">No subtitle tracks</span>
              )}
              {subTracks.filter((x) => x.id >= 0).map((t) => (
                <button
                  key={t.id}
                  type="button"
                  onClick={() => {
                    invoke('player_set_track', { kind: 'subtitle', id: t.id }).catch(() => {});
                    setSubTracks((ts) => ts.map((x) => ({ ...x, active: x.id === t.id })));
                    setShowSubs(false);
                  }}
                  className={`whitespace-nowrap text-left px-1.5 py-1 rounded text-xs font-medium transition ${
                    t.active ? 'bg-white/15 text-white' : 'text-white/60 hover:text-white hover:bg-white/10'
                  }`}
                >
                  {trackLabel('subtitle', subTracks.filter((x) => x.id >= 0).indexOf(t), t)}
                </button>
              ))}
            </div>
          )}
        </div>
        )}
        <div
          className="relative shrink-0"
          onMouseEnter={async () => {
            setShowSubs(false);
            if (!showAudio) {
              try { setAudioTracks(await invoke('player_list_tracks', { kind: 'audio' })); } catch {}
            }
            setShowAudio(true);
          }}
          onMouseLeave={() => setShowAudio(false)}
        >
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
            <div
              className="absolute bottom-full left-1/2 -translate-x-1/2 w-max max-w-[260px] max-h-52 overflow-y-auto flex flex-col items-stretch p-1 rounded-lg bg-black/30 border border-white/40 backdrop-blur-sm space-y-0.5"
              style={{ filter: 'drop-shadow(0 1px 2px rgba(0,0,0,0.95)) drop-shadow(0 0 6px rgba(0,0,0,0.8))' }}
            >
              {audioTracks.filter((x) => x.id >= 0).length === 0 && (
                <span className="block px-2 py-1 text-xs text-white/60">No audio tracks</span>
              )}
              {audioTracks.filter((x) => x.id >= 0).map((t) => (
                <button
                  key={t.id}
                  type="button"
                  onClick={() => {
                    invoke('player_set_track', { kind: 'audio', id: t.id }).catch(() => {});
                    setAudioTracks((ts) => ts.map((x) => ({ ...x, active: x.id === t.id })));
                    setShowAudio(false);
                  }}
                  className={`whitespace-nowrap text-left px-1.5 py-1 rounded text-xs font-medium transition ${
                    t.active ? 'bg-white/15 text-white' : 'text-white/60 hover:text-white hover:bg-white/10'
                  }`}
                >
                  {trackLabel('audio', audioTracks.filter((x) => x.id >= 0).indexOf(t), t)}
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

        {/* Always-on-top pin (owner): right side of fullscreen. */}
        <button
          type="button"
          onClick={() => {
            invoke<boolean>('player_always_on_top')
              .then((v) => setPinnedTop(v))
              .catch(() => {});
          }}
          className={`shrink-0 p-1.5 rounded hover:bg-[#262626] transition ${
            pinnedTop ? 'text-[#e50914]' : 'text-white'
          }`}
          title={pinnedTop ? 'Disable always on top' : 'Keep window on top'}
        >
          <Pin className={`w-4 h-4 ${pinnedTop ? 'fill-current' : ''}`} />
        </button>
        </div>
      </div>

      {/* Now playing (owner): current title and release year in the top-left corner, fading
          with the controls. */}
      <div
        className={`absolute top-3 left-4 z-20 max-w-[60%] truncate text-sm text-white font-medium transition ${
          controlsVisible ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
        style={{
          filter: 'drop-shadow(0 1px 2px rgba(0,0,0,0.95)) drop-shadow(0 0 6px rgba(0,0,0,0.8))',
        }}
        title={`${displayTitleInfo.title}${displayTitleInfo.year ? ` (${displayTitleInfo.year})` : ''}`}
      >
        <span>{displayTitleInfo.title}</span>
        {displayTitleInfo.year ? (
          <span className="text-[#a3a3a3] font-normal"> ({displayTitleInfo.year})</span>
        ) : null}
      </div>

      {/* Close (owner): white X, pinned to the top-right corner of the
          player window, above the control bar's auto-hide. The native
          traffic lights stay hidden (dead on this borderless window). */}
      <button
        type="button"
        onClick={() => {
          // Persist the final position no matter the throttle/pause state:
          // quitting mid-feature must always leave a resume point.
          const p = pollRef.current;
          if (!(player.isDisc && p.title === 0) && p.time_ms > 30_000) {
            savePositionMs(player.resolvedPath, p.time_ms);
          }
          void closePlayer();
        }}
        className={`absolute top-3 right-4 z-20 p-1.5 rounded-md text-white hover:bg-white/15 transition ${
          controlsVisible ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
        style={{ filter: 'drop-shadow(0 1px 2px rgba(0,0,0,0.95)) drop-shadow(0 0 6px rgba(0,0,0,0.8))' }}
        title="Stop and close"
      >
        <X className="w-5 h-5" />
      </button>

      {/* Playback Error Alert Modal */}
      {errorMessage && (
        <div className="absolute inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-sm">
          <div className="max-w-md w-full bg-[#181818] border border-[#333333] rounded-lg p-5 shadow-2xl space-y-4">
            <div className="flex items-start gap-3">
              <span className="text-[#e50914] text-xl font-bold">⚠️</span>
              <div className="space-y-1">
                <h3 className="text-white text-sm font-semibold">Playback Error</h3>
                <p className="text-xs text-[#a3a3a3] leading-relaxed whitespace-pre-wrap">{errorMessage}</p>
              </div>
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <button
                type="button"
                onClick={() => setErrorMessage(null)}
                className="px-3 py-1.5 text-xs rounded border border-[#333333] text-white hover:bg-[#262626] transition"
              >
                Dismiss
              </button>
              <button
                type="button"
                onClick={() => {
                  setErrorMessage(null);
                  void closePlayer();
                }}
                className="px-3 py-1.5 text-xs rounded bg-[#e50914] text-white hover:bg-[#b80710] font-medium transition"
              >
                Close Player
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
