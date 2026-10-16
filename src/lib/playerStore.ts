import { invoke } from '@tauri-apps/api/core';
import { useSyncExternalStore } from 'react';

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
}

interface PlayerStoreState {
  open: boolean;
  title: string;
  resolvedPath: string;
  isDisc: boolean;
}

let state: PlayerStoreState = { open: false, title: '', resolvedPath: '', isDisc: false };
const listeners = new Set<() => void>();

function emit() {
  for (const fn of listeners) fn();
}

export const playerStore = {
  open(info: PlayerOpenInfo) {
    state = {
      open: true,
      title: info.title,
      resolvedPath: info.resolved_path,
      isDisc: info.is_disc,
    };
    emit();
  },
  close() {
    state = { ...state, open: false };
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
 * Resolve a save path / file / folder and start playback in the embedded
 * VLC engine. Returns what was resolved so the caller can open the overlay
 * (embedded) or show nothing (external VLC on other platforms).
 */
export async function openInPlayer(filePath: string): Promise<PlayerOpenInfo> {
  const info = await invoke<PlayerOpenInfo>('player_open', { filePath });
  if (!info.external) {
    playerStore.open(info);
  }
  return info;
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
): Promise<PlayerOpenInfo> {
  const info = await openInPlayer(filePath);
  if (info.external || !timeMs || timeMs < 10000) return info;
  void (async () => {
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
  })();
  return info;
}

/** Saved playback position for a disc (the overlay poller persists it). */
export function savedPositionMs(discPath: string): number {
  try {
    return Number(localStorage.getItem(`nebulae_resume_pos:${discPath}`) || 0);
  } catch {
    return 0;
  }
}

export function savePositionMs(discPath: string, timeMs: number): void {
  try {
    localStorage.setItem(`nebulae_resume_pos:${discPath}`, String(Math.floor(timeMs)));
  } catch {
    // Storage unavailable: resume simply falls back to the disc menu.
  }
}

export async function closePlayer(): Promise<void> {
  try {
    await invoke('player_close');
  } finally {
    playerStore.close();
  }
}
