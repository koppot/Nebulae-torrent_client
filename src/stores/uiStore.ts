import { create } from 'zustand';
import { ViewMode } from '../types';

interface UiState {
  viewMode: ViewMode;
  theme: 'dark' | 'light';
  showSettings: boolean;
  showHelp: boolean;
  showAddTorrent: boolean;
  showCreateTorrent: boolean;
  showConnectivityDoctor: boolean;
  showVpnManager: boolean;
  showTrackerDashboard: boolean;
  splashDismissed: boolean;
  activeManagerTab: 'general' | 'files' | 'trackers' | 'peers' | 'speed' | 'log';
  managerCategoryFilter: string;
  searchQuery: string;
  initialAddTorrentSource: string | null;
  deleteModalTarget: { id: string; name: string } | null;

  setViewMode: (mode: ViewMode) => void;
  setTheme: (theme: 'dark' | 'light') => void;
  toggleTheme: () => void;
  setShowSettings: (show: boolean) => void;
  setShowHelp: (show: boolean) => void;
  setShowAddTorrent: (show: boolean) => void;
  setShowCreateTorrent: (show: boolean) => void;
  setShowConnectivityDoctor: (show: boolean) => void;
  setShowVpnManager: (show: boolean) => void;
  setShowTrackerDashboard: (show: boolean) => void;
  setSplashDismissed: (dismissed: boolean) => void;
  setActiveManagerTab: (tab: 'general' | 'files' | 'trackers' | 'peers' | 'speed' | 'log') => void;
  setManagerCategoryFilter: (filter: string) => void;
  setSearchQuery: (query: string) => void;
  setInitialAddTorrentSource: (source: string | null) => void;
  setDeleteModalTarget: (target: { id: string; name: string } | null) => void;
  openAddTorrentWithSource: (source: string) => void;
  addTorrentRejection: string | null;
  setAddTorrentRejection: (message: string | null) => void;
}

const getSavedViewMode = (): ViewMode => {
  try {
    const saved = localStorage.getItem('nebulae_view_mode');
    if (saved === 'media' || saved === 'manager' || saved === 'compact') {
      return saved;
    }
  } catch {}
  return 'manager';
};

export const useUiStore = create<UiState>((set) => ({
  viewMode: getSavedViewMode(),
  theme: 'dark',
  showSettings: false,
  showHelp: false,
  showAddTorrent: false,
  showCreateTorrent: false,
  showConnectivityDoctor: false,
  showVpnManager: false,
  showTrackerDashboard: false,
  splashDismissed: false,
  activeManagerTab: 'general',
  managerCategoryFilter: 'all',
  searchQuery: '',
  initialAddTorrentSource: null,
  deleteModalTarget: null,
  addTorrentRejection: null,

  setViewMode: (viewMode) => {
    try {
      localStorage.setItem('nebulae_view_mode', viewMode);
    } catch {}
    set({ viewMode });
  },
  setTheme: (theme) => {
    document.documentElement.classList.toggle('dark', theme === 'dark');
    set({ theme });
  },
  toggleTheme: () =>
    set((state) => {
      const nextTheme = state.theme === 'dark' ? 'light' : 'dark';
      document.documentElement.classList.toggle('dark', nextTheme === 'dark');
      return { theme: nextTheme };
    }),
  setShowSettings: (showSettings) => set({ showSettings }),
  setShowHelp: (showHelp) => set({ showHelp }),
  setShowAddTorrent: (showAddTorrent) => set({ showAddTorrent }),
  setShowCreateTorrent: (showCreateTorrent) => set({ showCreateTorrent }),
  setShowConnectivityDoctor: (showConnectivityDoctor) => set({ showConnectivityDoctor }),
  setShowVpnManager: (showVpnManager) => set({ showVpnManager }),
  setShowTrackerDashboard: (showTrackerDashboard) => set({ showTrackerDashboard }),
  setSplashDismissed: (splashDismissed) => set({ splashDismissed }),
  setActiveManagerTab: (activeManagerTab) => set({ activeManagerTab }),
  setManagerCategoryFilter: (managerCategoryFilter) => set({ managerCategoryFilter }),
  setSearchQuery: (searchQuery) => set({ searchQuery }),
  setInitialAddTorrentSource: (initialAddTorrentSource) => set({ initialAddTorrentSource }),
  setDeleteModalTarget: (deleteModalTarget) => set({ deleteModalTarget }),
  setAddTorrentRejection: (addTorrentRejection) => set({ addTorrentRejection }),
  // Single choke point for every "open this as a torrent" path (Finder open,
  // drag & drop, deep links). Only .torrent / .adc files and URL sources are
  // accepted; anything else opens the Add window with a clear rejection.
  openAddTorrentWithSource: (source: string) => {
    const trimmed = source.trim();
    const isUrlSource =
      trimmed.startsWith('magnet:') ||
      trimmed.startsWith('http://') ||
      trimmed.startsWith('https://') ||
      trimmed.startsWith('base64:') ||
      trimmed.startsWith('data:');
    const fileName = trimmed.split('/').pop()?.split('\\').pop() || trimmed;
    if (!isUrlSource && !/\.(torrent|adc)$/i.test(fileName)) {
      set({
        addTorrentRejection: `only .torrent or .adc files are accepted ('${fileName}' was ignored).`,
        showAddTorrent: true,
      });
      return;
    }
    set({
      addTorrentRejection: null,
      initialAddTorrentSource: trimmed,
      showAddTorrent: true,
    });
  },
}));
