import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { ConnectivityReport } from '../types';

interface ConnectivityState {
  report: ConnectivityReport | null;
  isRunningTest: boolean;
  certificate: string | null;
  runDoctorTest: () => Promise<void>;
  fetchCertificate: () => Promise<void>;
}

export const useConnectivityStore = create<ConnectivityState>((set) => ({
  report: null,
  isRunningTest: false,
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

  fetchCertificate: async () => {
    try {
      const cert = await invoke<string>('get_connectivity_certificate');
      set({ certificate: cert });
    } catch {
      set({ certificate: null });
    }
  },
}));
