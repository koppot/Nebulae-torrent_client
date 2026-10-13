import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { AdcTrackerProfile, SeedTimeRecord } from '../types';

interface TrackerState {
  profile: AdcTrackerProfile | null;
  seedTimeRecords: SeedTimeRecord[];
  isLoading: boolean;
  fetchProfile: () => Promise<void>;
  fetchSeedTimeRecords: () => Promise<void>;
  linkAccount: (username: string, passkey: string) => Promise<AdcTrackerProfile>;
  unlinkAccount: () => Promise<void>;
  calculateProjection: (uploaded: number, downloaded: number, additionalDownload: number) => Promise<number>;
}

export const useTrackerStore = create<TrackerState>((set, get) => ({
  profile: null,
  seedTimeRecords: [],
  isLoading: false,

  fetchProfile: async () => {
    try {
      const res = await invoke<AdcTrackerProfile>('get_adc_tracker_profile');
      set({ profile: res });
    } catch {
      const isLinkedParam = new URLSearchParams(window.location.search).get('linked') === 'true';
      set({
        profile: {
          username: isLinkedParam ? 'ADC_Archivist' : '',
          masked_passkey: isLinkedParam ? 'adc_pk_••••••••••••••••7890' : '',
          is_linked: isLinkedParam,
          announce_status: isLinkedParam ? 'Announce OK (HTTP 200)' : 'Unlinked',
          ratio: isLinkedParam ? 2.71 : 1.0,
          uploaded_bytes: isLinkedParam ? 185_400_000_000 : 0,
          downloaded_bytes: isLinkedParam ? 68_200_000_000 : 0,
          freeleech_active: isLinkedParam,
          bonus_points: isLinkedParam ? 3420.5 : 0,
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
    return res;
  },

  unlinkAccount: async () => {
    await invoke('unlink_adc_account');
    await get().fetchProfile();
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
