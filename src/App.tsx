import React, { useEffect, useState } from 'react';
import { useUiStore } from './stores/uiStore';
import { useSettingsStore } from './stores/settingsStore';
import { useTorrentStore } from './stores/torrentStore';
import { useConnectivityStore } from './stores/connectivityStore';
import { useVpnStore } from './stores/vpnStore';
import { useTrackerStore } from './stores/trackerStore';
import { SplashScreen } from './components/splash/SplashScreen';
import { TitleBar } from './components/layout/TitleBar';
import { StatusBar } from './components/layout/StatusBar';
import { TorrentManagerView } from './components/manager/TorrentManagerView';
import { SettingsModal } from './components/modals/SettingsModal';
import { HelpModal } from './components/modals/HelpModal';
import { AddTorrentModal } from './components/modals/AddTorrentModal';
import { CreateTorrentModal } from './components/modals/CreateTorrentModal';
import { ConnectivityDoctorModal } from './components/modals/ConnectivityDoctorModal';
import { VpnManagerModal } from './components/modals/VpnManagerModal';
import { TrackerModal } from './components/modals/TrackerModal';
import { DeleteTorrentModal } from './components/modals/DeleteTorrentModal';
import { FileUp } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export const App: React.FC = () => {
  const { splashDismissed, openAddTorrentWithSource } = useUiStore();
  const { loadSettings } = useSettingsStore();
  const { fetchTorrents } = useTorrentStore();
  const { runDoctorTest } = useConnectivityStore();
  const { fetchStatus: fetchVpnStatus } = useVpnStore();
  const { fetchProfile: fetchTrackerProfile } = useTrackerStore();
  const [isDragging, setIsDragging] = useState(false);

  useEffect(() => {
    loadSettings();
    fetchTorrents();
    runDoctorTest();
    fetchVpnStatus();
    fetchTrackerProfile();

    const torrentPollInterval = setInterval(() => {
      fetchTorrents();
    }, 1000);

    // Check for pending launch payload (file or protocol URI)
    invoke<string[]>('get_pending_open_payloads')
      .then((payloads) => {
        if (payloads && payloads.length > 0) {
          openAddTorrentWithSource(payloads[0]);
        }
      })
      .catch(() => {});

    // Listen for live opened URIs / files from macOS LaunchServices or secondary process
    const unlistenFns: Array<() => void> = [];

    listen<string>('nebulae://open-uri', (event) => {
      if (event.payload) {
        openAddTorrentWithSource(event.payload);
      }
    })
      .then((unlisten) => unlistenFns.push(unlisten))
      .catch(() => {});

    // Listen for native OS drag & drop events emitted by Tauri
    listen<{ paths?: string[]; position?: { x: number; y: number } }>('tauri://drag-enter', () => {
      setIsDragging(true);
    })
      .then((unlisten) => unlistenFns.push(unlisten))
      .catch(() => {});

    listen<{ paths?: string[]; position?: { x: number; y: number } }>('tauri://drag-over', () => {
      setIsDragging(true);
    })
      .then((unlisten) => unlistenFns.push(unlisten))
      .catch(() => {});

    listen<void>('tauri://drag-leave', () => {
      setIsDragging(false);
    })
      .then((unlisten) => unlistenFns.push(unlisten))
      .catch(() => {});

    listen<{ paths?: string[]; position?: { x: number; y: number } }>('tauri://drag-drop', (event) => {
      setIsDragging(false);
      const paths = event.payload?.paths;
      if (paths && paths.length > 0) {
        for (const filePath of paths) {
          openAddTorrentWithSource(filePath);
        }
      }
    })
      .then((unlisten) => unlistenFns.push(unlisten))
      .catch(() => {});

    listen<string>('nebulae://file-dropped', (event) => {
      setIsDragging(false);
      if (event.payload) {
        openAddTorrentWithSource(event.payload);
      }
    })
      .then((unlisten) => unlistenFns.push(unlisten))
      .catch(() => {});

    const params = new URLSearchParams(window.location.search);
    const viewParam = params.get('view');
    const modalParam = params.get('modal');
    const tabParam = params.get('tab');
    const splashParam = params.get('splash');

    if (splashParam === 'true') {
      useUiStore.getState().setSplashDismissed(false);
    } else if (splashParam === 'false') {
      useUiStore.getState().setSplashDismissed(true);
    }

    if (viewParam === 'manager') {
      useUiStore.getState().setViewMode('manager');
    }

    if (tabParam === 'peers' || tabParam === 'files' || tabParam === 'trackers' || tabParam === 'general') {
      useUiStore.getState().setActiveManagerTab(tabParam as any);
    }

    if (modalParam === 'doctor') {
      useUiStore.getState().setShowConnectivityDoctor(true);
    } else if (modalParam === 'vpn') {
      useUiStore.getState().setShowVpnManager(true);
    } else if (modalParam === 'tracker') {
      useUiStore.getState().setShowTrackerDashboard(true);
    } else if (modalParam === 'settings') {
      useUiStore.getState().setShowSettings(true);
    } else if (modalParam === 'help') {
      useUiStore.getState().setShowHelp(true);
    } else if (modalParam === 'add') {
      useUiStore.getState().setShowAddTorrent(true);
    } else if (modalParam === 'create') {
      useUiStore.getState().setShowCreateTorrent(true);
    }

    const handleKeyDown = (e: KeyboardEvent) => {
      const isCmdOrCtrl = e.metaKey || e.ctrlKey;
      if (isCmdOrCtrl && e.key.toLowerCase() === 'n') {
        e.preventDefault();
        useUiStore.getState().setShowCreateTorrent(true);
      } else if (isCmdOrCtrl && e.key.toLowerCase() === 'o') {
        e.preventDefault();
        useUiStore.getState().setShowAddTorrent(true);
      } else if (isCmdOrCtrl && e.key === ',') {
        e.preventDefault();
        useUiStore.getState().setShowSettings(true);
      }
    };
    window.addEventListener('keydown', handleKeyDown);

    return () => {
      clearInterval(torrentPollInterval);
      window.removeEventListener('keydown', handleKeyDown);
      for (const un of unlistenFns) un();
    };
  }, [loadSettings, fetchTorrents, runDoctorTest, fetchVpnStatus, fetchTrackerProfile, openAddTorrentWithSource]);

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(true);
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      const file = e.dataTransfer.files[0];
      const source = (file as any).path || file.name;
      openAddTorrentWithSource(source);
    }
  };

  return (
    <div
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
      className="relative flex flex-col h-screen w-screen bg-[#141414] text-white overflow-hidden font-sans select-none"
    >
      {/* 1. Splash Screen */}
      {!splashDismissed && <SplashScreen />}

      {/* 2. Top TitleBar & Navigation */}
      <TitleBar />

      {/* 3. Main View Mode Area */}
      <main className="flex-1 flex overflow-hidden">
        <TorrentManagerView />
      </main>

      {/* 4. Bottom Desktop Application Status Bar */}
      <StatusBar />

      {/* 4. Drag & Drop Full-Window Visual Overlay */}
      {isDragging && (
        <div className="absolute inset-0 z-50 bg-black/90 border-4 border-dashed border-[#e50914] flex flex-col items-center justify-center pointer-events-none">
          <div className="p-4 rounded-full bg-[#e50914]/20 text-[#e50914] mb-3 animate-bounce">
            <FileUp className="w-12 h-12" />
          </div>
          <h3 className="text-lg font-bold text-white mb-1">Drop Torrent to Add</h3>
          <p className="text-xs text-[#a3a3a3] font-mono">.torrent or .adc file</p>
        </div>
      )}

      {/* 5. Modals */}
      <SettingsModal />
      <HelpModal />
      <AddTorrentModal />
      <CreateTorrentModal />
      <ConnectivityDoctorModal />
      <VpnManagerModal />
      <TrackerModal />
      <DeleteTorrentModal />
    </div>
  );
};

export default App;
