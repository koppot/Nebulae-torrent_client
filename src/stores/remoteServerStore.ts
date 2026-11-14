/**
 * remoteServerStore — polling store for the nebulae-server remote.
 *
 * Token security: ALL API calls go through Rust `invoke` commands defined in
 * commands/remote_server.rs. The API token is read from the settings database
 * inside the Rust process and never sent to the WebView. The frontend passes
 * no credentials to any of these commands.
 *
 * Data isolation: this store is completely separate from useTorrentStore and
 * the local engine. Nothing here is mixed with local torrent data.
 */

import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

// ── Types (mirror commands/remote_server.rs) ──────────────────────────────

export interface RemoteServerStatus {
  name: string;
  version: string;
  engine: string;
  kill_switch_engaged: boolean;
}

export interface RemoteTorrentFileItem {
  index: number;
  name: string;
  size_bytes: number;
  progress: number;
  priority: number;
}

export interface RemoteTorrentTrackerItem {
  tier: number;
  url: string;
  status: string;
  seeds: number;
  peers: number;
  downloaded: number;
  message: string;
}

export interface RemoteTorrentItem {
  id: string;
  name: string;
  total_bytes: number;
  downloaded_bytes: number;
  uploaded_bytes: number;
  download_rate: number;
  upload_rate: number;
  progress: number;
  status: string;
  eta_seconds?: number | null;
  seeds: number;
  peers: number;
  ratio: number;
  category: string;
  is_media: boolean;
  poster_url?: string | null;
  info_hash: string;
  save_path: string;
  added_on: number;
  completed_on?: number | null;
  comment: string;
  created_by: string;
  piece_size: number;
  total_pieces: number;
  sequential_download: boolean;
  first_last_piece_prio: boolean;
  super_seeding: boolean;
  tags: string[];
  files: RemoteTorrentFileItem[];
  trackers: RemoteTorrentTrackerItem[];
  download_limit_kib: number;
  upload_limit_kib: number;
}

export type RemoteConnState =
  | 'disconnected'  // not configured
  | 'connecting'    // first poll in flight
  | 'connected'     // last poll succeeded
  | 'auth_error'    // 401/403
  | 'error';        // network / timeout

// ── Store ─────────────────────────────────────────────────────────────────

interface RemoteServerState {
  connState: RemoteConnState;
  errorMessage: string | null;
  serverStatus: RemoteServerStatus | null;
  lastSuccessAt: number | null;

  torrents: RemoteTorrentItem[];
  selectedId: string | null;
  isPolling: boolean;

  // Lifecycle
  configure: () => void;      // call when settings change; starts polling
  stopPolling: () => void;
  refresh: () => Promise<void>;
  selectTorrent: (id: string | null) => void;

  // Actions — return error string or null
  pauseTorrent: (id: string) => Promise<string | null>;
  resumeTorrent: (id: string) => Promise<string | null>;
  pauseAll: () => Promise<string | null>;
  resumeAll: () => Promise<string | null>;
  removeTorrent: (id: string, deleteFiles: boolean) => Promise<string | null>;
  triggerKillSwitch: () => Promise<string | null>;
  resetKillSwitch: () => Promise<string | null>;
  reannounce: (id: string) => Promise<string | null>;
  recheck: (id: string) => Promise<string | null>;
  setLimits: (id: string, dlKib: number, ulKib: number, ratioLimit: number) => Promise<string | null>;
}

// ── Module-level polling state ────────────────────────────────────────────

let _pollHandle: ReturnType<typeof setInterval> | null = null;
const POLL_INTERVAL_MS = 5_000;

function classifyInvokeError(e: unknown): { msg: string; kind: 'auth' | 'conn' | 'other' } {
  const s = String(e);
  if (s.includes('auth_error') || s.includes('401') || s.includes('403')) {
    return { msg: 'Authentication failed — check your API token in Settings.', kind: 'auth' };
  }
  if (s.includes('not enabled') || s.includes('not configured')) {
    return { msg: s, kind: 'other' };
  }
  if (s.includes('connection_error') || s.includes('timed out') || s.includes('timeout')) {
    return { msg: `Connection error: ${s.replace('connection_error:', '').trim()}`, kind: 'conn' };
  }
  return { msg: s, kind: 'other' };
}

// ── Store definition ──────────────────────────────────────────────────────

export const useRemoteServerStore = create<RemoteServerState>((set, get) => {

  async function pollTick() {
    // Fetch status and torrents in parallel via Rust commands.
    // The Rust side reads credentials from the DB; we pass nothing.
    try {
      const [status, torrents] = await Promise.all([
        invoke<RemoteServerStatus>('remote_server_status'),
        invoke<RemoteTorrentItem[]>('remote_list_torrents'),
      ]);
      set({
        connState: 'connected',
        errorMessage: null,
        serverStatus: status,
        torrents,
        lastSuccessAt: Date.now(),
      });
    } catch (e) {
      const { msg, kind } = classifyInvokeError(e);
      set({
        connState: kind === 'auth' ? 'auth_error' : 'error',
        errorMessage: msg,
      });
    }
  }

  return {
    connState: 'disconnected',
    errorMessage: null,
    serverStatus: null,
    lastSuccessAt: null,
    torrents: [],
    selectedId: null,
    isPolling: false,

    configure() {
      // Clear any existing poll before starting a new one.
      if (_pollHandle !== null) {
        clearInterval(_pollHandle);
        _pollHandle = null;
      }
      set({ connState: 'connecting', errorMessage: null, isPolling: true });
      void pollTick();
      _pollHandle = setInterval(() => void pollTick(), POLL_INTERVAL_MS);
    },

    stopPolling() {
      if (_pollHandle !== null) {
        clearInterval(_pollHandle);
        _pollHandle = null;
      }
      const s = get();
      if (
        !s.isPolling &&
        s.connState === 'disconnected' &&
        s.errorMessage === null &&
        s.serverStatus === null &&
        s.torrents.length === 0 &&
        s.selectedId === null
      ) {
        return;
      }
      set({
        isPolling: false,
        connState: 'disconnected',
        errorMessage: null,
        serverStatus: null,
        torrents: [],
        selectedId: null,
      });
    },

    async refresh() {
      await pollTick();
    },

    selectTorrent(id) {
      set({ selectedId: id });
    },

    async pauseTorrent(id) {
      try {
        await invoke('remote_pause_torrent', { id });
        await pollTick();
        return null;
      } catch (e) {
        return classifyInvokeError(e).msg;
      }
    },

    async resumeTorrent(id) {
      try {
        await invoke('remote_resume_torrent', { id });
        await pollTick();
        return null;
      } catch (e) {
        return classifyInvokeError(e).msg;
      }
    },

    async pauseAll() {
      try {
        await invoke('remote_pause_all');
        await pollTick();
        return null;
      } catch (e) {
        return classifyInvokeError(e).msg;
      }
    },

    async resumeAll() {
      try {
        await invoke('remote_resume_all');
        await pollTick();
        return null;
      } catch (e) {
        return classifyInvokeError(e).msg;
      }
    },

    async removeTorrent(id, deleteFiles) {
      try {
        await invoke('remote_remove_torrent', { id, deleteFiles });
        if (get().selectedId === id) set({ selectedId: null });
        await pollTick();
        return null;
      } catch (e) {
        return classifyInvokeError(e).msg;
      }
    },

    async triggerKillSwitch() {
      try {
        await invoke('remote_set_kill_switch', { engaged: true });
        await pollTick();
        return null;
      } catch (e) {
        return classifyInvokeError(e).msg;
      }
    },

    async resetKillSwitch() {
      try {
        await invoke('remote_set_kill_switch', { engaged: false });
        await pollTick();
        return null;
      } catch (e) {
        return classifyInvokeError(e).msg;
      }
    },

    async reannounce(id) {
      try {
        await invoke('remote_reannounce_torrent', { id });
        await pollTick();
        return null;
      } catch (e) {
        return classifyInvokeError(e).msg;
      }
    },

    async recheck(id) {
      try {
        await invoke('remote_recheck_torrent', { id });
        await pollTick();
        return null;
      } catch (e) {
        return classifyInvokeError(e).msg;
      }
    },

    async setLimits(id, dlKib, ulKib, ratioLimit) {
      try {
        await invoke('remote_set_torrent_limits', { id, dlKib, ulKib, ratioLimit });
        await pollTick();
        return null;
      } catch (e) {
        return classifyInvokeError(e).msg;
      }
    },
  };
});
