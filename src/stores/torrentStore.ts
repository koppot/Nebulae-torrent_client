import { create } from 'zustand';
import type {
  TorrentItem,
  CreateTorrentParams,
  InspectTorrentResult,
  TorrentContentMatch,
  TorrentRenameEntry,
  TorrentOperationSnapshot,
  StagedTorrentAdd,
} from '../types/index.ts';
import { invoke } from '@tauri-apps/api/core';
import { createLatestGate, mergeFileStats } from '../lib/torrentFiles.ts';

// Per-torrent request ordering for detail fetches: a response is applied only
// to its own torrent, and never after a response to a newer request for it.
const detailGate = createLatestGate();

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

export interface OperationTombstone {
  operation_id: string;
  torrent_id: string;
  revision: number;
  timestamp: number;
  type?: 'move' | 'remove';
  destination?: string;
}

// In-flight read coalescing and pending mutation tracking
let inFlightFetch: Promise<void> | null = null;
let needsFollowupRefresh = false;
let fetchScopeGeneration = 0;
const pendingMutations = new Set<string>();

export function invalidateTorrentFetches() {
  fetchScopeGeneration++;
  inFlightFetch = null;
  needsFollowupRefresh = false;
}

export interface TerminalNotice {
  operation_id: string;
  torrent_id: string;
  name?: string;
  outcome: string;
  timestamp: number;
  status?: 'confirmed' | 'unconfirmed' | 'uncertain' | 'error';
}

const MAX_TOMBSTONES = 100;

function addTombstone(
  tombstones: Record<string, OperationTombstone>,
  entry: OperationTombstone
): Record<string, OperationTombstone> {
  const next = { ...tombstones, [entry.operation_id]: entry };
  const keys = Object.keys(next);
  if (keys.length > MAX_TOMBSTONES) {
    keys.sort((a, b) => next[a].timestamp - next[b].timestamp);
    const excess = keys.length - MAX_TOMBSTONES;
    for (let i = 0; i < excess; i++) {
      delete next[keys[i]];
    }
  }
  return next;
}

function isOperationTombstoned(
  tombstones: Record<string, OperationTombstone>,
  opId: string,
  torrentId?: string,
  createdAt?: number
): boolean {
  if (tombstones[opId]) {
    return true;
  }
  if (torrentId) {
    const match = Object.values(tombstones).find(
      (t) =>
        (t.torrent_id === torrentId || t.torrent_id.toLowerCase() === torrentId.toLowerCase()) &&
        (createdAt ? createdAt <= t.timestamp : false)
    );
    if (match) {
      return true;
    }
  }
  return false;
}

interface TorrentState {
  torrents: TorrentItem[];
  selectedId: string | null;
  isLoading: boolean;
  filesLoadingId: string | null;
  filesError: string | null;
  fetchTorrents: (forceFollowup?: boolean) => Promise<void>;
  /** Drops the displayed list when the Manager/Server scope changes. */
  clearTorrents: () => void;
  applyTorrentTelemetry: (incoming: TorrentItem[]) => void;
  fetchTorrentDetails: (id: string) => Promise<void>;
  /** Silently re-reads per-file statistics for one torrent (no loading state). */
  refreshTorrentFiles: (id: string) => Promise<void>;
  setSelectedId: (id: string | null) => void;
  pauseTorrent: (id: string) => Promise<void>;
  resumeTorrent: (id: string) => Promise<void>;
  pauseAll: () => Promise<void>;
  resumeAll: () => Promise<void>;
  removeTorrent: (id: string, deleteFiles?: boolean) => Promise<void>;
  addTorrent: (source: string, downloadDir?: string, bytes?: number[]) => Promise<void>;
  addTorrentStaged: (staged: StagedTorrentAdd) => Promise<string>;
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
  setFilesPriority: (id: string, fileIndices: number[], priority: number) => Promise<void>;
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
  activeOperations: Record<string, TorrentOperationSnapshot>;
  fetchActiveOperations: () => Promise<void>;
  handleOperationEvent: (op: TorrentOperationSnapshot) => void;
  dismissOperation: (id: string) => Promise<void>;
  terminalNotice: TerminalNotice | null;
  lastOutcome: string | null;
  tombstones: Record<string, OperationTombstone>;
  dismissTerminalNotice: () => void;
}

export function areTorrentsSemanticallyEqual(a: TorrentItem, b: TorrentItem): boolean {
  return (
    a.id === b.id &&
    a.info_hash === b.info_hash &&
    a.status === b.status &&
    a.progress === b.progress &&
    a.download_rate === b.download_rate &&
    a.upload_rate === b.upload_rate &&
    a.current_download_rate === b.current_download_rate &&
    a.current_upload_rate === b.current_upload_rate &&
    a.avg_download_rate === b.avg_download_rate &&
    a.avg_upload_rate === b.avg_upload_rate &&
    a.eta_seconds === b.eta_seconds &&
    a.total_bytes === b.total_bytes &&
    a.downloaded_bytes === b.downloaded_bytes &&
    a.uploaded_bytes === b.uploaded_bytes &&
    a.seeds === b.seeds &&
    a.peers === b.peers &&
    a.ratio === b.ratio &&
    a.is_media === b.is_media &&
    a.category === b.category &&
    a.save_path === b.save_path &&
    a.added_on === b.added_on &&
    a.last_checked === b.last_checked &&
    a.name === b.name &&
    a.errorString === b.errorString &&
    a.sequential_download === b.sequential_download &&
    a.first_last_piece_prio === b.first_last_piece_prio &&
    a.sort_order === b.sort_order &&
    a.racingMode === b.racingMode &&
    a.bandwidthPriority === b.bandwidthPriority &&
    a.files === b.files &&
    a.trackers === b.trackers
  );
}

export const areTorrentsEqual = areTorrentsSemanticallyEqual;

export const useTorrentStore = create<TorrentState>((set, get) => ({
  torrents: [],
  selectedId: null,
  isLoading: false,
  filesLoadingId: null,
  filesError: null,
  activeOperations: {},
  terminalNotice: null,
  lastOutcome: null,
  tombstones: {},
  dismissTerminalNotice: () => set({ terminalNotice: null, lastOutcome: null }),

  applyTorrentTelemetry: (incoming: TorrentItem[]) => {
    const prevTorrents = get().torrents;
    const sanitized = sanitizeTorrentRates(incoming, prevTorrents);
    const reconciled = reconcileRecheckState(sanitized);

    const tombstones = get().tombstones;
    const activeOps = get().activeOperations;

    // Filter out deleted torrents (tombstoned or with an active remove operation)
    // and protect moved paths against stale reads
    const safeReconciled = reconciled
      .filter((t) => {
        const hasRemoveTombstone = Object.values(tombstones).some(
          (tb) =>
            (tb.type === 'remove' || (!tb.type && !tb.destination)) &&
            (tb.torrent_id === t.id ||
              (t.info_hash && tb.torrent_id.toLowerCase() === t.info_hash.toLowerCase()))
        );
        const hasActiveRemove = Boolean(
          (activeOps[t.id] && activeOps[t.id].kind?.type === 'remove') ||
          (t.info_hash && activeOps[t.info_hash.toLowerCase()]?.kind?.type === 'remove')
        );
        return !hasRemoveTombstone && !hasActiveRemove;
      })
      .map((t) => {
        const activeMove =
          (activeOps[t.id]?.kind?.type === 'move' ? activeOps[t.id] : undefined) ||
          (t.info_hash && activeOps[t.info_hash.toLowerCase()]?.kind?.type === 'move'
            ? activeOps[t.info_hash.toLowerCase()]
            : undefined);
        const moveDest =
          activeMove?.destination ||
          (activeMove?.kind?.type === 'move' ? activeMove.kind.destination : undefined);
        if (moveDest) {
          return { ...t, save_path: moveDest };
        }
        const moveTombstone = Object.values(tombstones).find(
          (tb) =>
            tb.destination &&
            (tb.torrent_id === t.id ||
              (t.info_hash && tb.torrent_id.toLowerCase() === t.info_hash.toLowerCase()))
        );
        if (moveTombstone?.destination) {
          return { ...t, save_path: moveTombstone.destination };
        }
        return t;
      });

    // Retain rows with active operations that have not reached a terminal outcome
    // (phase !== 'complete') even if the engine stops reporting them.
    const retainedTorrents: TorrentItem[] = [...safeReconciled];
    for (const prev of prevTorrents) {
      const op = activeOps[prev.id] || (prev.info_hash ? activeOps[prev.info_hash.toLowerCase()] : undefined);
      if (op && op.phase !== 'complete') {
        const alreadyInReconciled = retainedTorrents.some(
          (t) => t.id === prev.id || (t.info_hash && prev.info_hash && t.info_hash.toLowerCase() === prev.info_hash.toLowerCase())
        );
        if (!alreadyInReconciled) {
          retainedTorrents.push(prev);
        }
      }
    }

    // Fast-path structural check to prevent React re-render cascades when nothing changed
    let hasChanges = prevTorrents.length !== retainedTorrents.length;
    const finalTorrents: TorrentItem[] = [];

    if (!hasChanges) {
      for (let i = 0; i < retainedTorrents.length; i++) {
        const prev = prevTorrents[i];
        const next = retainedTorrents[i];
        if (areTorrentsEqual(prev, next)) {
          finalTorrents.push(prev);
        } else {
          hasChanges = true;
          finalTorrents.push(next);
        }
      }
    }

    if (hasChanges) {
      set({ torrents: hasChanges && finalTorrents.length === retainedTorrents.length ? finalTorrents : retainedTorrents, isLoading: false });
    } else if (get().isLoading) {
      set({ isLoading: false });
    }

    // If a torrent is currently selected and lacks file details, trigger fetch
    const currentSelId = get().selectedId;
    if (currentSelId) {
      const activeList = hasChanges ? finalTorrents : prevTorrents;
      const match = activeList.find(
        (t) => t.id === currentSelId || (t.info_hash && t.info_hash.toLowerCase() === currentSelId.toLowerCase())
      );
      if (match && (!match.files || match.files.length === 0) && get().filesLoadingId !== match.id && !get().filesError) {
        get().fetchTorrentDetails(match.id);
      }
    }
  },

  fetchTorrentDetails: async (id: string) => {
    set({ filesLoadingId: id, filesError: null });
    const seq = detailGate.begin(id);
    try {
      const detail = await invoke<TorrentItem>('get_torrent_details', { id });
      if (detail && detailGate.accept(id, seq)) {
        set((state) => ({
          torrents: state.torrents.map((t) =>
            t.id === id || (t.info_hash && detail.info_hash && t.info_hash.toLowerCase() === detail.info_hash.toLowerCase())
              ? { ...t, files: mergeFileStats(t.files, detail.files ?? []), trackers: detail.trackers }
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

  refreshTorrentFiles: async (id: string) => {
    const seq = detailGate.begin(id);
    try {
      const detail = await invoke<TorrentItem>('get_torrent_details', { id });
      if (!detail || !detailGate.accept(id, seq)) return;
      const nextFiles = detail.files ?? [];
      if (nextFiles.length === 0) return;
      set((state) => {
        let changed = false;
        const torrents = state.torrents.map((t) => {
          const same =
            t.id === id ||
            (t.info_hash && detail.info_hash && t.info_hash.toLowerCase() === detail.info_hash.toLowerCase());
          if (!same) return t;
          const files = mergeFileStats(t.files, nextFiles);
          if (files === t.files) return t;
          changed = true;
          return { ...t, files };
        });
        return changed ? { torrents } : state;
      });
    } catch {
      // A failed background refresh keeps the last good snapshot on screen.
    }
  },

  clearTorrents: () => {
    invalidateTorrentFetches();
    set({ torrents: [], selectedId: null, isLoading: false });
  },

  fetchTorrents: async (forceFollowup = false) => {
    if (inFlightFetch) {
      if (forceFollowup) {
        needsFollowupRefresh = true;
      }
      return inFlightFetch;
    }

    const scopeGen = fetchScopeGeneration;
    inFlightFetch = (async () => {
      if (get().torrents.length === 0) {
        set({ isLoading: true });
      }
      try {
        const data = await invoke<TorrentItem[]>('list_torrents');
        if (scopeGen === fetchScopeGeneration) {
          get().applyTorrentTelemetry(data);
        }
      } catch {
        if (scopeGen === fetchScopeGeneration) {
          set({ isLoading: false });
        }
      } finally {
        inFlightFetch = null;
        if (needsFollowupRefresh && scopeGen === fetchScopeGeneration) {
          needsFollowupRefresh = false;
          void get().fetchTorrents(false);
        }
      }
    })();

    return inFlightFetch;
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
    if (pendingMutations.has(id)) return;
    pendingMutations.add(id);
    try {
      await invoke('pause_torrent', { id });
      set({
        torrents: get().torrents.map((t) =>
          t.id === id ? { ...t, status: 'Paused', download_rate: 0, upload_rate: 0 } : t
        ),
      });
      await get().fetchTorrents(true);
    } catch (err) {
      console.error('pause_torrent failed:', err);
    } finally {
      pendingMutations.delete(id);
    }
  },

  resumeTorrent: async (id: string) => {
    if (pendingMutations.has(id)) return;
    pendingMutations.add(id);
    try {
      await invoke('resume_torrent', { id });
      set({
        torrents: get().torrents.map((t) =>
          t.id === id ? { ...t, status: t.progress >= 1.0 ? 'Seeding' : 'Downloading' } : t
        ),
      });
      await get().fetchTorrents(true);
    } catch (err) {
      console.error('resume_torrent failed:', err);
    } finally {
      pendingMutations.delete(id);
    }
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
    await get().fetchTorrents(true);
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
    await get().fetchTorrents(true);
  },

  removeTorrent: async (id: string, deleteFiles?: boolean) => {
    const existing = get().activeOperations[id] || (id ? get().activeOperations[id.toLowerCase()] : undefined);
    if (existing && existing.phase !== 'error' && existing.phase !== 'complete') {
      return;
    }

    const now = Date.now();
    const optimisticOpId = `op-opt-${now}`;
    const matchingTorrent = get().torrents.find(
      (t) => t.id === id || (t.info_hash && t.info_hash.toLowerCase() === id.toLowerCase())
    );
    const optimisticOp: TorrentOperationSnapshot = {
      operation_id: optimisticOpId,
      torrent_id: id,
      kind: { type: 'remove', delete_files: !!deleteFiles },
      phase: deleteFiles ? 'removing_torrent_and_deleting_files' : 'removing_torrent',
      revision: 1,
      created_at: now,
      observed_at: now,
      is_hydrated: false,
    };
    const initialOps: Record<string, TorrentOperationSnapshot> = {
      ...get().activeOperations,
      [id]: optimisticOp,
      [id.toLowerCase()]: optimisticOp,
    };
    if (matchingTorrent) {
      initialOps[matchingTorrent.id] = optimisticOp;
      initialOps[matchingTorrent.id.toLowerCase()] = optimisticOp;
      if (matchingTorrent.info_hash) {
        initialOps[matchingTorrent.info_hash.toLowerCase()] = optimisticOp;
      }
    }
    set({ activeOperations: initialOps });

    try {
      await invoke('remove_torrent', { id, deleteFiles: !!deleteFiles });
    } catch (err: unknown) {
      const safeError = typeof err === 'string' ? err : (err as Error)?.message || 'Removal failed';
      set((state) => {
        // Replace optimistic state with authoritative state without allowing the invoke catch to overwrite it with a different ID
        const current = state.activeOperations[id] || (id ? state.activeOperations[id.toLowerCase()] : undefined);
        const resolvedOpId = current?.operation_id || optimisticOpId;
        const currentRev = current?.revision ?? 1;

        if (current && current.phase === 'error') {
          return state;
        }

        const isUncertain =
          safeError.includes('The engine response was not received') ||
          safeError.includes('Removal or file deletion may still be in progress') ||
          safeError.toLowerCase().includes('timeout') ||
          safeError.toLowerCase().includes('unconfirmed');

        const errorSnapshot: TorrentOperationSnapshot = {
          operation_id: resolvedOpId,
          torrent_id: id,
          kind: { type: 'remove', delete_files: !!deleteFiles },
          phase: 'error',
          safe_error: safeError,
          revision: currentRev + 1,
          created_at: current?.created_at ?? Date.now(),
          observed_at: current?.observed_at,
          is_hydrated: current?.is_hydrated,
        };

        const matching = state.torrents.find(
          (t) => t.id === id || (t.info_hash && t.info_hash.toLowerCase() === id.toLowerCase())
        );

        const notice: TerminalNotice = {
          operation_id: resolvedOpId,
          torrent_id: id,
          name: matching?.name || id,
          outcome: safeError,
          timestamp: Date.now(),
          status: isUncertain ? 'uncertain' : 'error',
        };

        const updated = { ...state.activeOperations };
        for (const [key, item] of Object.entries(updated)) {
          if (item.operation_id === resolvedOpId) {
            updated[key] = errorSnapshot;
          }
        }
        updated[id] = errorSnapshot;
        updated[id.toLowerCase()] = errorSnapshot;
        if (matching) {
          updated[matching.id] = errorSnapshot;
          updated[matching.id.toLowerCase()] = errorSnapshot;
          if (matching.info_hash) {
            updated[matching.info_hash.toLowerCase()] = errorSnapshot;
          }
        }

        return {
          activeOperations: updated,
          terminalNotice: notice,
          lastOutcome: safeError,
        };
      });
    }
  },

  fetchActiveOperations: async () => {
    try {
      const list = await invoke<TorrentOperationSnapshot[]>('get_active_torrent_operations');
      set((state) => {
        const currentOps = { ...state.activeOperations };
        let changed = false;

        for (const item of list) {
          if (isOperationTombstoned(state.tombstones, item.operation_id, item.torrent_id, item.created_at)) {
            continue;
          }

          const existing =
            currentOps[item.torrent_id] || (item.torrent_id ? currentOps[item.torrent_id.toLowerCase()] : undefined);

          if (existing) {
            // Reconcile snapshot without overwriting newer events
            if (existing.operation_id === item.operation_id && existing.revision >= item.revision) {
              continue;
            }
            if (existing.created_at > item.created_at) {
              continue;
            }
            item.observed_at = existing.observed_at || existing.created_at;
            item.is_hydrated = existing.is_hydrated ?? false;
          } else {
            item.observed_at = Date.now();
            item.is_hydrated = true;
          }

          currentOps[item.torrent_id] = item;
          if (item.torrent_id) {
            currentOps[item.torrent_id.toLowerCase()] = item;
          }
          const matchingTorrent = state.torrents.find(
            (t) => t.id === item.torrent_id || (t.info_hash && t.info_hash.toLowerCase() === item.torrent_id.toLowerCase())
          );
          if (matchingTorrent) {
            currentOps[matchingTorrent.id] = item;
            currentOps[matchingTorrent.id.toLowerCase()] = item;
            if (matchingTorrent.info_hash) {
              currentOps[matchingTorrent.info_hash.toLowerCase()] = item;
            }
          }
          changed = true;
        }

        return changed ? { activeOperations: currentOps } : state;
      });
    } catch (err) {
      console.warn('fetchActiveOperations failed:', err);
    }
  },

  handleOperationEvent: (op: TorrentOperationSnapshot) => {
    set((state) => {
      if (isOperationTombstoned(state.tombstones, op.operation_id, op.torrent_id, op.created_at)) {
        return state;
      }

      const existing =
        state.activeOperations[op.torrent_id] ||
        (op.torrent_id ? state.activeOperations[op.torrent_id.toLowerCase()] : undefined);

      if (existing) {
        // Ignore older revisions of the same operation
        if (existing.operation_id === op.operation_id && op.revision < existing.revision) {
          return state;
        }
        // Ignore obsolete events from an earlier operation
        const isExistingOptimistic = existing.operation_id.startsWith('op-opt-');
        if (!isExistingOptimistic && existing.operation_id !== op.operation_id && op.created_at < existing.created_at) {
          return state;
        }
        op.observed_at = existing.observed_at;
        op.is_hydrated = existing.is_hydrated;
      }

      const updated = { ...state.activeOperations };
      const matchingTorrent = state.torrents.find(
        (t) => t.id === op.torrent_id || (t.info_hash && t.info_hash.toLowerCase() === op.torrent_id.toLowerCase())
      );

      const keysToUpdate = [op.torrent_id];
      if (op.torrent_id) keysToUpdate.push(op.torrent_id.toLowerCase());
      if (matchingTorrent) {
        keysToUpdate.push(matchingTorrent.id);
        keysToUpdate.push(matchingTorrent.id.toLowerCase());
        if (matchingTorrent.info_hash) {
          keysToUpdate.push(matchingTorrent.info_hash.toLowerCase());
        }
      }

      let torrents = state.torrents;
      let selectedId = state.selectedId;

      if (op.phase === 'complete') {
        let resolvedOutcome = op.outcome;
        let noticeStatus: 'confirmed' | 'unconfirmed' | 'uncertain' | 'error' = 'confirmed';
        if (!resolvedOutcome) {
          if (op.kind?.type === 'remove') {
            if (op.kind.delete_files) {
              resolvedOutcome =
                'Torrent removed. File-deletion request processed; deletion of every file could not be verified.';
              noticeStatus = 'unconfirmed';
            } else {
              resolvedOutcome = 'Torrent removed. Files kept.';
              noticeStatus = 'confirmed';
            }
          } else if (op.kind?.type === 'move') {
            resolvedOutcome = `Moved to ${op.destination || op.kind.destination || 'new location'}`;
            noticeStatus = 'confirmed';
          } else {
            resolvedOutcome = 'Operation complete';
            noticeStatus = 'confirmed';
          }
        } else {
          if (
            resolvedOutcome.includes('could not be verified') ||
            resolvedOutcome.includes('not independently confirmed')
          ) {
            noticeStatus = 'unconfirmed';
          }
        }

        const notice: TerminalNotice = {
          operation_id: op.operation_id,
          torrent_id: op.torrent_id,
          name: matchingTorrent?.name || op.torrent_id,
          outcome: resolvedOutcome,
          timestamp: Date.now(),
          status: noticeStatus,
        };

        const tombstone: OperationTombstone = {
          operation_id: op.operation_id,
          torrent_id: op.torrent_id,
          revision: op.revision,
          timestamp: Date.now(),
          type: op.kind?.type,
          destination: op.kind?.type === 'move' ? (op.destination || op.kind.destination) : undefined,
        };

        if (op.kind?.type === 'remove') {
          torrents = state.torrents.filter(
            (t) => t.id !== op.torrent_id && (!t.info_hash || t.info_hash.toLowerCase() !== op.torrent_id.toLowerCase())
          );
          if (selectedId && (selectedId === op.torrent_id || (matchingTorrent && selectedId === matchingTorrent.id))) {
            selectedId = torrents[0]?.id || null;
          }
        } else if (op.kind?.type === 'move') {
          const newPath = op.destination || op.kind.destination;
          if (newPath) {
            torrents = state.torrents.map((t) => {
              const isMatch =
                t.id === op.torrent_id ||
                (matchingTorrent && t.id === matchingTorrent.id) ||
                (t.info_hash && op.torrent_id && t.info_hash.toLowerCase() === op.torrent_id.toLowerCase());
              return isMatch ? { ...t, save_path: newPath } : t;
            });
          }
        }

        // Remove every alias for this operation_id
        for (const [key, item] of Object.entries(updated)) {
          if (item.operation_id === op.operation_id) {
            delete updated[key];
          }
        }
        for (const k of keysToUpdate) {
          delete updated[k];
        }

        return {
          activeOperations: updated,
          torrents,
          selectedId,
          terminalNotice: notice,
          lastOutcome: resolvedOutcome,
          tombstones: addTombstone(state.tombstones, tombstone),
        };
      } else if (op.phase === 'error') {
        const errorMsg = op.safe_error || 'Operation failed';
        const isUncertain =
          errorMsg.includes('The engine response was not received') ||
          errorMsg.includes('Removal or file deletion may still be in progress') ||
          errorMsg.toLowerCase().includes('timeout') ||
          errorMsg.toLowerCase().includes('unconfirmed');

        const notice: TerminalNotice = {
          operation_id: op.operation_id,
          torrent_id: op.torrent_id,
          name: matchingTorrent?.name || op.torrent_id,
          outcome: errorMsg,
          timestamp: Date.now(),
          status: isUncertain ? 'uncertain' : 'error',
        };

        // Retain the torrent row in torrents (do not remove it upon error)
        for (const [key, item] of Object.entries(updated)) {
          if (item.torrent_id === op.torrent_id && item.operation_id !== op.operation_id) {
            delete updated[key];
          }
        }
        for (const k of keysToUpdate) {
          updated[k] = op;
        }

        return {
          activeOperations: updated,
          torrents,
          selectedId,
          terminalNotice: notice,
          lastOutcome: errorMsg,
        };
      } else {
        // In-flight phase (removing_torrent, removing_torrent_and_deleting_files, moving, reconciling)
        for (const [key, item] of Object.entries(updated)) {
          if (item.torrent_id === op.torrent_id && item.operation_id !== op.operation_id) {
            delete updated[key];
          }
        }
        for (const k of keysToUpdate) {
          updated[k] = op;
        }
      }

      return {
        activeOperations: updated,
        torrents,
        selectedId,
      };
    });
  },

  dismissOperation: async (id: string) => {
    const currentOps = get().activeOperations;
    const targetOp =
      currentOps[id] ||
      (id ? currentOps[id.toLowerCase()] : undefined) ||
      Object.values(currentOps).find(
        (op) => op.torrent_id === id || op.torrent_id.toLowerCase() === id.toLowerCase() || op.operation_id === id
      );

    // Allow dismissal only for terminal errors; do not let dismissal clear an in-flight reservation
    if (targetOp && targetOp.phase !== 'error') {
      console.warn(`Cannot dismiss operation ${targetOp.operation_id}: phase is '${targetOp.phase}'`);
      return;
    }

    const targetOpId = targetOp?.operation_id || id;
    const targetTorrentId = targetOp?.torrent_id || id;
    const targetRevision = targetOp?.revision ?? 1;

    try {
      await invoke('dismiss_torrent_operation', { id });
    } catch (err) {
      console.warn('dismiss_torrent_operation failed:', err);
      return;
    }

    // Remove every alias matching operation_id in frontend and preserve tombstone
    set((state) => {
      const nextOps = { ...state.activeOperations };
      if (targetOpId) {
        for (const [key, op] of Object.entries(nextOps)) {
          if (op.operation_id === targetOpId) {
            delete nextOps[key];
          }
        }
      }
      delete nextOps[id];
      delete nextOps[id.toLowerCase()];
      delete nextOps[targetTorrentId];
      delete nextOps[targetTorrentId.toLowerCase()];

      const tombstone: OperationTombstone = {
        operation_id: targetOpId,
        torrent_id: targetTorrentId,
        revision: targetRevision,
        timestamp: Date.now(),
      };

      const nextNotice =
        state.terminalNotice &&
        (state.terminalNotice.operation_id === targetOpId ||
          state.terminalNotice.torrent_id === targetTorrentId ||
          state.terminalNotice.torrent_id === id)
          ? null
          : state.terminalNotice;

      return {
        activeOperations: nextOps,
        tombstones: addTombstone(state.tombstones, tombstone),
        terminalNotice: nextNotice,
        lastOutcome: nextNotice ? state.lastOutcome : null,
      };
    });

    void get().fetchTorrents();
  },

  addTorrent: async (source: string, downloadDir?: string, bytes?: number[]) => {
    await invoke('add_torrent', { source, downloadDir, bytes });
    await get().fetchTorrents();
  },

  // F-2 staged add: the torrent is added PAUSED, the user's name and
  // file-selection edits are applied, then it starts (if requested). Returns
  // the new torrent id. On failure the torrent exists PAUSED (the backend
  // error text says so) and nothing downloads until it is started.
  addTorrentStaged: async (staged: StagedTorrentAdd) => {
    const id = await invoke<string>('add_torrent_staged', { staged });
    await get().fetchTorrents();
    return id;
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
    const currentOps = get().activeOperations;
    const matchingTorrent = get().torrents.find(
      (t) => t.id === id || (t.info_hash && t.info_hash.toLowerCase() === id.toLowerCase())
    );
    const existing =
      currentOps[id] ||
      (id ? currentOps[id.toLowerCase()] : undefined) ||
      (matchingTorrent?.info_hash ? currentOps[matchingTorrent.info_hash.toLowerCase()] : undefined);
    if (existing && existing.phase !== 'error' && existing.phase !== 'complete') {
      return;
    }

    const now = Date.now();
    const optimisticOpId = `op-opt-${now}`;
    const optimisticOp: TorrentOperationSnapshot = {
      operation_id: optimisticOpId,
      torrent_id: id,
      kind: { type: 'move', destination: newPath },
      phase: 'moving',
      destination: newPath,
      revision: 1,
      created_at: now,
      observed_at: now,
      is_hydrated: false,
    };

    const initialOps: Record<string, TorrentOperationSnapshot> = {
      ...currentOps,
      [id]: optimisticOp,
      [id.toLowerCase()]: optimisticOp,
    };
    if (matchingTorrent) {
      initialOps[matchingTorrent.id] = optimisticOp;
      initialOps[matchingTorrent.id.toLowerCase()] = optimisticOp;
      if (matchingTorrent.info_hash) {
        initialOps[matchingTorrent.info_hash.toLowerCase()] = optimisticOp;
      }
    }
    set({ activeOperations: initialOps });

    try {
      await invoke('set_torrent_location', { id, newPath });
    } catch (err: unknown) {
      const safeError = typeof err === 'string' ? err : (err as Error)?.message || 'Move failed';
      set((state) => {
        const current = state.activeOperations[id] || (id ? state.activeOperations[id.toLowerCase()] : undefined);
        const resolvedOpId = current?.operation_id || optimisticOpId;
        const currentRev = current?.revision ?? 1;

        if (current && current.phase === 'error') {
          return state;
        }

        const isUncertain =
          safeError.includes('The engine response was not received') ||
          safeError.includes('may still be in progress') ||
          safeError.toLowerCase().includes('timeout') ||
          safeError.toLowerCase().includes('unconfirmed');

        const errorSnapshot: TorrentOperationSnapshot = {
          operation_id: resolvedOpId,
          torrent_id: id,
          kind: { type: 'move', destination: newPath },
          phase: 'error',
          destination: newPath,
          safe_error: safeError,
          revision: currentRev + 1,
          created_at: current?.created_at ?? Date.now(),
          observed_at: current?.observed_at,
          is_hydrated: current?.is_hydrated,
        };

        const matching = state.torrents.find(
          (t) => t.id === id || (t.info_hash && t.info_hash.toLowerCase() === id.toLowerCase())
        );

        const notice: TerminalNotice = {
          operation_id: resolvedOpId,
          torrent_id: id,
          name: matching?.name || id,
          outcome: safeError,
          timestamp: Date.now(),
          status: isUncertain ? 'uncertain' : 'error',
        };

        const updated = { ...state.activeOperations };
        for (const [key, item] of Object.entries(updated)) {
          if (item.operation_id === resolvedOpId) {
            updated[key] = errorSnapshot;
          }
        }
        updated[id] = errorSnapshot;
        updated[id.toLowerCase()] = errorSnapshot;
        if (matching) {
          updated[matching.id] = errorSnapshot;
          updated[matching.id.toLowerCase()] = errorSnapshot;
          if (matching.info_hash) {
            updated[matching.info_hash.toLowerCase()] = errorSnapshot;
          }
        }

        return {
          activeOperations: updated,
          terminalNotice: notice,
          lastOutcome: safeError,
        };
      });
      throw err;
    } finally {
      await get().fetchTorrents();
    }
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

  setFilesPriority: async (id: string, fileIndices: number[], priority: number) => {
    if (fileIndices.length === 0) return;
    try {
      await invoke('set_files_priority', { id, fileIndices, priority });
    } catch (err) {
      console.warn('set_files_priority failed:', err);
    }
    await get().fetchTorrents();
    const changed = new Set(fileIndices);
    set({
      torrents: get().torrents.map((t) =>
        t.id === id
          ? {
              ...t,
              files: t.files.map((f) => (changed.has(f.index) ? { ...f, priority } : f)),
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
