import { create } from 'zustand';
import { TorrentItem } from '../types';
import { invoke } from '@tauri-apps/api/core';

interface TorrentState {
  torrents: TorrentItem[];
  selectedId: string | null;
  isLoading: boolean;
  fetchTorrents: () => Promise<void>;
  setSelectedId: (id: string | null) => void;
  pauseTorrent: (id: string) => Promise<void>;
  resumeTorrent: (id: string) => Promise<void>;
  addTorrent: (source: string, downloadDir?: string) => Promise<void>;
  selectTorrentFile: () => Promise<string | null>;
  selectDownloadDirectory: () => Promise<string | null>;
}

export const useTorrentStore = create<TorrentState>((set, get) => ({
  torrents: [],
  selectedId: 'sample-1',
  isLoading: false,

  fetchTorrents: async () => {
    set({ isLoading: true });
    try {
      const data = await invoke<TorrentItem[]>('list_torrents');
      set({ torrents: data, isLoading: false });
    } catch {
      // Fallback sample mock data for webview dev / browser mode
      set({
        torrents: [
          {
            id: 'sample-1',
            name: 'Tokyo Story (1953) [4K Restoration]',
            total_bytes: 34_500_000_000,
            downloaded_bytes: 34_500_000_000,
            uploaded_bytes: 52_000_000_000,
            download_rate: 0,
            upload_rate: 1_250_000,
            progress: 1.0,
            status: 'Seeding',
            seeds: 42,
            peers: 8,
            ratio: 1.51,
            category: 'Movies',
            is_media: true,
          },
          {
            id: 'sample-2',
            name: 'Seven Samurai (1954) [Criterion Remaster]',
            total_bytes: 48_200_000_000,
            downloaded_bytes: 26_510_000_000,
            uploaded_bytes: 4_300_000_000,
            download_rate: 14_800_000,
            upload_rate: 850_000,
            progress: 0.55,
            status: 'Downloading',
            eta_seconds: 1460,
            seeds: 88,
            peers: 24,
            ratio: 0.16,
            category: 'Movies',
            is_media: true,
          },
          {
            id: 'sample-3',
            name: 'Arch Linux 2026.10 x86_64 Installation ISO',
            total_bytes: 950_000_000,
            downloaded_bytes: 950_000_000,
            uploaded_bytes: 2_100_000_000,
            download_rate: 0,
            upload_rate: 320_000,
            progress: 1.0,
            status: 'Completed',
            seeds: 154,
            peers: 12,
            ratio: 2.21,
            category: 'Software',
            is_media: false,
          },
        ],
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
}));
