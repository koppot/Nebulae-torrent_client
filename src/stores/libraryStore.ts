/**
 * Library store — persists user preferences and custom ordering for the
 * media grid (Features 3, 4, 6).
 *
 * All preferences are backed by Tauri commands that write to SQLite so they
 * survive restarts, rescans, and torrent-client synchronisation.
 */
import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { useTorrentStore } from './torrentStore';

export interface MediaOrderEntry {
  info_hash: string;
  sort_order: number | null;
}

interface LibraryState {
  /** Maximum number of poster columns per row (default 5, range 2–12). */
  postersPerRow: number;
  /** Poster card width in pixels (default 150, range 100–300). */
  posterSize: number;
  /** Gap between poster cards in pixels (default 16, range 4–48). */
  posterSpacing: number;
  /** Whether the Downloaded shelf grid is in custom-ordering mode. */
  customOrderMode: boolean;
  /** Whether the External shelf grid is in custom-ordering mode. */
  externalReorderMode: boolean;
  /** Saved ordered list of downloaded torrent info_hashes. */
  downloadedOrder: string[];
  /** Saved ordered list of external media paths. */
  externalOrder: string[];
  isLoading: boolean;

  // Actions
  loadPreferences: () => Promise<void>;
  setPostersPerRow: (count: number) => Promise<void>;
  setPosterSize: (size: number) => Promise<void>;
  setPosterSpacing: (spacing: number) => Promise<void>;
  setCustomOrderMode: (enabled: boolean) => void;
  setExternalReorderMode: (enabled: boolean) => void;
  saveDownloadedOrder: (hashes: string[]) => Promise<void>;
  saveExternalOrder: (paths: string[]) => Promise<void>;
  saveMediaOrder: (orders: MediaOrderEntry[]) => Promise<void>;
  setUserDisplayTitle: (infoHash: string, title: string) => Promise<void>;
  resetUserDisplayTitle: (infoHash: string) => Promise<void>;
}

export const useLibraryStore = create<LibraryState>((set) => ({
  postersPerRow: 5,
  posterSize: 150,
  posterSpacing: 16,
  customOrderMode: false,
  externalReorderMode: false,
  downloadedOrder: [],
  externalOrder: [],
  isLoading: false,

  loadPreferences: async () => {
    set({ isLoading: true });
    try {
      const [count, size, spacing, downloadedOrder, externalOrder] = await Promise.all([
        invoke<number>('get_posters_per_row').catch(() => 5),
        invoke<number>('get_poster_size').catch(() => 150),
        invoke<number>('get_poster_spacing').catch(() => 16),
        invoke<string[]>('get_downloaded_media_order').catch(() => []),
        invoke<string[]>('get_external_media_order').catch(() => []),
      ]);
      set({
        postersPerRow: count || 5,
        posterSize: size || 150,
        posterSpacing: spacing || 16,
        downloadedOrder: downloadedOrder || [],
        externalOrder: externalOrder || [],
      });
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

  setPosterSize: async (size: number) => {
    const clamped = Math.max(100, Math.min(300, size));
    set({ posterSize: clamped });
    try {
      await invoke('set_poster_size', { size: clamped });
    } catch {
      // Silently keep the in-memory value; will retry on next load.
    }
  },

  setPosterSpacing: async (spacing: number) => {
    const clamped = Math.max(4, Math.min(48, spacing));
    set({ posterSpacing: clamped });
    try {
      await invoke('set_poster_spacing', { spacing: clamped });
    } catch {
      // Silently keep the in-memory value; will retry on next load.
    }
  },

  setCustomOrderMode: (enabled: boolean) => {
    set({ customOrderMode: enabled });
  },

  setExternalReorderMode: (enabled: boolean) => {
    set({ externalReorderMode: enabled });
  },

  saveDownloadedOrder: async (hashes: string[]) => {
    set({ downloadedOrder: hashes });
    const orders: MediaOrderEntry[] = hashes.map((hash, i) => ({
      info_hash: hash,
      sort_order: i,
    }));
    try {
      await invoke('set_media_order', { orders });
      // Update in-memory torrentStore state to maintain sort_order consistency
      const torrents = useTorrentStore.getState().torrents;
      if (torrents && torrents.length > 0) {
        const orderMap = new Map(hashes.map((h, i) => [h.toLowerCase(), i]));
        const updated = torrents.map((t) => ({
          ...t,
          sort_order: orderMap.has(t.info_hash.toLowerCase())
            ? orderMap.get(t.info_hash.toLowerCase())!
            : t.sort_order,
        }));
        useTorrentStore.setState({ torrents: updated });
      }
    } catch {
      // Non-fatal; already reflected in local state
    }
  },

  saveExternalOrder: async (paths: string[]) => {
    set({ externalOrder: paths });
    try {
      await invoke('set_external_media_order', { paths });
    } catch {
      // Non-fatal; already reflected in local state
    }
  },

  saveMediaOrder: async (orders: MediaOrderEntry[]) => {
    try {
      await invoke('set_media_order', { orders });
      const hashes = orders
        .filter((o) => o.sort_order !== null)
        .sort((a, b) => (a.sort_order ?? 0) - (b.sort_order ?? 0))
        .map((o) => o.info_hash);
      set({ downloadedOrder: hashes });
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
