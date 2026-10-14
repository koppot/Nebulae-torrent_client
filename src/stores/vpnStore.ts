import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { VpnMode, VpnProfile, VpnStatus, NetworkInterfaceInfo } from '../types';

interface VpnState {
  status: VpnStatus | null;
  profiles: VpnProfile[];
  interfaces: NetworkInterfaceInfo[];
  isLoading: boolean;
  fetchStatus: () => Promise<void>;
  fetchProfiles: () => Promise<void>;
  fetchInterfaces: () => Promise<void>;
  setVpnMode: (mode: VpnMode) => Promise<void>;
  toggleKillSwitch: (enabled: boolean) => Promise<void>;
  importWireguard: (content: string, name: string) => Promise<VpnProfile>;
  importOpenvpn: (content: string, name: string) => Promise<VpnProfile>;
  deleteProfile: (id: string) => Promise<void>;
}

export const useVpnStore = create<VpnState>((set, get) => ({
  status: null,
  profiles: [],
  interfaces: [],
  isLoading: false,

  fetchInterfaces: async () => {
    try {
      const res = await invoke<NetworkInterfaceInfo[]>('get_network_interfaces');
      set({ interfaces: res });
    } catch {
      set({ interfaces: [] });
    }
  },

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
