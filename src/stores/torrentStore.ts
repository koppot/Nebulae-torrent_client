import { create } from 'zustand';
import { TorrentItem } from '../types';
import { invoke } from '@tauri-apps/api/core';

export interface SystemPathPresets {
  downloads?: string | null;
  movies?: string | null;
  desktop?: string | null;
  documents?: string | null;
}

interface TorrentState {
  torrents: TorrentItem[];
  selectedId: string | null;
  isLoading: boolean;
  fetchTorrents: () => Promise<void>;
  setSelectedId: (id: string | null) => void;
  pauseTorrent: (id: string) => Promise<void>;
  resumeTorrent: (id: string) => Promise<void>;
  removeTorrent: (id: string) => Promise<void>;
  addTorrent: (source: string, downloadDir?: string) => Promise<void>;
  selectTorrentFile: () => Promise<string | null>;
  selectDownloadDirectory: () => Promise<string | null>;
  getSystemPathPresets: () => Promise<SystemPathPresets>;
}

export const useTorrentStore = create<TorrentState>((set, get) => ({
  torrents: [],
  selectedId: null,
  isLoading: false,

  fetchTorrents: async () => {
    set({ isLoading: true });
    try {
      const data = await invoke<TorrentItem[]>('list_torrents');
      set({ torrents: data, isLoading: false });
    } catch {
      set({
        torrents: [],
        isLoading: false,
      });
    }
  },

  setSelectedId: (selectedId) => set({ selectedId }),

  pauseTorrent: async (id: string) => {
    try {
      await invoke('pause_torrent_stub', { id });
    } catch {
      //
    }
    set({
      torrents: get().torrents.map((t) => (t.id === id ? { ...t, status: 'Paused', download_rate: 0, upload_rate: 0 } : t)),
    });
  },

  resumeTorrent: async (id: string) => {
    try {
      await invoke('resume_torrent_stub', { id });
    } catch {
      //
    }
    set({
      torrents: get().torrents.map((t) => (t.id === id ? { ...t, status: 'Downloading' } : t)),
    });
  },

  removeTorrent: async (id: string) => {
    try {
      await invoke('remove_torrent_stub', { id });
    } catch {
      //
    }
    const remaining = get().torrents.filter((t) => t.id !== id);
    set({
      torrents: remaining,
      selectedId: get().selectedId === id ? (remaining[0]?.id || null) : get().selectedId,
    });
  },

  addTorrent: async (source: string, downloadDir?: string) => {
    try {
      await invoke('add_torrent_stub', { source, downloadDir });
      await get().fetchTorrents();
    } catch {
      // Offline/fallback state update
      const newId = `manual-${Date.now()}`;
      const name = source.split('/').pop()?.split('\\').pop() || source;
      set({
        torrents: [
          ...get().torrents,
          {
            id: newId,
            name: name.replace(/\.torrent$/i, ''),
            total_bytes: 1_500_000_000,
            downloaded_bytes: 0,
            uploaded_bytes: 0,
            download_rate: 2_400_000,
            upload_rate: 150_000,
            progress: 0.05,
            status: 'Downloading',
            eta_seconds: 620,
            seeds: 12,
            peers: 5,
            ratio: 0.0,
            category: 'General',
            is_media: false,
          },
        ],
        selectedId: newId,
      });
    }
  },

  selectTorrentFile: async () => {
    try {
      return await invoke<string | null>('select_torrent_file');
    } catch {
      return null;
    }
  },

  selectDownloadDirectory: async () => {
    try {
      return await invoke<string | null>('select_download_directory');
    } catch {
      return null;
    }
  },

  getSystemPathPresets: async () => {
    try {
      return await invoke<SystemPathPresets>('get_system_path_presets');
    } catch {
      return {
        downloads: null,
        movies: null,
        desktop: null,
        documents: null,
      };
    }
  },
}));
