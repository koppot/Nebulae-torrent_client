import React, { useEffect, useState } from 'react';
import { useUiStore, ManagerTab } from './stores/uiStore';
import { usePlayerStore, playerStore, type PlayerOpenInfo } from './lib/playerStore';
import { FileWithPath } from './types';
import { useSettingsStore } from './stores/settingsStore';
import { useTorrentStore, smoothTelemetrySpeeds, reconcileRecheckState } from './stores/torrentStore';
import { useConnectivityStore } from './stores/connectivityStore';
import { useVpnStore } from './stores/vpnStore';
import { useTrackerStore } from './stores/trackerStore';
import { SplashScreen } from './components/splash/SplashScreen';
import { TitleBar } from './components/layout/TitleBar';
import { StatusBar } from './components/layout/StatusBar';
import { TorrentManagerView } from './components/manager/TorrentManagerView';
import { SimpleMediaView } from './components/media/SimpleMediaView';
import { CompactBarView } from './components/compact/CompactBarView';
import { SettingsModal } from './components/modals/SettingsModal';
import { HelpModal } from './components/modals/HelpModal';
import { AddTorrentModal } from './components/modals/AddTorrentModal';
import { CreateTorrentModal } from './components/modals/CreateTorrentModal';
import { ConnectivityDoctorModal } from './components/modals/ConnectivityDoctorModal';
import { PortForwardModal } from './components/modals/PortForwardModal';
import { VpnManagerModal } from './components/modals/VpnManagerModal';
import { TrackerModal } from './components/modals/TrackerModal';
import { DeleteTorrentModal } from './components/modals/DeleteTorrentModal';
import { RecoveryScreen } from './components/recovery/RecoveryScreen';
import PlayerOverlay from './components/player/PlayerOverlay';
import { FileUp, Film } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export const App: React.FC = () => {
  const { splashDismissed, openAddTorrentWithSource, viewMode, setViewMode } = useUiStore();
  const playerOpen = usePlayerStore().open;

  // The webview itself must go transparent while the player is up — the
  // native video view sits BELOW it (see embed_macos.rs).
  useEffect(() => {
    document.body.classList.toggle('player-active', playerOpen);
  }, [playerOpen]);

  // Programmatic playback (engine-side autoplay, diagnostics) opens the
  // player via a backend event rather than a frontend invoke.
  // Feature label (clean title + year) for engine-adopted sessions: the
  // engine only knows the disc folder name, so look the torrent up by the
  // session path and use its media metadata.
  const featureLabelForPath = (p: string): string | undefined => {
    const torrents = useTorrentStore.getState().torrents;
    // MOST-SPECIFIC match wins: a torrent saved at a broad parent (e.g. a
    // whole volume) must not label every external disc under it with its
    // own title - the longest save_path that still contains the session
    // path identifies the release.
    let best: { root: string; label: string } | null = null;
    for (const t of torrents) {
      const root = t.save_path?.replace(/\/+$/, '');
      if (!root || !p.startsWith(root)) continue;
      const mm = t.media_meta;
      const label = mm?.clean_title
        ? mm.year ? `${mm.clean_title} (${mm.year})` : mm.clean_title
        : t.name;
      if (!best || root.length > best.root.length) best = { root, label };
    }
    return best?.label;
  };
  useEffect(() => {
    const un = listen<PlayerOpenInfo>('player-opened', (e) => {
      invoke('log_frontend', { message: `[player-opened] external=${e.payload.external}` }).catch(() => {});
      if (e.payload.external) return;
      // The event repeats several times per open; an explicit open already
      // set the correct display title for THIS path (e.g. the import
      // prompt's "Name (Year)") - don't overwrite it with a torrent guess.
      const cur = playerStore.get();
      if (cur.open && cur.resolvedPath === e.payload.resolved_path) return;
      playerStore.open(e.payload, featureLabelForPath(e.payload.resolved_path));
    });
    // Belt-and-braces: poll the engine for a live session while the player
    // UI is closed. Event delivery raced the webview's own loading in the
    // autoplay path (owner: player UI never appeared).
    const iv = window.setInterval(() => {
      if (playerStore.get().open) return;
      invoke<{ resolved_path: string; title: string; is_disc: boolean } | null>(
        'player_engine_playback',
      )
        .then((s) => {
          if (s && !playerStore.get().open) {
            playerStore.open(
              {
                resolved_path: s.resolved_path,
                title: s.title,
                is_disc: s.is_disc,
                external: false,
              },
              featureLabelForPath(s.resolved_path),
            );
          }
        })
        .catch(() => {});
    }, 1000);
    return () => {
      un.then((f) => f()).catch(() => {});
      window.clearInterval(iv);
    };
  }, []);
  const { settings, loadSettings, fetchEngineMode, saveSettings, isLoading: settingsLoading } = useSettingsStore();
  const { fetchTorrents } = useTorrentStore();
  const { runDoctorTest } = useConnectivityStore();
  const { fetchStatus: fetchVpnStatus } = useVpnStore();
  const { fetchProfile: fetchTrackerProfile } = useTrackerStore();
  const [isDragging, setIsDragging] = useState(false);
  const [engineInitError, setEngineInitError] = useState<string | null>(null);
  const [showFirstRunMediaModal, setShowFirstRunMediaModal] = useState(false);

  useEffect(() => {
    if (!settingsLoading && !settings.media_mode_decision_made) {
      setShowFirstRunMediaModal(true);
    }
  }, [settingsLoading, settings.media_mode_decision_made]);

  useEffect(() => {
    invoke<string | null>('get_engine_init_error')
      .then((err) => {
        if (err) {
          setEngineInitError(err);
        }
      })
      .catch(() => {});

    loadSettings();
    fetchEngineMode();
    fetchTorrents();
    runDoctorTest();
    fetchVpnStatus();
    fetchTrackerProfile();

    // Listen for live opened URIs / files from macOS LaunchServices or secondary process
    const unlistenFns: Array<() => void> = [];

    // Real-time 500ms push telemetry from Rust engine (speeds are
    // display-smoothed; raw engine rates burst between full speed and 0)
    listen<import('./types').TorrentItem[]>('nebulae://telemetry', (event) => {
      if (event.payload) {
        useTorrentStore.setState({
          torrents: smoothTelemetrySpeeds(reconcileRecheckState(event.payload)),
          isLoading: false,
        });
      }
    })
      .then((unlisten) => unlistenFns.push(unlisten))
      .catch(() => {});

    // Relaxed background heartbeat fallback
    const torrentPollInterval = setInterval(() => {
      fetchTorrents();
    }, 5000);

    // Check for pending launch payload (file or protocol URI)
    invoke<string[]>('get_pending_open_payloads')
      .then((payloads) => {
        if (payloads && payloads.length > 0) {
          openAddTorrentWithSource(payloads[0]);
        }
      })
      .catch(() => {});

    listen<string>('nebulae://open-uri', (event) => {
      if (event.payload) {
        openAddTorrentWithSource(event.payload);
      }
    })
      .then((unlisten) => unlistenFns.push(unlisten))
      .catch(() => {});

    // Tray menu actions that open specific windows
    listen<string>('nebulae://tray-action', (event) => {
      if (event.payload === 'add-torrent') {
        useUiStore.getState().setShowAddTorrent(true);
      } else if (event.payload === 'create-torrent') {
        useUiStore.getState().setShowCreateTorrent(true);
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
    } else if (localStorage.getItem('nebulae_pref_skip_splash') === '1') {
      // Owner preference: start without the splash screen.
      useUiStore.getState().setSplashDismissed(true);
    }

    if (viewParam === 'manager' || viewParam === 'media' || viewParam === 'compact') {
      useUiStore.getState().setViewMode(viewParam);
    }

    if (tabParam === 'peers' || tabParam === 'files' || tabParam === 'trackers' || tabParam === 'general') {
      useUiStore.getState().setActiveManagerTab(tabParam as ManagerTab);
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
  }, [loadSettings, fetchEngineMode, fetchTorrents, runDoctorTest, fetchVpnStatus, fetchTrackerProfile, openAddTorrentWithSource]);

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
      const source = (file as FileWithPath).path || file.name;
      openAddTorrentWithSource(source);
    }
  };

  if (engineInitError) {
    return <RecoveryScreen error={engineInitError} />;
  }

  return (
    <div
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
      className={`relative flex flex-col h-screen w-screen text-white overflow-hidden font-sans select-none ${
        playerOpen ? '' : 'bg-[#141414]'
      }`}
    >
      {/* The embedded player replaces the whole interface: the video is the
          native view UNDER this transparent webview, so nothing opaque may
          be painted behind it. */}
      {!playerOpen && (
        <>
      {/* 1. Splash Screen */}
      {!splashDismissed && <SplashScreen />}

      {/* 2. Top TitleBar & Navigation */}
      <TitleBar />

      {/* 3. Main View Mode Area */}
      <main className="flex-1 flex overflow-hidden">
        {viewMode === 'media' && <SimpleMediaView />}
        {viewMode === 'compact' && <CompactBarView />}
        {viewMode === 'manager' && <TorrentManagerView />}
      </main>

      {/* 4. Bottom Desktop Application Status Bar (Manager view only) */}
      {viewMode === 'manager' && <StatusBar />}
        </>
      )}

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
      <PlayerOverlay />
      <AddTorrentModal />
      <CreateTorrentModal />
      <ConnectivityDoctorModal />
      <PortForwardModal />
      <VpnManagerModal />
      <TrackerModal />
      <DeleteTorrentModal />

      {/* First-Run Media Mode Choice */}
      {splashDismissed && showFirstRunMediaModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm p-4">
          <div className="bg-[#181818] border border-[#333333] rounded-xl p-6 max-w-md w-full shadow-2xl space-y-4">
            <div className="flex items-center gap-3">
              <div className="p-2.5 rounded-lg bg-[#e50914]/10 text-[#e50914] border border-[#e50914]/20">
                <Film className="w-6 h-6" />
              </div>
              <div>
                <h3 className="text-base font-bold text-white">Would you like to enable media mode?</h3>
              </div>
            </div>

            <p className="text-xs text-[#cccccc] leading-relaxed">
              Nebulae’s Media Center lets you play DVDs, Blu-ray, and UHD discs with interactive disc menus, while organizing and managing your media collection in one place.
            </p>

            <div className="flex items-center justify-end gap-3 pt-2">
              <button
                type="button"
                onClick={() => {
                  void saveSettings({ media_mode_enabled: false, media_mode_decision_made: true });
                  setViewMode('manager');
                  localStorage.setItem('nebulae_media_mode_choice', 'no');
                  setShowFirstRunMediaModal(false);
                }}
                className="px-4 py-2 rounded-lg bg-[#262626] hover:bg-[#333333] border border-[#333333] text-xs text-[#a3a3a3] hover:text-white font-medium transition"
              >
                No
              </button>
              <button
                type="button"
                autoFocus
                onClick={() => {
                  void saveSettings({ media_mode_enabled: true, media_mode_decision_made: true });
                  setViewMode('media');
                  localStorage.setItem('nebulae_media_mode_choice', 'yes');
                  setShowFirstRunMediaModal(false);
                }}
                className="px-4 py-2 rounded-lg bg-[#e50914] hover:bg-[#f40612] text-xs text-white font-semibold shadow-sm transition ring-1 ring-white/20"
              >
                Yes
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default App;
