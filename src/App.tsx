import React, { useEffect, useState } from 'react';
import { useUiStore } from './stores/uiStore';
import { useSettingsStore } from './stores/settingsStore';
import { useTorrentStore } from './stores/torrentStore';
import { useConnectivityStore } from './stores/connectivityStore';
import { useVpnStore } from './stores/vpnStore';
import { useTrackerStore } from './stores/trackerStore';
import { SplashScreen } from './components/splash/SplashScreen';
import { TitleBar } from './components/layout/TitleBar';
import { SimpleMediaView } from './components/media/SimpleMediaView';
import { TorrentManagerView } from './components/manager/TorrentManagerView';
import { SettingsModal } from './components/modals/SettingsModal';
import { HelpModal } from './components/modals/HelpModal';
import { AddTorrentModal } from './components/modals/AddTorrentModal';
import { ConnectivityDoctorModal } from './components/modals/ConnectivityDoctorModal';
import { VpnManagerModal } from './components/modals/VpnManagerModal';
import { TrackerModal } from './components/modals/TrackerModal';
import { FileUp } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export const App: React.FC = () => {
  const { viewMode, splashDismissed, openAddTorrentWithSource } = useUiStore();
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
    } else if (splashParam === 'false' || viewParam || modalParam) {
      useUiStore.getState().setSplashDismissed(true);
    }

    if (viewParam === 'manager') {
      useUiStore.getState().setViewMode('manager');
    } else if (viewParam === 'media') {
      useUiStore.getState().setViewMode('media');
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
    }

    return () => {
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
      className="relative flex flex-col h-screen w-screen adc-watch-bg text-adc-ivory overflow-hidden font-sans select-none"
    >
      {/* 1. Cinematic Splash Screen */}
      {!splashDismissed && <SplashScreen />}

      {/* 2. Top TitleBar & Navigation */}
      <TitleBar />

      {/* 3. Main View Mode Area */}
      <main className="flex-1 flex overflow-hidden">
        {viewMode === 'media' ? <SimpleMediaView /> : <TorrentManagerView />}
      </main>

      {/* 4. Drag & Drop Full-Window Visual Overlay */}
      {isDragging && (
        <div className="absolute inset-0 z-50 bg-black/90 border-4 border-dashed border-adc-crimson backdrop-blur-md flex flex-col items-center justify-center pointer-events-none">
          <div className="p-4 rounded-full bg-adc-crimson/20 text-adc-crimson-light mb-4 animate-bounce">
            <FileUp className="w-12 h-12" />
          </div>
          <h3 className="text-xl font-bold text-white mb-1">Drop .torrent or .adc to Start Transfer</h3>
          <p className="text-xs text-adc-muted font-mono">Payload will be configured for download</p>
        </div>
      )}

      {/* 5. Modals */}
      <SettingsModal />
      <HelpModal />
      <AddTorrentModal />
      <ConnectivityDoctorModal />
      <VpnManagerModal />
      <TrackerModal />
    </div>
  );
};

export default App;
