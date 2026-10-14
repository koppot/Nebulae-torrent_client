import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { ConnectivityReport } from '../types';

interface ConnectivityState {
  report: ConnectivityReport | null;
  isRunningTest: boolean;
  isRetestingPort: boolean;
  certificate: string | null;
  runDoctorTest: () => Promise<void>;
  retestPort: (port: number) => Promise<boolean>;
  fetchCertificate: () => Promise<void>;
}

export const useConnectivityStore = create<ConnectivityState>((set) => ({
  report: null,
  isRunningTest: false,
  isRetestingPort: false,
  certificate: null,

  runDoctorTest: async () => {
    set({ isRunningTest: true });
    try {
      const res = await invoke<ConnectivityReport>('run_connectivity_doctor_test');
      set({ report: res, certificate: res.certificate_text, isRunningTest: false });
    } catch {
      set({ report: null, certificate: null, isRunningTest: false });
    }
  },

  retestPort: async (port: number) => {
    set({ isRetestingPort: true });
    try {
      const ok = await invoke<boolean>('retest_listening_port', { port });
      set({ isRetestingPort: false });
      return ok;
    } catch {
      set({ isRetestingPort: false });
      return false;
    }
  },

  fetchCertificate: async () => {
    try {
      const cert = await invoke<string>('get_connectivity_certificate');
      set({ certificate: cert });
    } catch {
      set({ certificate: null });
    }
  },
}));
