import { create } from 'zustand';
import { ViewMode } from '../types';

interface UiState {
  viewMode: ViewMode;
  theme: 'dark' | 'light';
  showSettings: boolean;
  showHelp: boolean;
  showAddTorrent: boolean;
  showConnectivityDoctor: boolean;
  showVpnManager: boolean;
  showTrackerDashboard: boolean;
  splashDismissed: boolean;
  activeManagerTab: 'general' | 'files' | 'trackers' | 'peers' | 'speed' | 'log';
  managerCategoryFilter: string;
  searchQuery: string;

  setViewMode: (mode: ViewMode) => void;
  setTheme: (theme: 'dark' | 'light') => void;
  toggleTheme: () => void;
  setShowSettings: (show: boolean) => void;
  setShowHelp: (show: boolean) => void;
  setShowAddTorrent: (show: boolean) => void;
  setShowConnectivityDoctor: (show: boolean) => void;
  setShowVpnManager: (show: boolean) => void;
  setShowTrackerDashboard: (show: boolean) => void;
  setSplashDismissed: (dismissed: boolean) => void;
  setActiveManagerTab: (tab: 'general' | 'files' | 'trackers' | 'peers' | 'speed' | 'log') => void;
  setManagerCategoryFilter: (filter: string) => void;
  setSearchQuery: (query: string) => void;
}

export const useUiStore = create<UiState>((set) => ({
  viewMode: 'media',
  theme: 'dark',
  showSettings: false,
  showHelp: false,
  showAddTorrent: false,
  showConnectivityDoctor: false,
  showVpnManager: false,
  showTrackerDashboard: false,
  splashDismissed: false,
  activeManagerTab: 'general',
  managerCategoryFilter: 'all',
  searchQuery: '',

  setViewMode: (viewMode) => set({ viewMode }),
  setTheme: (theme) => {
    document.documentElement.classList.toggle('dark', theme === 'dark');
    set({ theme });
  },
  toggleTheme: () => set((state) => {
    const nextTheme = state.theme === 'dark' ? 'light' : 'dark';
    document.documentElement.classList.toggle('dark', nextTheme === 'dark');
    return { theme: nextTheme };
  }),
  setShowSettings: (showSettings) => set({ showSettings }),
  setShowHelp: (showHelp) => set({ showHelp }),
  setShowAddTorrent: (showAddTorrent) => set({ showAddTorrent }),
  setShowConnectivityDoctor: (showConnectivityDoctor) => set({ showConnectivityDoctor }),
  setShowVpnManager: (showVpnManager) => set({ showVpnManager }),
  setShowTrackerDashboard: (showTrackerDashboard) => set({ showTrackerDashboard }),
  setSplashDismissed: (splashDismissed) => set({ splashDismissed }),
  setActiveManagerTab: (activeManagerTab) => set({ activeManagerTab }),
  setManagerCategoryFilter: (managerCategoryFilter) => set({ managerCategoryFilter }),
  setSearchQuery: (searchQuery) => set({ searchQuery }),
}));
