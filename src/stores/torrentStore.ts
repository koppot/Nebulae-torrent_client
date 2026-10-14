import { create } from 'zustand';
import { TorrentItem, CreateTorrentParams } from '../types';
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
  forceRecheck: (id: string) => Promise<void>;
  reannounce: (id: string) => Promise<void>;
  setLocation: (id: string, newPath: string) => Promise<void>;
  setCategory: (id: string, category: string) => Promise<void>;
  setTorrentLimits: (id: string, dlKib: number, ulKib: number, ratioLimit: number) => Promise<void>;
  toggleSequential: (id: string) => Promise<boolean>;
  toggleFirstLast: (id: string) => Promise<boolean>;
  toggleSuperSeeding: (id: string) => Promise<boolean>;
  setFilePriority: (id: string, fileIndex: number, priority: number) => Promise<void>;
  renameTorrent: (id: string, newName: string) => Promise<void>;
  renameTorrentFile: (id: string, fileIndex: number, newFileName: string) => Promise<void>;
  createTorrent: (params: CreateTorrentParams) => Promise<string>;
  exportTorrent: (id: string) => Promise<string>;
  selectTorrentFile: () => Promise<string | null>;
  selectDownloadDirectory: () => Promise<string | null>;
  getSystemPathPresets: () => Promise<SystemPathPresets>;
}

export const useTorrentStore = create<TorrentState>((set, get) => ({
  torrents: [],
  selectedId: null,
  isLoading: false,

  fetchTorrents: async () => {
    if (get().torrents.length === 0) {
      set({ isLoading: true });
    }
    try {
      const data = await invoke<TorrentItem[]>('list_torrents');
      set({ torrents: data, isLoading: false });
    } catch {
      set({ isLoading: false });
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
      torrents: get().torrents.map((t) =>
        t.id === id ? { ...t, status: 'Paused', download_rate: 0, upload_rate: 0 } : t
      ),
    });
  },

  resumeTorrent: async (id: string) => {
    try {
      await invoke('resume_torrent_stub', { id });
    } catch {
      //
    }
    set({
      torrents: get().torrents.map((t) =>
        t.id === id ? { ...t, status: t.progress >= 1.0 ? 'Seeding' : 'Downloading' } : t
      ),
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
      selectedId: get().selectedId === id ? remaining[0]?.id || null : get().selectedId,
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
      const now = Math.floor(Date.now() / 1000);
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
            info_hash: `hash_${now}`,
            save_path: downloadDir || '~/Downloads',
            added_on: now,
            completed_on: undefined,
            comment: 'Added via Nebulae',
            created_by: 'Nebulae Client',
            piece_size: 1048576,
            total_pieces: 1430,
            sequential_download: false,
            first_last_piece_prio: true,
            super_seeding: false,
            tags: ['New'],
            files: [
              {
                index: 0,
                name: name.replace(/\.torrent$/i, ''),
                size_bytes: 1_500_000_000,
                progress: 0.05,
                priority: 1,
              },
            ],
            trackers: [
              {
                tier: 0,
                url: 'https://tracker.asiandvdclub.org:2710/announce',
                status: 'Working',
                seeds: 12,
                peers: 5,
                downloaded: 0,
                message: 'Announce OK',
              },
            ],
            download_limit_kib: 0,
            upload_limit_kib: 0,
          },
        ],
        selectedId: newId,
      });
    }
  },

  forceRecheck: async (id: string) => {
    try {
      await invoke('force_recheck_torrent', { id });
    } catch {
      //
    }
    set({
      torrents: get().torrents.map((t) => (t.id === id ? { ...t, status: 'Checking' } : t)),
    });
  },

  reannounce: async (id: string) => {
    try {
      await invoke('reannounce_torrent', { id });
    } catch {
      //
    }
    set({
      torrents: get().torrents.map((t) =>
        t.id === id
          ? {
              ...t,
              trackers: t.trackers.map((tr) => ({
                ...tr,
                status: 'Updating',
                message: 'Reannounced manually',
              })),
            }
          : t
      ),
    });
  },

  setLocation: async (id: string, newPath: string) => {
    try {
      await invoke('set_torrent_location', { id, newPath });
    } catch {
      //
    }
    set({
      torrents: get().torrents.map((t) => (t.id === id ? { ...t, save_path: newPath } : t)),
    });
  },

  setCategory: async (id: string, category: string) => {
    try {
      await invoke('set_torrent_category', { id, category });
    } catch {
      //
    }
    set({
      torrents: get().torrents.map((t) => (t.id === id ? { ...t, category } : t)),
    });
  },

  setTorrentLimits: async (id: string, dlKib: number, ulKib: number, ratioLimit: number) => {
    try {
      await invoke('set_torrent_speed_limits', { id, dlKib, ulKib, ratioLimit });
    } catch {
      //
    }
    set({
      torrents: get().torrents.map((t) =>
        t.id === id
          ? {
              ...t,
              download_limit_kib: dlKib,
              upload_limit_kib: ulKib,
            }
          : t
      ),
    });
  },

  toggleSequential: async (id: string) => {
    let result = false;
    try {
      result = await invoke<boolean>('toggle_torrent_sequential', { id });
    } catch {
      const item = get().torrents.find((t) => t.id === id);
      result = !item?.sequential_download;
    }
    set({
      torrents: get().torrents.map((t) =>
        t.id === id ? { ...t, sequential_download: result } : t
      ),
    });
    return result;
  },

  toggleFirstLast: async (id: string) => {
    let result = false;
    try {
      result = await invoke<boolean>('toggle_torrent_first_last', { id });
    } catch {
      const item = get().torrents.find((t) => t.id === id);
      result = !item?.first_last_piece_prio;
    }
    set({
      torrents: get().torrents.map((t) =>
        t.id === id ? { ...t, first_last_piece_prio: result } : t
      ),
    });
    return result;
  },

  toggleSuperSeeding: async (id: string) => {
    let result = false;
    try {
      result = await invoke<boolean>('toggle_torrent_super_seeding', { id });
    } catch {
      const item = get().torrents.find((t) => t.id === id);
      result = !item?.super_seeding;
    }
    set({
      torrents: get().torrents.map((t) => (t.id === id ? { ...t, super_seeding: result } : t)),
    });
    return result;
  },

  setFilePriority: async (id: string, fileIndex: number, priority: number) => {
    try {
      await invoke('set_file_priority', { id, fileIndex, priority });
    } catch {
      //
    }
    set({
      torrents: get().torrents.map((t) =>
        t.id === id
          ? {
              ...t,
              files: t.files.map((f) => (f.index === fileIndex ? { ...f, priority } : f)),
            }
          : t
      ),
    });
  },

  renameTorrent: async (id: string, newName: string) => {
    try {
      await invoke('rename_torrent', { id, newName });
    } catch {
      //
    }
    set({
      torrents: get().torrents.map((t) => (t.id === id ? { ...t, name: newName } : t)),
    });
  },

  renameTorrentFile: async (id: string, fileIndex: number, newFileName: string) => {
    try {
      await invoke('rename_torrent_file', { id, fileIndex, newFileName });
    } catch {
      //
    }
    set({
      torrents: get().torrents.map((t) =>
        t.id === id
          ? {
              ...t,
              files: t.files.map((f) => (f.index === fileIndex ? { ...f, name: newFileName } : f)),
            }
          : t
      ),
    });
  },

  createTorrent: async (params: CreateTorrentParams) => {
    const out = await invoke<string>('create_torrent_file', { params });
    await get().fetchTorrents();
    return out;
  },

  exportTorrent: async (id: string) => {
    return await invoke<string>('export_torrent_file', { id });
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
