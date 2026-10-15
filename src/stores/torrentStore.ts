import { create } from 'zustand';
import { TorrentItem, CreateTorrentParams, InspectTorrentResult, TorrentContentMatch, TorrentRenameEntry } from '../types';
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
  pauseAll: () => Promise<void>;
  resumeAll: () => Promise<void>;
  removeTorrent: (id: string, deleteFiles?: boolean) => Promise<void>;
  addTorrent: (source: string, downloadDir?: string, bytes?: number[]) => Promise<void>;
  inspectTorrent: (source?: string, bytes?: number[]) => Promise<InspectTorrentResult>;
  matchTorrentContent: (source: string, downloadDir: string) => Promise<TorrentContentMatch>;
  applyTorrentRenames: (downloadDir: string, renames: TorrentRenameEntry[]) => Promise<number>;
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
  renameTorrentFolder: (id: string, oldFolderPath: string, newFolderName: string) => Promise<void>;
  createTorrent: (params: CreateTorrentParams) => Promise<string>;
  exportTorrent: (id: string) => Promise<string>;
  selectTorrentFile: () => Promise<string | null>;
  selectSourceFolder: () => Promise<string | null>;
  selectSourceFile: () => Promise<string | null>;
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
      await invoke('pause_torrent', { id });
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
      await invoke('resume_torrent', { id });
    } catch {
      //
    }
    set({
      torrents: get().torrents.map((t) =>
        t.id === id ? { ...t, status: t.progress >= 1.0 ? 'Seeding' : 'Downloading' } : t
      ),
    });
  },

  pauseAll: async () => {
    try {
      await invoke('pause_all_torrents');
    } catch {
      //
    }
    set({
      torrents: get().torrents.map((t) => ({ ...t, status: 'Paused', download_rate: 0, upload_rate: 0 })),
    });
  },

  resumeAll: async () => {
    try {
      await invoke('resume_all_torrents');
    } catch {
      //
    }
    set({
      torrents: get().torrents.map((t) => ({
        ...t,
        status: t.progress >= 1.0 ? 'Seeding' : 'Downloading',
      })),
    });
  },

  removeTorrent: async (id: string, deleteFiles?: boolean) => {
    try {
      await invoke('remove_torrent', { id, deleteFiles: !!deleteFiles });
    } catch (err) {
      console.error('remove_torrent error:', err);
      throw err;
    }
    const remaining = get().torrents.filter((t) => t.id !== id);
    set({
      torrents: remaining,
      selectedId: get().selectedId === id ? remaining[0]?.id || null : get().selectedId,
    });
  },

  addTorrent: async (source: string, downloadDir?: string, bytes?: number[]) => {
    await invoke('add_torrent', { source, downloadDir, bytes });
    await get().fetchTorrents();
  },

  inspectTorrent: async (source?: string, bytes?: number[]) => {
    return await invoke<InspectTorrentResult>('inspect_torrent_file', { source, bytes });
  },

  matchTorrentContent: async (source: string, downloadDir: string) => {
    return await invoke<TorrentContentMatch>('match_torrent_content', { source, downloadDir });
  },

  applyTorrentRenames: async (downloadDir: string, renames: TorrentRenameEntry[]) => {
    return await invoke<number>('apply_torrent_renames', { downloadDir, renames });
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
      await get().fetchTorrents();
    } catch {
      //
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
    }
  },

  renameTorrentFolder: async (id: string, oldFolderPath: string, newFolderName: string) => {
    try {
      await invoke('rename_torrent_folder', { id, oldFolderPath, newFolderName });
      await get().fetchTorrents();
    } catch {
      // Offline fallback
      const oldNorm = oldFolderPath.replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
      const oldPrefix = `${oldNorm}/`;
      set({
        torrents: get().torrents.map((t) => {
          if (t.id !== id) return t;
          return {
            ...t,
            files: t.files.map((f) => {
              const norm = f.name.replace(/\\/g, '/');
              if (norm.startsWith(oldPrefix)) {
                return { ...f, name: `${newFolderName}/${norm.slice(oldPrefix.length)}` };
              }
              return f;
            }),
          };
        }),
      });
    }
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

  selectSourceFolder: async () => {
    try {
      return await invoke<string | null>('select_source_folder');
    } catch {
      return null;
    }
  },

  selectSourceFile: async () => {
    try {
      return await invoke<string | null>('select_source_file');
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
