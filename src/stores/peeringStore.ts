import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { PeerTelemetry, PeeringSettings } from '../types';

interface PeeringState {
  settings: PeeringSettings | null;
  peers: PeerTelemetry[];
  isLoading: boolean;
  fetchSettings: () => Promise<void>;
  saveSettings: (settings: PeeringSettings) => Promise<void>;
  fetchPeers: (torrentId: string) => Promise<void>;
}

export const usePeeringStore = create<PeeringState>((set) => ({
  settings: null,
  peers: [],
  isLoading: false,

  fetchSettings: async () => {
    try {
      const res = await invoke<PeeringSettings>('get_peering_settings');
      set({ settings: res });
    } catch {
      set({
        settings: {
          mode: 'Balanced',
          enable_locality_preference: true,
          enable_geographic_metadata: true,
          enable_rate_limited_probes: false,
          max_connections_per_torrent: 50,
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

  fetchPeers: async (torrentId: string) => {
    set({ isLoading: true });
    try {
      const res = await invoke<PeerTelemetry[]>('get_peer_telemetry', { torrentId });
      set({ peers: res, isLoading: false });
    } catch {
      set({ peers: [], isLoading: false });
    }
  },
}));
