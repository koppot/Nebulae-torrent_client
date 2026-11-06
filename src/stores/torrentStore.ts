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

// Speed presentation: Down Speed and Up Speed represent strictly current
// instantaneous rates. Historical lifetime/since-start averages are computed
// into distinct telemetry fields (avg_download_rate, avg_upload_rate) and
// never substituted into the primary rate columns.
export function sanitizeTorrentRates(torrents: TorrentItem[], prevTorrents?: TorrentItem[]): TorrentItem[] {
  const now = Date.now();
  const prevMap = new Map<string, TorrentItem>();
  if (prevTorrents) {
    for (const p of prevTorrents) {
      prevMap.set(p.id, p);
      if (p.info_hash) prevMap.set(p.info_hash.toLowerCase(), p);
    }
  }

  return torrents.map((t) => {
    const prev = prevMap.get(t.id) || (t.info_hash ? prevMap.get(t.info_hash.toLowerCase()) : undefined);

    // Preserve files and trackers when the incoming overview item omits them
    const files = t.files && t.files.length > 0 ? t.files : (prev?.files ?? []);
    const trackers = t.trackers && t.trackers.length > 0 ? t.trackers : (prev?.trackers ?? []);

    const isDownloading = t.status === 'Downloading';
    const isSeeding = t.status === 'Seeding';
    const isTransferring = isDownloading || isSeeding;

    // Raw instantaneous engine rates
    const rawDown = t.current_download_rate ?? t.download_rate ?? 0;
    const rawUp = t.current_upload_rate ?? t.upload_rate ?? 0;

    // Instantaneous rates strictly enforced: non-transferring directions are 0
    const instDown = isDownloading ? rawDown : 0;
    const instUp = isTransferring ? rawUp : 0;

    // Lifetime/since-start averages kept separate for hover tooltips and details
    const avgDown = lifetimeAverage(t.downloaded_bytes, t.seconds_downloading, t.added_on, now);
    const avgUp = lifetimeAverage(t.uploaded_bytes, t.seconds_seeding, t.added_on, now);

    // ETA from remaining bytes over instantaneous rate when actively downloading
    const remaining = Math.max(0, t.total_bytes * (1 - t.progress));
    const etaSeconds = isDownloading
      ? (t.eta_seconds ?? (instDown > 0 ? Math.round(remaining / instDown) : undefined))
      : undefined;

    return {
      ...t,
      files,
      trackers,
      download_rate: instDown,
      upload_rate: instUp,
      current_download_rate: rawDown,
      current_upload_rate: rawUp,
      avg_download_rate: avgDown,
      avg_upload_rate: avgUp,
      eta_seconds: etaSeconds,
    };
  });
}

// Backwards-compatibility alias
export const averageTransferSpeeds = sanitizeTorrentRates;

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
  filesLoadingId: string | null;
  filesError: string | null;
  fetchTorrents: () => Promise<void>;
  applyTorrentTelemetry: (incoming: TorrentItem[]) => void;
  fetchTorrentDetails: (id: string) => Promise<void>;
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
  filesLoadingId: null,
  filesError: null,

  applyTorrentTelemetry: (incoming: TorrentItem[]) => {
    const prevTorrents = get().torrents;
    const sanitized = sanitizeTorrentRates(incoming, prevTorrents);
    const reconciled = reconcileRecheckState(sanitized);

    set({ torrents: reconciled, isLoading: false });

    // If a torrent is currently selected and lacks file details, trigger fetch
    const currentSelId = get().selectedId;
    if (currentSelId) {
      const match = reconciled.find(
        (t) => t.id === currentSelId || (t.info_hash && t.info_hash.toLowerCase() === currentSelId.toLowerCase())
      );
      if (match && (!match.files || match.files.length === 0) && get().filesLoadingId !== match.id && !get().filesError) {
        get().fetchTorrentDetails(match.id);
      }
    }
  },

  fetchTorrentDetails: async (id: string) => {
    set({ filesLoadingId: id, filesError: null });
    try {
      const detail = await invoke<TorrentItem>('get_torrent_details', { id });
      if (detail) {
        set((state) => ({
          torrents: state.torrents.map((t) =>
            t.id === id || (t.info_hash && detail.info_hash && t.info_hash.toLowerCase() === detail.info_hash.toLowerCase())
              ? { ...t, files: detail.files, trackers: detail.trackers }
              : t
          ),
          filesLoadingId: state.filesLoadingId === id ? null : state.filesLoadingId,
          filesError: null,
        }));
      } else {
        set((state) => ({
          filesLoadingId: state.filesLoadingId === id ? null : state.filesLoadingId,
        }));
      }
    } catch (err: unknown) {
      set((state) => ({
        filesLoadingId: state.filesLoadingId === id ? null : state.filesLoadingId,
        filesError: state.selectedId === id ? String(err) : state.filesError,
      }));
    }
  },

  fetchTorrents: async () => {
    if (get().torrents.length === 0) {
      set({ isLoading: true });
    }
    try {
      const data = await invoke<TorrentItem[]>('list_torrents');
      get().applyTorrentTelemetry(data);
    } catch {
      set({ isLoading: false });
    }
  },

  setSelectedId: (selectedId) => {
    set({ selectedId, filesError: null });
    if (selectedId) {
      const torrent = get().torrents.find(
        (t) => t.id === selectedId || (t.info_hash && t.info_hash.toLowerCase() === selectedId.toLowerCase())
      );
      if (!torrent?.files || torrent.files.length === 0) {
        get().fetchTorrentDetails(selectedId);
      }
    }
  },

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
    // Mirror only what the engine accepted. A refused request (first/last piece
    // priority is not supported by the current engine) leaves the torrent as it was
    // instead of flipping the flag locally.
    const current = get().torrents.find((t) => t.id === id)?.first_last_piece_prio ?? false;
    try {
      const result = await invoke<boolean>('toggle_torrent_first_last', { id });
      set({
        torrents: get().torrents.map((t) =>
          t.id === id ? { ...t, first_last_piece_prio: result } : t
        ),
      });
      return result;
    } catch (err) {
      console.warn('toggle_torrent_first_last failed:', err);
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
