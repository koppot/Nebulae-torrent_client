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

export const App: React.FC = () => {
  const { viewMode, splashDismissed } = useUiStore();
  const { loadSettings } = useSettingsStore();
  const { fetchTorrents, addTorrent } = useTorrentStore();
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
  }, [loadSettings, fetchTorrents, runDoctorTest, fetchVpnStatus, fetchTrackerProfile]);

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
      await addTorrent(file.name);
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
          <h3 className="text-xl font-bold text-white mb-1">Drop .torrent to Start Transfer</h3>
          <p className="text-xs text-adc-muted font-mono">Payload will be placed into default downloads</p>
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
