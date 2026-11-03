import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { AdcTrackerProfile, SeedTimeRecord } from '../types';

interface TrackerState {
  profile: AdcTrackerProfile | null;
  seedTimeRecords: SeedTimeRecord[];
  isLoading: boolean;
  isSyncing: boolean;
  fetchProfile: () => Promise<void>;
  fetchSeedTimeRecords: () => Promise<void>;
  linkAccount: (username: string, passkey: string) => Promise<AdcTrackerProfile>;
  unlinkAccount: () => Promise<void>;
  syncStats: () => Promise<void>;
  calculateProjection: (uploaded: number, downloaded: number, additionalDownload: number) => Promise<number>;
}

export const useTrackerStore = create<TrackerState>((set, get) => ({
  profile: null,
  seedTimeRecords: [],
  isLoading: false,
  isSyncing: false,

  fetchProfile: async () => {
    try {
      const res = await invoke<AdcTrackerProfile>('get_adc_tracker_profile');
      set({ profile: res });
    } catch {
      set({
        profile: {
          username: '',
          masked_passkey: '',
          is_linked: false,
          announce_status: 'Unable to reach client database',
          ratio: 1.0,
          uploaded_bytes: 0,
          downloaded_bytes: 0,
          freeleech_active: false,
          bonus_points: 0,
          required_seed_time_hours: 72,
        },
      });
    }
  },

  fetchSeedTimeRecords: async () => {
    try {
      const res = await invoke<SeedTimeRecord[]>('get_seed_time_records');
      set({ seedTimeRecords: res });
    } catch {
      set({
        seedTimeRecords: [],
      });
    }
  },

  linkAccount: async (username: string, passkey: string) => {
    const res = await invoke<AdcTrackerProfile>('link_adc_account', { username, passkey });
    set({ profile: res });
    await get().fetchSeedTimeRecords();
    // Trigger a live stats fetch shortly after linking so the panel shows real
    // numbers without waiting for the background 30-minute poll cycle.
    setTimeout(() => get().syncStats(), 2000);
    return res;
  },

  unlinkAccount: async () => {
    await invoke('unlink_adc_account');
    await get().fetchProfile();
  },

  syncStats: async () => {
    set({ isSyncing: true });
    try {
      const res = await invoke<AdcTrackerProfile>('sync_adc_stats');
      set({ profile: res });
    } catch {
      // Non-fatal: background poller will retry; leave existing profile intact.
    } finally {
      set({ isSyncing: false });
    }
  },

  calculateProjection: async (uploaded: number, downloaded: number, additionalDownload: number) => {
    try {
      return await invoke<number>('calculate_ratio_projection', {
        uploaded,
        downloaded,
        additionalDownload,
      });
    } catch {
      const total = downloaded + additionalDownload;
      return total === 0 ? 1.0 : uploaded / total;
    }
  },
}));
