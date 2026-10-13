import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { VpnMode, VpnProfile, VpnStatus } from '../types';

interface VpnState {
  status: VpnStatus | null;
  profiles: VpnProfile[];
  isLoading: boolean;
  fetchStatus: () => Promise<void>;
  fetchProfiles: () => Promise<void>;
  setVpnMode: (mode: VpnMode) => Promise<void>;
  toggleKillSwitch: (enabled: boolean) => Promise<void>;
  importWireguard: (content: string, name: string) => Promise<VpnProfile>;
  importOpenvpn: (content: string, name: string) => Promise<VpnProfile>;
  deleteProfile: (id: string) => Promise<void>;
}

export const useVpnStore = create<VpnState>((set, get) => ({
  status: null,
  profiles: [],
  isLoading: false,

  fetchStatus: async () => {
    try {
      const res = await invoke<VpnStatus>('get_vpn_status');
      set({ status: res });
    } catch {
      set({
        status: {
          connected: false,
          kill_switch_active: true,
          kill_switch_triggered: false,
          bytes_sent: 0,
          bytes_received: 0,
          mode: 'NoVpn',
        },
      });
    }
  },

  fetchProfiles: async () => {
    try {
      const res = await invoke<VpnProfile[]>('list_vpn_profiles');
      set({ profiles: res });
    } catch {
      set({ profiles: [] });
    }
  },

  setVpnMode: async (mode: VpnMode) => {
    try {
      await invoke('set_vpn_mode', { mode });
    } catch {
      //
    }
    await get().fetchStatus();
  },

  toggleKillSwitch: async (enabled: boolean) => {
    try {
      await invoke('toggle_kill_switch', { enabled });
    } catch {
      //
    }
    await get().fetchStatus();
  },

  importWireguard: async (content: string, name: string) => {
    const profile = await invoke<VpnProfile>('import_wireguard_profile', {
      content,
      profileName: name,
    });
    await get().fetchProfiles();
    return profile;
  },

  importOpenvpn: async (content: string, name: string) => {
    const profile = await invoke<VpnProfile>('import_openvpn_profile', {
      content,
      profileName: name,
    });
    await get().fetchProfiles();
    return profile;
  },

  deleteProfile: async (id: string) => {
    try {
      await invoke('delete_vpn_profile', { id });
    } catch {
      //
    }
    await get().fetchProfiles();
  },
}));
