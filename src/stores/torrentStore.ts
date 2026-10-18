import { create } from 'zustand';
import { TorrentItem, CreateTorrentParams, InspectTorrentResult, TorrentContentMatch, TorrentRenameEntry } from '../types';
import { invoke } from '@tauri-apps/api/core';

// Display-side speed presentation, modeled on the established torrent
// clients. qBittorrent (libtorrent) and Transmission do NOT display raw
// per-tick rates: both derive speed from cumulative byte counters over a
// short window (libtorrent: ~2s sliding window; Transmission tr_bandwidth:
// ~2s decaying average of counter deltas). librqbit's raw per-second rates
// swing between burst and 0 on small swarms, so raw display flickers.
// Nebulae therefore computes a decaying average of COUNTER DELTAS over a
// ~15s time constant (owner-requested: the 5s window still fluctuated
// widely on small swarms) and settles honestly to zero when the counters
// stop moving. Per-tick raw rates are never passed through.
const SPEED_WINDOW_SECONDS = 15.0;
const SPEED_IDLE_SNAP_TICKS = 10; // ~5s at 500ms telemetry before snapping to exact 0
interface SpeedCounterState {
  lastDownBytes: number | null;
  lastUpBytes: number | null;
  lastTimeMs: number | null;
  avgDown: number;
  avgUp: number;
  idleTicks: number;
}
const speedCounters = new Map<string, SpeedCounterState>();

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
    if (!recheck.seenChecking && now - recheck.requestedAt < 1500) {
      // The daemon has not yet transitioned into checking in this polled sample,
      // and we are within the debounce grace window. Preserve Queued for checking.
      return { ...t, status: 'Queued for checking', progress: 0 };
    }

    // Either we previously saw checking and it has completed, or the grace
    // window elapsed. Clean up activeRechecks and report the engine's final truth.
    activeRechecks.delete(t.id);
    return t;
  });
}

export function smoothTelemetrySpeeds(torrents: TorrentItem[]): TorrentItem[] {
  const seen = new Set<string>();
  const now = Date.now();
  const out = torrents.map((t) => {
    seen.add(t.id);
    const active = t.status === 'Downloading' || t.status === 'Seeding';
    if (!active) {
      speedCounters.delete(t.id);
      return { ...t, download_rate: 0, upload_rate: 0 };
    }
    const prev = speedCounters.get(t.id) ?? {
      lastDownBytes: null,
      lastUpBytes: null,
      lastTimeMs: null,
      avgDown: 0,
      avgUp: 0,
      idleTicks: 0,
    };

    // dt guard: clamp weird intervals (app sleep, first sample, clock jumps).
    let dt = prev.lastTimeMs === null ? 0 : (now - prev.lastTimeMs) / 1000;
    if (dt <= 0.01 || dt > 30) {
      // Anchor (or re-anchor after a long gap) without producing a spike.
      dt = 0;
    }

    const deltaOf = (current: number, last: number | null) => {
      if (dt === 0 || last === null) return null; // no interval yet
      const delta = current - last;
      return delta > 0 ? delta : 0; // counter reset (recheck/re-add) = no traffic
    };

    const downDelta = deltaOf(t.downloaded_bytes, prev.lastDownBytes);
    const upDelta = deltaOf(t.uploaded_bytes, prev.lastUpBytes);
    const instDown = downDelta === null ? null : downDelta / dt;
    const instUp = upDelta === null ? null : upDelta / dt;

    // Exponential smoothing with a time-based weight, equivalent to the
    // clients' windowed averages: newer samples weigh proportionally to
    // the elapsed fraction of the window.
    const weight = dt === 0 ? 0 : 1 - Math.exp(-dt / SPEED_WINDOW_SECONDS);
    const noTraffic =
      (downDelta === null || downDelta === 0) && (upDelta === null || upDelta === 0);
    const idleTicks = noTraffic ? prev.idleTicks + 1 : 0;
    const decayed = (avg: number) => {
      // Counter not advancing: decay toward zero on the same time constant.
      return avg * Math.exp(-dt / SPEED_WINDOW_SECONDS);
    };

    let avgDown = prev.avgDown;
    let avgUp = prev.avgUp;
    if (dt === 0) {
      // First sample or re-anchor: keep the current average, wait for an interval.
    } else if (instDown !== null && instDown > 0) {
      avgDown += weight * (instDown - avgDown);
    } else if (downDelta === 0) {
      avgDown = decayed(avgDown);
    }
    if (dt !== 0 && instUp !== null && instUp > 0) {
      avgUp += weight * (instUp - avgUp);
    } else if (dt !== 0 && upDelta === 0) {
      avgUp = decayed(avgUp);
    }

    // Snap to exact zero once traffic has been absent for a while, so an
    // idle torrent does not display an asymptotic sliver forever.
    const snap = idleTicks >= SPEED_IDLE_SNAP_TICKS;
    const finalDown = snap ? 0 : avgDown;
    const finalUp = snap ? 0 : avgUp;

    // ETA from the SAME smoothed speed the UI displays (what the established
    // clients do: remaining bytes / windowed rate). Computing it from the raw
    // engine rate made it jump between 0, infinity and hours; deriving it
    // from the displayed average keeps it consistent and slowly-moving. A
    // genuinely stalled download shows infinity only after the idle snap.
    const remaining = Math.max(0, t.total_bytes * (1 - t.progress));
    const etaSeconds =
      t.status !== 'Downloading' || finalDown <= 0
        ? undefined
        : Math.round(remaining / finalDown);

    speedCounters.set(t.id, {
      lastDownBytes: t.downloaded_bytes,
      lastUpBytes: t.uploaded_bytes,
      lastTimeMs: now,
      avgDown: finalDown,
      avgUp: finalUp,
      idleTicks,
    });
    return { ...t, download_rate: finalDown, upload_rate: finalUp, eta_seconds: etaSeconds };
  });
  for (const id of [...speedCounters.keys()]) {
    if (!seen.has(id)) speedCounters.delete(id);
  }
  return out;
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
      set({ torrents: smoothTelemetrySpeeds(reconcileRecheckState(data)), isLoading: false });
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

  forceRecheck: async (id: string) => {
    activeRechecks.set(id, { requestedAt: Date.now(), seenChecking: false });
    set({
      torrents: get().torrents.map((t) =>
        t.id === id ? { ...t, status: 'Queued for checking', progress: 0 } : t
      ),
    });
    try {
      await invoke('force_recheck_torrent', { id });
    } catch (err) {
      console.warn('force_recheck_torrent failed:', err);
      activeRechecks.delete(id);
      await get().fetchTorrents();
      return;
    }
    // Pull engine truth immediately
    await get().fetchTorrents();

    // Fast-poll during verification (200ms) so intermediate progress
    // is reflected smoothly in the UI
    let ticksLeft = 60; // Up to 12s of fast polling if needed
    const fastPoll = async () => {
      const entry = activeRechecks.get(id);
      if (!entry || ticksLeft <= 0) return;
      ticksLeft--;
      await get().fetchTorrents();
      const current = get().torrents.find((t) => t.id === id);
      if (current && (current.status === 'Checking' || current.status === 'Queued for checking')) {
        setTimeout(fastPoll, 200);
      } else {
        activeRechecks.delete(id);
      }
    };
    setTimeout(fastPoll, 100);
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
