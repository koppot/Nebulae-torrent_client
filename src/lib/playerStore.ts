import { invoke } from '@tauri-apps/api/core';
import { useSyncExternalStore } from 'react';
import { CloseCoordinator } from './playerSessionFence';

export interface PlayerOpenInfo {
  resolved_path: string;
  title: string;
  is_disc: boolean;
  external: boolean;
  /** Session generation (engine-side). Used to fence stale callbacks that
   *  outlive a user close; absent only from legacy payloads. */
  gen?: number | null;
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
  gen?: number | null;
}

let state: PlayerStoreState = { open: false, title: '', year: null, resolvedPath: '', isDisc: false, gen: null };
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
      gen: info.gen ?? null,
    };
    emit();
  },
  close() {
    invoke('log_frontend', { message: `[frontend-diag] playerStore.close() called` }).catch(() => {});
    state = { ...state, open: false };
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
};

export function usePlayerStore(): PlayerStoreState {
  return useSyncExternalStore(playerStore.subscribe, playerStore.get);
}

/**
 * One coordinator for the app: idempotent close (Q / X / error modal share
 * it) plus the session-generation fence that keeps stale engine callbacks
 * (player-opened repeats, in-flight readiness polls) from reopening the
 * player UI after a close. See playerSessionFence.ts.
 */
const closeCoordinator = new CloseCoordinator({
  invokeClose: () => invoke<number>('player_close'),
  closeStore: () => playerStore.close(),
});

/** Record an engine-advertised session generation (App.tsx adoption paths). */
export function noteEngineGen(gen: number | null | undefined): void {
  closeCoordinator.noteEngineGen(gen);
}

/** True when a callback carries a generation at/below the closed one. */
export function isStaleEngineGen(gen: number | null | undefined): boolean {
  return closeCoordinator.isStaleEngineGen(gen);
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
    // Plain opens are MENU-FIRST: the engine's watcher lands the disc on its
    // interactive menu and stays there. Only an explicit resume skips the
    // menu (engine flag, no race) and seeks once the feature engages.
    const resume = !!opts?.resume;
    const seekTo = opts?.seekTo ?? 0;
    invoke('log_frontend', {
      message: `[frontend-diag] openInPlayer starting: filePath=${filePath} resume=${resume} seekTo=${seekTo}`,
    }).catch(() => {});
    // A close still in flight must land BEFORE the new session starts:
    // player_close stops whatever is live and the commands run concurrently
    // on the backend, so an overlapping close would kill the new session.
    const openEpoch = await closeCoordinator.beginOpen();
    const info = await invoke<PlayerOpenInfo>('player_open', {
      filePath,
      resume,
    });
    invoke('log_frontend', {
      message: `[frontend-diag] openInPlayer resolved: gen=${info.gen} path=${info.resolved_path} is_disc=${info.is_disc} external=${info.external}`,
    }).catch(() => {});
    // Fence anchor: even if no player-opened event or poll has arrived yet,
    // a close right after this point still fences this session's callbacks.
    closeCoordinator.noteEngineGen(info.gen);
  if (!info.external) {
    if (!closeCoordinator.isOpenCurrent(openEpoch)) {
      // The user closed the player while this open was in flight: tear the
      // orphaned session down instead of reopening the UI they dismissed.
      void closeCoordinator.close();
      return info;
    }
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
    if (info.is_disc && resume && seekTo >= 10000) {
      void seekWhenFeatureStarts(seekTo);
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
  void (async () => {
    // The caller (menu Play button) already cancelled the menu watcher and
    // activated the highlighted menu entry; wait for the feature title to
    // engage, then seek to the saved position.
    const deadline = Date.now() + 60_000;
    while (Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, 1000));
      try {
        const p = await invoke<PlayerPollShape>('player_poll');
        if (p.length_ms > 300_000 && p.playing) {
          if (timeMs >= 10000) {
            await invoke('player_seek', { positionMs: timeMs });
          }
          return;
        }
      } catch {
        return; // player closed meanwhile
      }
    }
  })();
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
  // Explicit resume: skip the menu entirely (engine flag) and continue the
  // feature at the saved position.
  return openInPlayer(filePath, { resume: true, seekTo: timeMs, displayTitle, year });
}

/**
 * Wait for the feature title to engage (a real runtime length is reported —
 * menus never have one), then seek to the given position. Must NOT open
 * anything itself — openInPlayer owns the open; routing back through it
 * caused an open/stop recursion that kept destroying the fresh session.
 */
async function seekWhenFeatureStarts(timeMs: number): Promise<void> {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    await new Promise((r) => setTimeout(r, 1000));
    try {
      const p = await invoke<PlayerPollShape>('player_poll');
      // Feature title engaged: non-trivial length and actually playing.
      if (p.length_ms > 300_000 && p.playing) {
        await invoke('player_seek', { positionMs: timeMs });
        return;
      }
    } catch {
      return; // player closed meanwhile
    }
  }
}

/** Saved playback position for a disc (the overlay poller persists it).
 *  When `title` is provided (> 0), reads the title-scoped key strictly (DVD multi-episode);
 *  never falls back to other episodes.
 *  When `title` is 0 or undefined, reads the path-only / title:0 key. */
export function savedPositionMs(discPath: string, title?: number): number {
  try {
    if (title !== undefined) {
      if (title > 0) {
        const v = localStorage.getItem(`nebulae_resume_pos:${discPath}:title:${title}`);
        return v !== null ? (Number(v) || 0) : 0;
      }
      const v0 = localStorage.getItem(`nebulae_resume_pos:${discPath}:title:0`);
      if (v0 !== null) return Number(v0) || 0;
      return Number(localStorage.getItem(`nebulae_resume_pos:${discPath}`) || 0);
    }
    return Number(localStorage.getItem(`nebulae_resume_pos:${discPath}`) || 0);
  } catch {
    return 0;
  }
}

/** Persist a playback position.
 *  Writes the path-only key and the title-scoped key so in-session restore
 *  matches the exact title on multi-title discs. */
export function savePositionMs(discPath: string, timeMs: number, title?: number): void {
  try {
    const v = String(Math.floor(timeMs));
    localStorage.setItem(`nebulae_resume_pos:${discPath}`, v);
    if (title !== undefined) {
      localStorage.setItem(`nebulae_resume_pos:${discPath}:title:${title}`, v);
      localStorage.setItem(`nebulae_resume_title:${discPath}`, String(title));
    }
  } catch {
    // Storage unavailable: resume simply falls back to the disc menu.
  }
}

/** Clear the saved resume position for a specific feature title upon natural completion. */
export function clearFeatureResume(discPath: string, title?: number): void {
  try {
    if (title !== undefined && title > 0) {
      localStorage.removeItem(`nebulae_resume_pos:${discPath}:title:${title}`);
      const savedTitle = localStorage.getItem(`nebulae_resume_title:${discPath}`);
      if (savedTitle === String(title)) {
        localStorage.removeItem(`nebulae_resume_pos:${discPath}`);
        localStorage.removeItem(`nebulae_resume_title:${discPath}`);
      }
    } else {
      localStorage.removeItem(`nebulae_resume_pos:${discPath}`);
      localStorage.removeItem(`nebulae_resume_pos:${discPath}:title:0`);
      localStorage.removeItem(`nebulae_resume_title:${discPath}`);
    }
  } catch {
    // Storage unavailable: no-op.
  }
}

/** Remove all saved positions for a disc — path-only key and all title-scoped
 *  keys stored under the same prefix. */
export function clearPositionMs(discPath: string): void {
  try {
    const prefix = `nebulae_resume_pos:${discPath}`;
    localStorage.removeItem(prefix);
    localStorage.removeItem(`nebulae_resume_title:${discPath}`);
    const toRemove: string[] = [];
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (k && k.startsWith(`${prefix}:title:`)) toRemove.push(k);
    }
    for (const k of toRemove) localStorage.removeItem(k);
  } catch {
    // Storage unavailable: no-op.
  }
}

/**
 * THE shared close path for Q, the player X button and the error modal.
 * Idempotent (concurrent callers share one backend teardown) and trapping-
 * proof (the store closes before the backend is asked, so a slow or stuck
 * teardown can never hold the library UI hostage). Stale engine callbacks
 * are fenced by session generation, so they cannot reopen the player.
 */
export async function closePlayer(): Promise<void> {
  return closeCoordinator.close();
}
