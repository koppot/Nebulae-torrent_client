/**
 * Library store — persists user preferences and custom ordering for the
 * media grid (Features 3, 4, 6).
 *
 * All preferences are backed by Tauri commands that write to SQLite so they
 * survive restarts, rescans, and torrent-client synchronisation.
 */
import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

interface MediaOrderEntry {
  info_hash: string;
  sort_order: number | null;
}

interface LibraryState {
  /** Number of poster columns (default 5, range 2–12). */
  postersPerRow: number;
  /** Whether the grid is in custom-ordering mode. */
  customOrderMode: boolean;
  isLoading: boolean;

  // Actions
  loadPreferences: () => Promise<void>;
  setPostersPerRow: (count: number) => Promise<void>;
  setCustomOrderMode: (enabled: boolean) => void;
  saveMediaOrder: (orders: MediaOrderEntry[]) => Promise<void>;
  setUserDisplayTitle: (infoHash: string, title: string) => Promise<void>;
  resetUserDisplayTitle: (infoHash: string) => Promise<void>;
}

export const useLibraryStore = create<LibraryState>((set) => ({
  postersPerRow: 5,
  customOrderMode: false,
  isLoading: false,

  loadPreferences: async () => {
    set({ isLoading: true });
    try {
      const count = await invoke<number>('get_posters_per_row');
      set({ postersPerRow: count });
    } catch {
      // Leave defaults.
    } finally {
      set({ isLoading: false });
    }
  },

  setPostersPerRow: async (count: number) => {
    const clamped = Math.max(2, Math.min(12, count));
    set({ postersPerRow: clamped });
    try {
      await invoke('set_posters_per_row', { count: clamped });
    } catch {
      // Silently keep the in-memory value; will retry on next load.
    }
  },

  setCustomOrderMode: (enabled: boolean) => {
    set({ customOrderMode: enabled });
  },

  saveMediaOrder: async (orders: MediaOrderEntry[]) => {
    try {
      await invoke('set_media_order', { orders });
    } catch {
      // Non-fatal; the UI reorder is already reflected in local state.
    }
  },

  setUserDisplayTitle: async (infoHash: string, title: string) => {
    await invoke('set_user_display_title', { infoHash, title });
  },

  resetUserDisplayTitle: async (infoHash: string) => {
    await invoke('reset_user_display_title', { infoHash });
  },
}));
