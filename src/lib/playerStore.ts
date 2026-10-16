import { invoke } from '@tauri-apps/api/core';
import { useSyncExternalStore } from 'react';

export interface PlayerOpenInfo {
  resolved_path: string;
  title: string;
  is_disc: boolean;
  external: boolean;
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

export async function closePlayer(): Promise<void> {
  try {
    await invoke('player_close');
  } finally {
    playerStore.close();
  }
}
