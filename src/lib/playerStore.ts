import { invoke } from '@tauri-apps/api/core';
import { useSyncExternalStore } from 'react';
import { useSettingsStore } from '../stores/settingsStore';

export interface PlayerOpenInfo {
  resolved_path: string;
  title: string;
  is_disc: boolean;
  external: boolean;
}

interface PlayerPollShape {
  time_ms: number;
  length_ms: number;
  playing: boolean;
  title: number;
  title_count: number;
  chapter: number;
  chapter_count: number;
  at_menu: boolean;
}

interface PlayerStoreState {
  open: boolean;
  title: string;
  year?: number | string | null;
  resolvedPath: string;
  isDisc: boolean;
}

/** Per-release thumbnail cache: resolvedPath -> (rounded position_ms -> base64 data URL). */
const thumbnailCache = new Map<string, Map<number, string>>();

function cacheKey(positionMs: number): number {
  return Math.round(positionMs / 1000) * 1000; // bucket to nearest second
}

function getCachedThumbnail(resolvedPath: string, positionMs: number): string | null {
  const byPos = thumbnailCache.get(resolvedPath);
  if (!byPos) return null;
  return byPos.get(cacheKey(positionMs)) ?? null;
}

function setCachedThumbnail(resolvedPath: string, positionMs: number, dataUrl: string): void {
  let byPos = thumbnailCache.get(resolvedPath);
  if (!byPos) {
    byPos = new Map();
    thumbnailCache.set(resolvedPath, byPos);
  }
  byPos.set(cacheKey(positionMs), dataUrl);
}

function clearThumbnails(): void {
  thumbnailCache.clear();
}

let state: PlayerStoreState = { open: false, title: '', year: null, resolvedPath: '', isDisc: false };
const listeners = new Set<() => void>();

function emit() {
  for (const fn of listeners) fn();
}

export const playerStore = {
  open(info: PlayerOpenInfo, displayTitle?: string, year?: number | string | null) {
    state = {
      open: true,
      title: displayTitle || info.title,
      year: year ?? null,
      resolvedPath: info.resolved_path,
      isDisc: info.is_disc,
    };
    emit();
  },
  close() {
    state = { ...state, open: false };
    clearThumbnails();
    try {
      if (typeof document !== 'undefined') {
        document.body.classList.remove('player-active');
      }
    } catch {}
    emit();
  },
  subscribe(fn: () => void) {
    listeners.add(fn);
    return () => {
      listeners.delete(fn);
    };
  },
  get() {
    return state;
  },

  /** Read a cached thumbnail for the given playback position (no fetch). */
  getCachedThumbnail(resolvedPath: string, positionMs: number): string | null {
    return getCachedThumbnail(resolvedPath, positionMs);
  },

  /**
   * Fetch a thumbnail for the given playback position, cached per release.
   * Returns a base64 data URL or null if the frame could not be captured.
   */
  async getThumbnail(resolvedPath: string, positionMs: number): Promise<string | null> {
    const cached = getCachedThumbnail(resolvedPath, positionMs);
    if (cached) return cached;
    try {
      const dataUrl = await invoke<string>('player_thumbnail_at', { positionMs });
      setCachedThumbnail(resolvedPath, positionMs, dataUrl);
      return dataUrl;
    } catch {
      return null;
    }
  },
};

export function usePlayerStore(): PlayerStoreState {
  return useSyncExternalStore(playerStore.subscribe, playerStore.get);
}

let isOpening = false;

/**
 * Resolve a save path / file / folder and start playback in the embedded
 * VLC engine. Returns what was resolved so the caller can open the overlay
 * (embedded) or show nothing (external VLC on other platforms).
 */
export async function openInPlayer(
  filePath: string,
  opts?: { resume?: boolean; seekTo?: number; displayTitle?: string; year?: number | string | null },
): Promise<PlayerOpenInfo> {
  if (isOpening) {
    console.warn('[playerStore] openInPlayer ignored: already opening');
    const cur = playerStore.get();
    return {
      resolved_path: cur.resolvedPath || filePath,
      title: cur.title || opts?.displayTitle || filePath,
      is_disc: cur.isDisc,
      external: false,
    };
  }
  isOpening = true;
  try {
    // Plain opens are MENU-FIRST by default: the engine's watcher lands the disc
    // on its interactive menu and stays there. When the user enables the setting
    // "Resume playback from the last saved point instead of menu", check for a
    // saved position and skip the menu straight to the feature if available.
    const resumeSetting = useSettingsStore.getState().settings.resume_playback_from_saved_point;
    const hasExplicitResume = typeof opts?.resume === 'boolean';
    let resume = hasExplicitResume ? !!opts?.resume : false;
    let seekTo = opts?.seekTo ?? 0;

    if (!hasExplicitResume && resumeSetting) {
      const saved = savedPositionMs(filePath);
      if (saved > 0) {
        resume = true;
        seekTo = saved;
      }
    }

    const info = await invoke<PlayerOpenInfo>('player_open', {
      filePath,
      resume,
    });
    if (!hasExplicitResume && resumeSetting && seekTo === 0 && info.resolved_path) {
      const savedResolved = savedPositionMs(info.resolved_path);
      if (savedResolved > 0) {
        seekTo = savedResolved;
      }
    }
  if (!info.external) {
    playerStore.open(info, opts?.displayTitle, opts?.year);
    // Media collection (owner): every externally-opened item is saved as a
    // persistent title, EXCEPT anything inside a downloaded torrent's
    // folder - those already appear as downloaded cards.
    try {
      const { useTorrentStore } = await import('../stores/torrentStore');
      // The torrent's CONTENT lives at save_path/<name>: a broad save_path
      // (e.g. a whole volume) must not swallow external discs that merely
      // sit under the same parent - only items inside the torrent's own
      // download directory count as "already in the library".
      const inDownload = useTorrentStore
        .getState()
        .torrents.some((t) => {
          if (!t.save_path) return false;
          // Normalize to forward slashes for comparison (Windows uses \).
          const norm = (p: string) => p.replace(/\\/g, '/').replace(/\/+$/, '');
          const contentRoot = `${norm(t.save_path)}/${t.name}`;
          const filePath = norm(info.resolved_path);
          // Component-aware check: filePath must be contentRoot itself, or
          // start with contentRoot + separator so that /Downloads-2 or
          // /Downloads/Other do not falsely match /Downloads.
          return (
            filePath === contentRoot ||
            filePath.startsWith(contentRoot + '/')
          );
        });
      if (!inDownload) {
        invoke('add_external_media', {
          path: info.resolved_path,
          title: opts?.displayTitle || info.title,
        }).catch(() => {});
      }
    } catch {
      /* collection is best-effort */
    }
    if (resume && seekTo >= 5000) {
      void seekWhenReady(seekTo, info.is_disc);
    }
  }
  return info;
  } finally {
    isOpening = false;
  }
}

/**
 * Resume WITHOUT reopening: the disc is already open at its menu (the
 * player bar's Play button on the menu). Wait for the feature title to
 * engage, then seek to the saved position.
 */
export async function resumeFromMenu(
  _filePath: string,
  timeMs: number,
): Promise<void> {
  void seekWhenReady(timeMs, true);
}

/**
 * Resume a disc WITHOUT dropping back into its interactive menu position:
 * the engine always opens discs at the menu, so we wait until the feature
 * title actually engages (a real runtime length is reported — menus never
 * have one), then seek to the position saved by the overlay's poller.
 */
export async function resumeInPlayer(
  filePath: string,
  timeMs: number,
  displayTitle?: string,
  year?: number | string | null,
): Promise<PlayerOpenInfo> {
  return openInPlayer(filePath, { resume: true, seekTo: timeMs, displayTitle, year });
}

/**
 * Wait for media playback to begin, then seek to the given position and verify.
 * Handles both plain video files (MKV, MP4) and optical disc features.
 */
async function seekWhenReady(timeMs: number, isDisc: boolean): Promise<void> {
  const deadline = Date.now() + 60_000;
  let seekSent = false;
  let retries = 0;
  while (Date.now() < deadline) {
    await new Promise((r) => setTimeout(r, 600));
    try {
      const p = await invoke<PlayerPollShape>('player_poll');
      const ready = p.playing && p.length_ms > 0 && (!isDisc || !p.at_menu || p.length_ms > 60_000);
      if (ready) {
        if (!seekSent) {
          await invoke('player_seek', { positionMs: timeMs });
          seekSent = true;
          await new Promise((r) => setTimeout(r, 300));
          continue;
        }
        if (Math.abs(p.time_ms - timeMs) <= 5000 || p.time_ms >= timeMs) {
          return;
        }
        retries++;
        if (retries <= 3) {
          await invoke('player_seek', { positionMs: timeMs });
        } else {
          return;
        }
      }
    } catch {
      return;
    }
  }
}

/** Normalize media path so files within BDMV or VIDEO_TS resolve to the release root. */
export function normalizeMediaPath(p: string): string {
  if (!p) return '';
  const norm = p.replace(/\\/g, '/').replace(/\/+$/, '');
  const bdmvIdx = norm.toUpperCase().indexOf('/BDMV');
  if (bdmvIdx > 0) return norm.substring(0, bdmvIdx);
  const videoTsIdx = norm.toUpperCase().indexOf('/VIDEO_TS');
  if (videoTsIdx > 0) return norm.substring(0, videoTsIdx);
  return norm;
}

/** Saved playback position for a release (discs and plain files). */
export function savedPositionMs(filePath: string): number {
  try {
    if (!filePath) return 0;
    const direct = Number(localStorage.getItem(`nebulae_resume_pos:${filePath}`) || 0);
    if (direct > 0) return direct;
    const norm = normalizeMediaPath(filePath);
    if (norm && norm !== filePath) {
      const fromNorm = Number(localStorage.getItem(`nebulae_resume_pos:${norm}`) || 0);
      if (fromNorm > 0) return fromNorm;
    }
    return 0;
  } catch {
    return 0;
  }
}

export function savePositionMs(filePath: string, timeMs: number): void {
  try {
    if (!filePath) return;
    const val = String(Math.floor(timeMs));
    localStorage.setItem(`nebulae_resume_pos:${filePath}`, val);
    const norm = normalizeMediaPath(filePath);
    if (norm && norm !== filePath) {
      localStorage.setItem(`nebulae_resume_pos:${norm}`, val);
    }
  } catch {
    // Storage unavailable
  }
}

export async function closePlayer(): Promise<void> {
  try {
    await invoke('player_close');
  } finally {
    playerStore.close();
  }
}
