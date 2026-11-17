import { create } from 'zustand';
import { ViewMode } from '../types';
import { openPathKey } from '../utils/openPath';

export type ManagerTab = 'general' | 'files' | 'trackers' | 'peers' | 'peer-history' | 'speed' | 'log';

export type SettingsTab =
  | 'behavior'
  | 'downloads'
  | 'connection'
  | 'speed'
  | 'bittorrent'
  | 'rss'
  | 'webui'
  | 'rtorrent'
  | 'media'
  | 'advanced';

interface UiState {
  viewMode: ViewMode;
  theme: 'dark' | 'light';
  showSettings: boolean;
  settingsTab: SettingsTab;
  showHelp: boolean;
  showAddTorrent: boolean;
  showCreateTorrent: boolean;
  showConnectivityDoctor: boolean;
  showVpnManager: boolean;
  showTrackerDashboard: boolean;
  showBugReport: boolean;
  bugReportContext?: { error?: string; category?: string } | null;
  splashDismissed: boolean;
  activeManagerTab: ManagerTab;
  managerCategoryFilter: string;
  managerTrackerFilter: string | null;
  searchQuery: string;
  initialAddTorrentSource: string | null;
  activeAddTorrentSource: string | null;
  queuedAddTorrentSources: string[];
  deleteModalTarget: { id: string; name: string } | null;

  setViewMode: (mode: ViewMode) => void;
  setTheme: (theme: 'dark' | 'light') => void;
  toggleTheme: () => void;
  setShowSettings: (show: boolean) => void;
  setSettingsTab: (tab: SettingsTab) => void;
  openSettings: (tab?: SettingsTab) => void;
  setShowHelp: (show: boolean) => void;
  setShowAddTorrent: (show: boolean) => void;
  setShowCreateTorrent: (show: boolean) => void;
  setShowConnectivityDoctor: (show: boolean) => void;
  setShowVpnManager: (show: boolean) => void;
  setShowTrackerDashboard: (show: boolean) => void;
  showQbitImport: boolean;
  setShowQbitImport: (show: boolean) => void;
  setShowBugReport: (show: boolean, ctx?: { error?: string; category?: string } | null) => void;
  setSplashDismissed: (dismissed: boolean) => void;
  setActiveManagerTab: (tab: ManagerTab) => void;
  setManagerCategoryFilter: (filter: string) => void;
  setManagerTrackerFilter: (tracker: string | null) => void;
  setSearchQuery: (query: string) => void;
  setInitialAddTorrentSource: (source: string | null) => void;
  completeAddTorrentSource: () => void;
  setDeleteModalTarget: (target: { id: string; name: string } | null) => void;
  openAddTorrentWithSource: (source: string) => void;
  openAddTorrentWithSources: (sources: string[]) => void;
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

const recentlyOpenedSources = new Map<string, number>();

export const useUiStore = create<UiState>((set, get) => ({
  viewMode: getSavedViewMode(),
  theme: 'dark',
  showSettings: false,
  settingsTab: 'behavior',
  showHelp: false,
  showAddTorrent: false,
  showCreateTorrent: false,
  showConnectivityDoctor: false,
  showVpnManager: false,
  showTrackerDashboard: false,
  showQbitImport: false,
  showBugReport: false,
  bugReportContext: null,
  splashDismissed: false,
  activeManagerTab: 'general',
  managerCategoryFilter: 'all',
  managerTrackerFilter: null,
  searchQuery: '',
  initialAddTorrentSource: null,
  activeAddTorrentSource: null,
  queuedAddTorrentSources: [],
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
  setSettingsTab: (settingsTab) => set({ settingsTab }),
  openSettings: (tab) => set({ showSettings: true, ...(tab ? { settingsTab: tab } : {}) }),
  setShowHelp: (showHelp) => set({ showHelp }),
  setShowAddTorrent: (showAddTorrent) =>
    set((state) => {
      if (!showAddTorrent) {
        return {
          showAddTorrent: false,
          activeAddTorrentSource: null,
          initialAddTorrentSource: null,
          queuedAddTorrentSources: [],
          addTorrentRejection: null,
        };
      }
      if (!state.activeAddTorrentSource && state.queuedAddTorrentSources.length > 0) {
        const [next, ...remaining] = state.queuedAddTorrentSources;
        return {
          showAddTorrent: true,
          activeAddTorrentSource: next,
          initialAddTorrentSource: next,
          queuedAddTorrentSources: remaining,
        };
      }
      return { showAddTorrent: true };
    }),
  setShowCreateTorrent: (showCreateTorrent) => set({ showCreateTorrent }),
  setShowConnectivityDoctor: (showConnectivityDoctor) => set({ showConnectivityDoctor }),
  setShowVpnManager: (showVpnManager) => set({ showVpnManager }),
  setShowTrackerDashboard: (showTrackerDashboard) => set({ showTrackerDashboard }),
  setShowQbitImport: (showQbitImport) => set({ showQbitImport }),
  setShowBugReport: (showBugReport, ctx) =>
    set({ showBugReport, bugReportContext: ctx ?? null }),
  setSplashDismissed: (splashDismissed) => set({ splashDismissed }),
  setActiveManagerTab: (activeManagerTab) => set({ activeManagerTab }),
  setManagerCategoryFilter: (managerCategoryFilter) => set({ managerCategoryFilter }),
  setManagerTrackerFilter: (managerTrackerFilter) => set({ managerTrackerFilter }),
  setSearchQuery: (searchQuery) => set({ searchQuery }),
  setInitialAddTorrentSource: (initialAddTorrentSource) => set({ initialAddTorrentSource }),
  completeAddTorrentSource: () =>
    set((state) => {
      const [next, ...remaining] = state.queuedAddTorrentSources;
      if (next) {
        return {
          showAddTorrent: true,
          activeAddTorrentSource: next,
          initialAddTorrentSource: next,
          queuedAddTorrentSources: remaining,
        };
      }
      return {
        showAddTorrent: false,
        activeAddTorrentSource: null,
        initialAddTorrentSource: null,
      };
    }),
  setDeleteModalTarget: (deleteModalTarget) => set({ deleteModalTarget }),
  setAddTorrentRejection: (addTorrentRejection) => set({ addTorrentRejection }),
  // Single choke point for every "open this as a torrent" path (Finder open,
  // drag & drop, deep links). Only .torrent / .adc files and URL sources are
  // accepted; anything else opens the Add window with a clear rejection.
  openAddTorrentWithSource: (source) => get().openAddTorrentWithSources([source]),
  openAddTorrentWithSources: (sources) => {
    const accepted: string[] = [];
    const rejected: string[] = [];
    const now = Date.now();
    const queued = get().queuedAddTorrentSources;
    const alreadyQueued = new Set([
      ...(get().activeAddTorrentSource ? [openPathKey(get().activeAddTorrentSource!)] : []),
      ...queued.map((item) => openPathKey(item)),
    ]);

    for (const source of sources) {
      const trimmed = source.trim();
      if (!trimmed) continue;
      const isUrlSource = /^(magnet|adc|https?|base64|data):/i.test(trimmed);
      const fileName = trimmed.split('/').pop()?.split('\\').pop() || trimmed;
      if (!isUrlSource && !/\.(torrent|adc)$/i.test(fileName)) {
        rejected.push(fileName);
        continue;
      }
      const sourceKey = isUrlSource ? trimmed : openPathKey(trimmed);
      if (alreadyQueued.has(sourceKey)) continue;
      const lastOpened = recentlyOpenedSources.get(sourceKey) ?? 0;
      if (now - lastOpened < 1000) continue;
      alreadyQueued.add(sourceKey);
      recentlyOpenedSources.set(sourceKey, now);
      accepted.push(trimmed);
    }

    for (const [sourceKey, timestamp] of recentlyOpenedSources) {
      if (now - timestamp >= 5000) recentlyOpenedSources.delete(sourceKey);
    }

    set((state) => {
      let nextQueue = [...state.queuedAddTorrentSources, ...accepted];
      let activeAddTorrentSource = state.activeAddTorrentSource;
      let initialAddTorrentSource = state.initialAddTorrentSource;
      let showAddTorrent = state.showAddTorrent;
      if (!activeAddTorrentSource && nextQueue.length > 0) {
        activeAddTorrentSource = nextQueue[0];
        initialAddTorrentSource = activeAddTorrentSource;
        nextQueue = nextQueue.slice(1);
        showAddTorrent = true;
      } else if (rejected.length > 0) {
        showAddTorrent = true;
      }
      return {
        showAddTorrent,
        activeAddTorrentSource,
        initialAddTorrentSource,
        queuedAddTorrentSources: nextQueue,
        addTorrentRejection: rejected.length
          ? `Unsupported drop item${rejected.length === 1 ? '' : 's'}: ${rejected.join(', ')}. Only .torrent/.adc files and magnet or HTTP(S) links can be added here.`
          : accepted.length > 0
            ? null
            : state.addTorrentRejection,
      };
    });
  },
}));
