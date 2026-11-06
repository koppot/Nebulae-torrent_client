import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { PeerTelemetry, PeeringSettings } from '../types';

interface PeeringState {
  settings: PeeringSettings | null;
  peers: PeerTelemetry[];
  activeTorrentId: string | null;
  isLoading: boolean;
  error: string | null;
  fetchSettings: () => Promise<void>;
  saveSettings: (settings: PeeringSettings) => Promise<void>;
  fetchPeers: (torrentId: string) => Promise<void>;
  clearPeers: () => void;
}

export const usePeeringStore = create<PeeringState>((set, get) => ({
  settings: null,
  peers: [],
  activeTorrentId: null,
  isLoading: false,
  error: null,

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
          probe_rtt_enabled: false,
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
    set({ activeTorrentId: null, peers: [], isLoading: false, error: null });
  },

  fetchPeers: async (torrentId: string) => {
    const isNew = get().activeTorrentId !== torrentId;
    if (isNew) {
      set({ activeTorrentId: torrentId, peers: [], isLoading: true, error: null });
    } else {
      set({ activeTorrentId: torrentId, error: null });
    }

    try {
      const res = await invoke<PeerTelemetry[]>('get_peer_telemetry', { torrentId });
      // Guard against race condition: only update if activeTorrentId matches
      if (get().activeTorrentId === torrentId) {
        set({ peers: res, isLoading: false, error: null });
      }
    } catch (err: unknown) {
      if (get().activeTorrentId === torrentId) {
        set({ peers: [], isLoading: false, error: String(err) });
      }
    }
  },
}));
