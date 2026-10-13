import { create } from 'zustand';
import { UserSettings } from '../types';
import { invoke } from '@tauri-apps/api/core';

interface SettingsState {
  settings: UserSettings;
  isLoading: boolean;
  loadSettings: () => Promise<void>;
  saveSettings: (updated: Partial<UserSettings>) => Promise<void>;
}

const defaultSettings: UserSettings = {
  download_directory: '~/Downloads',
  max_download_speed_kib: 0,
  max_upload_speed_kib: 0,
  theme: 'dark',
  enable_splash: true,
  splash_message: 'Consider joining our community\nasiandvdclub.org',
  splash_url: 'https://asiandvdclub.org',
  listening_port: 6881,
  enable_upnp: true,
};

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: defaultSettings,
  isLoading: false,

  loadSettings: async () => {
    set({ isLoading: true });
    try {
      const data = await invoke<UserSettings>('load_user_settings');
      set({ settings: data, isLoading: false });
    } catch {
      // In web browser preview or test mode fallback
      set({ settings: defaultSettings, isLoading: false });
    }
  },

  saveSettings: async (updated: Partial<UserSettings>) => {
    const current = get().settings;
    const merged = { ...current, ...updated };
    set({ settings: merged });
    try {
      await invoke('save_user_settings', { settings: merged });
    } catch {
      // Fallback
    }
  },
}));
