import { create } from 'zustand';
import { TorrentItem, CreateTorrentParams, InspectTorrentResult, TorrentContentMatch, TorrentRenameEntry } from '../types';
import { invoke } from '@tauri-apps/api/core';

// Display-side speed presentation (owner directive 2026-10-20): the Manager
// shows the AVERAGE rate since the respective transfer began - total bytes
// transferred over cumulative ACTIVE seconds - never a rolling or windowed
// average of instantaneous rates. The values come straight from the engine's
// lifetime counters (Transmission downloadedEver/secondsDownloading and
// uploadedEver/secondsSeeding), so they are stable across app restarts, do
// not flicker, and settle to a plain 0 B/s for a transfer with no active
// time yet. Engines that cannot report active seconds (older remote server
// builds) fall back to wall-clock time since the torrent was added, which
// keeps the number a true since-start average.
function lifetimeAverage(bytes: number, activeSeconds: number, addedOn: number, nowMs: number): number {
  if (activeSeconds > 0) return bytes / activeSeconds;
  if (bytes > 0 && addedOn > 0) {
    const wallSeconds = Math.max(1, Math.floor(nowMs / 1000 - addedOn));
    return bytes / wallSeconds;
  }
  return 0;
}

interface RecheckEntry {
  requestedAt: number;
  seenChecking: boolean;
}
const activeRechecks = new Map<string, RecheckEntry>();

export function reconcileRecheckState(torrents: TorrentItem[]): TorrentItem[] {
  const now = Date.now();
  return torrents.map((t) => {
    const recheck = activeRechecks.get(t.id);
    if (!recheck) return t;

    if (t.status === 'Checking') {
      recheck.seenChecking = true;
      return t;
    }

    if (t.status === 'Queued for checking') {
      return t;
    }

    // Torrent is currently NOT reported as checking by the engine.
    if (!recheck.seenChecking) {
      if (now - recheck.requestedAt < 10000) {
        // Within 10s debounce grace window before engine flips to Checking:
        // preserve optimistic Queued for checking
        return { ...t, status: 'Queued for checking', progress: 0 };
      }
      // Grace window elapsed without engine ever reporting Checking/Queued:
      // discard stale recheck tracking and report engine truth
      activeRechecks.delete(t.id);
      return t;
    }

    // Engine was previously seen Checking, and now is NO LONGER Checking or Queued for checking.
    // That means verification finished successfully!
    activeRechecks.delete(t.id);
    return t;
  });
}

export function averageTransferSpeeds(torrents: TorrentItem[]): TorrentItem[] {
  const now = Date.now();
  return torrents.map((t) => {
    // Preserve raw instantaneous engine rates before replacing with since-start averages
    const rawDown = t.current_download_rate ?? t.download_rate;
    const rawUp = t.current_upload_rate ?? t.upload_rate;

    const active = t.status === 'Downloading' || t.status === 'Seeding';
    if (!active) {
      return {
        ...t,
        download_rate: 0,
        upload_rate: 0,
        current_download_rate: 0,
        current_upload_rate: 0,
      };
    }
    // Average since the respective transfer began: cumulative bytes over
    // cumulative ACTIVE seconds. A paused stretch does not dilute the rate
    // because paused time never enters the active-seconds denominator.
    const avgDown = lifetimeAverage(t.downloaded_bytes, t.seconds_downloading, t.added_on, now);
    const avgUp = lifetimeAverage(t.uploaded_bytes, t.seconds_seeding, t.added_on, now);

    // ETA from the SAME average the UI displays: remaining bytes / since-start
    // average. Consistent with the columns and immune to burst flicker.
    const remaining = Math.max(0, t.total_bytes * (1 - t.progress));
    const etaSeconds =
      t.status !== 'Downloading' || avgDown <= 0 ? undefined : Math.round(remaining / avgDown);

    return {
      ...t,
      download_rate: avgDown,
      upload_rate: avgUp,
      current_download_rate: rawDown,
      current_upload_rate: rawUp,
      eta_seconds: etaSeconds,
    };
  });
}

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
  forceRecheck: (id: string) => Promise<{ success: boolean; error?: string }>;
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
      set({ torrents: averageTransferSpeeds(reconcileRecheckState(data)), isLoading: false });
    } catch {
      set({ isLoading: false });
    }
  },

  setSelectedId: (selectedId) => set({ selectedId }),

  pauseTorrent: async (id: string) => {
    try {
      await invoke('pause_torrent', { id });
    } catch (err) {
      console.warn('pause_torrent failed:', err);
    }
    set({
      torrents: get().torrents.map((t) =>
        t.id === id ? { ...t, status: 'Paused', download_rate: 0, upload_rate: 0 } : t
      ),
    });
    // Pull engine truth immediately so the optimistic state is never
    // reverted by the next telemetry/poll tick (owner-reported 5-10s lag).
    await get().fetchTorrents();
  },

  resumeTorrent: async (id: string) => {
    try {
      await invoke('resume_torrent', { id });
    } catch (err) {
      console.warn('resume_torrent failed:', err);
    }
    set({
      torrents: get().torrents.map((t) =>
        t.id === id ? { ...t, status: t.progress >= 1.0 ? 'Seeding' : 'Downloading' } : t
      ),
    });
    await get().fetchTorrents();
  },

  pauseAll: async () => {
    try {
      await invoke('pause_all_torrents');
    } catch (err) {
      console.warn('pause_all_torrents failed:', err);
    }
    set({
      torrents: get().torrents.map((t) => ({ ...t, status: 'Paused', download_rate: 0, upload_rate: 0 })),
    });
    await get().fetchTorrents();
  },

  resumeAll: async () => {
    try {
      await invoke('resume_all_torrents');
    } catch (err) {
      console.warn('resume_all_torrents failed:', err);
    }
    set({
      torrents: get().torrents.map((t) => ({
        ...t,
        status: t.progress >= 1.0 ? 'Seeding' : 'Downloading',
      })),
    });
    await get().fetchTorrents();
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

  forceRecheck: async (id: string): Promise<{ success: boolean; error?: string }> => {
    activeRechecks.set(id, { requestedAt: Date.now(), seenChecking: false });
    set({
      torrents: get().torrents.map((t) =>
        t.id === id ? { ...t, status: 'Queued for checking', progress: 0 } : t
      ),
    });
    try {
      await invoke('force_recheck_torrent', { id });
      // Schedule a single refresh to capture initial checking state;
      // subsequent progress updates stream live from engine telemetry.
      setTimeout(() => {
        void get().fetchTorrents();
      }, 300);
      return { success: true };
    } catch (err: unknown) {
      const msg =
        typeof err === 'string'
          ? err
          : (err as Error)?.message || 'Failed to start force recheck';
      console.warn('force_recheck_torrent failed:', msg);
      activeRechecks.delete(id);
      await get().fetchTorrents();
      return { success: false, error: msg };
    }
  },

  reannounce: async (id: string) => {
    try {
      await invoke('reannounce_torrent', { id });
    } catch (err) {
      console.warn('reannounce_torrent failed:', err);
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
    // next-announce times only move once the tracker answers; refresh now
    // and let the next poll pick up the final scheduled time.
    await get().fetchTorrents();
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
    try {
      const result = await invoke<boolean>('toggle_torrent_sequential', { id });
      set({
        torrents: get().torrents.map((t) =>
          t.id === id ? { ...t, sequential_download: result } : t
        ),
      });
      return result;
    } catch (err) {
      console.error('[sequential] failed to toggle sequential download:', err);
      throw err;
    }
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
    // Mirror only what the engine accepted. A refused request (superseeding is
    // not supported by the current engine) leaves the torrent as it was
    // instead of flipping the flag locally.
    const current = get().torrents.find((t) => t.id === id)?.super_seeding ?? false;
    try {
      const result = await invoke<boolean>('toggle_torrent_super_seeding', { id });
      set({
        torrents: get().torrents.map((t) => (t.id === id ? { ...t, super_seeding: result } : t)),
      });
      return result;
    } catch (err) {
      console.warn('toggle_torrent_super_seeding failed:', err);
      return current;
    }
  },

  setFilePriority: async (id: string, fileIndex: number, priority: number) => {
    try {
      await invoke('set_file_priority', { id, fileIndex, priority });
    } catch (err) {
      console.warn('set_file_priority failed:', err);
    }
    // The dropdown reads the mapped file priority from the engine; without
    // this refresh it keeps showing the old bucket forever.
    await get().fetchTorrents();
    // Mirror the change locally too so the dropdown updates even before the
    // refreshed engine payload lands.
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

  // Renames must surface engine errors (unsupported renames, name clashes,
  // disk failures) in the modal - no optimistic UI faking.
  renameTorrentFile: async (id: string, fileIndex: number, newFileName: string) => {
    await invoke('rename_torrent_file', { id, fileIndex, newFileName });
    await get().fetchTorrents();
  },

  renameTorrentFolder: async (id: string, oldFolderPath: string, newFolderName: string) => {
    await invoke('rename_torrent_folder', { id, oldFolderPath, newFolderName });
    await get().fetchTorrents();
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
