import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { PeerTelemetry, PeeringSettings } from '../types';

interface PeeringState {
  settings: PeeringSettings | null;
  peers: PeerTelemetry[];
  activeTorrentId: string | null;
  isLoading: boolean;
  error: string | null;
  isStale: boolean;
  fetchSettings: () => Promise<void>;
  saveSettings: (settings: PeeringSettings) => Promise<void>;
  fetchPeers: (torrentId: string, scopeKey?: string) => Promise<void>;
  clearPeers: () => void;
}

let selectionToken = 0;
const activeRequests = new Map<string, Promise<void>>();

export const usePeeringStore = create<PeeringState>((set, get) => ({
  settings: null,
  peers: [],
  activeTorrentId: null,
  isLoading: false,
  error: null,
  isStale: false,

  fetchSettings: async () => {
    try {
      const res = await invoke<PeeringSettings>('get_peering_settings');
      set({ settings: res });
    } catch {
      set({
        settings: {
          mode: 'Balanced',
          locality_enabled: true,
          max_peers_per_torrent: 80,
          max_global_peers: 500,
          prefer_utp: true,
          require_encryption: true,
          probe_rtt_enabled: true,
        },
      });
    }
  },

  saveSettings: async (settings: PeeringSettings) => {
    try {
      await invoke('save_peering_settings', { settings });
      set({ settings });
    } catch {
      set({ settings });
    }
  },

  clearPeers: () => {
    selectionToken++;
    activeRequests.clear();
    set({ activeTorrentId: null, peers: [], isLoading: false, error: null, isStale: false });
  },

  fetchPeers: async (torrentId: string, scopeKey = 'local') => {
    const isNew = get().activeTorrentId !== torrentId;
    if (isNew) {
      selectionToken++;
      set({ activeTorrentId: torrentId, peers: [], isLoading: true, error: null, isStale: false });
    } else {
      set({ error: null });
    }

    const currentToken = selectionToken;
    const requestKey = `${scopeKey}:${torrentId}:${currentToken}`;

    // Coalesce identical in-flight reads
    const existingPromise = activeRequests.get(requestKey);
    if (existingPromise) {
      return existingPromise;
    }

    const promise = (async () => {
      try {
        const res = await invoke<PeerTelemetry[]>('get_peer_telemetry', { torrentId });
        // Guard against race conditions and A-to-B-to-A selection changes
        if (selectionToken === currentToken && get().activeTorrentId === torrentId) {
          set({ peers: res, isLoading: false, error: null, isStale: false });
        }
      } catch (err: unknown) {
        if (selectionToken === currentToken && get().activeTorrentId === torrentId) {
          // Keep last-good peers on same-selection refresh failure; mark them stale with safe error text
          const existingPeers = get().peers;
          const safeError = err instanceof Error ? err.message : String(err);
          set({
            peers: existingPeers,
            isLoading: false,
            error: safeError,
            isStale: existingPeers.length > 0,
          });
        }
      } finally {
        activeRequests.delete(requestKey);
      }
    })();

    activeRequests.set(requestKey, promise);
    return promise;
  },
}));
