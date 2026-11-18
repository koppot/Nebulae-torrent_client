import React, { useCallback, useEffect, useRef, useState } from 'react';
import { useUiStore, ManagerTab } from './stores/uiStore';
import { usePlayerStore, playerStore, noteEngineGen, isStaleEngineGen, openInPlayer, type PlayerOpenInfo } from './lib/playerStore';
import { FileWithPath } from './types';
import { useSettingsStore } from './stores/settingsStore';
import { useTorrentStore } from './stores/torrentStore';
import { useConnectivityStore } from './stores/connectivityStore';
import { useVpnStore } from './stores/vpnStore';
import { useTrackerStore } from './stores/trackerStore';
import { useLibraryStore } from './stores/libraryStore';
import { normalizeOpenPath, openPathKey, readTorrentFileAsDataUrl } from './utils/openPath';
import { SplashScreen } from './components/splash/SplashScreen';
import { TitleBar } from './components/layout/TitleBar';
import { StatusBar } from './components/layout/StatusBar';
import { TorrentManagerView } from './components/manager/TorrentManagerView';
import { SimpleMediaView } from './components/media/SimpleMediaView';
import { CompactBarView } from './components/compact/CompactBarView';
import { RemoteServerView } from './components/server/RemoteServerView';
import { SettingsModal } from './components/modals/SettingsModal';
import {
  addDeliveryFailure,
  classifyDeliveryFailure,
  clearDismissalsForToken,
  dismissalKey,
  extractDeliveryTokenFromUri,
  type DeliveryFailure,
} from './lib/deliveryFailures';
import { HelpModal } from './components/modals/HelpModal';
import { BugReportModal } from './components/modals/BugReportModal';
import { AddTorrentModal } from './components/modals/AddTorrentModal';
import { CreateTorrentModal } from './components/modals/CreateTorrentModal';
import { ConnectivityDoctorModal } from './components/modals/ConnectivityDoctorModal';
import { PortForwardModal } from './components/modals/PortForwardModal';
import { VpnManagerModal } from './components/modals/VpnManagerModal';
import { TrackerModal } from './components/modals/TrackerModal';
import { DeleteTorrentModal } from './components/modals/DeleteTorrentModal';
import { QbitImportModal } from './components/modals/QbitImportModal';
import { RecoveryScreen } from './components/recovery/RecoveryScreen';
import PlayerOverlay from './components/player/PlayerOverlay';
import { FileUp, Film, GripHorizontal } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useRemoteServerStore } from './stores/remoteServerStore';

export const App: React.FC = () => {
  const { splashDismissed, viewMode, setViewMode } = useUiStore();
  const playerOpen = usePlayerStore().open;
  const theme = useSettingsStore((s) => s.settings.theme);
  const remoteServerEnabled = useSettingsStore((s) => s.settings.rtorrent_server_enabled);
  const remoteServerUrl = useSettingsStore((s) => s.settings.rtorrent_server_url);
  const remoteServerToken = useSettingsStore((s) => s.settings.rtorrent_server_token);

  // Apply the persisted preference at the document root so existing surfaces
  // can share one palette without changing their layout or behavior.
  useEffect(() => {
    const root = document.documentElement;
    const light = theme === 'light';
    root.dataset.theme = light ? 'light' : 'dark';
    root.style.colorScheme = light ? 'light' : 'dark';
    root.classList.toggle('dark', !light);
  }, [theme]);

  // Start or stop remote server polling whenever the connection settings change.
  useEffect(() => {
    if (remoteServerEnabled && remoteServerUrl?.trim()) {
      useRemoteServerStore.getState().configure();
    } else {
      useRemoteServerStore.getState().stopPolling();
    }
  }, [remoteServerEnabled, remoteServerUrl, remoteServerToken]);

  // Handle nebulae:open-settings events dispatched by child components (e.g. RemoteServerView).
  const { setShowSettings: openSettingsModal, setSettingsTab: openSettingsTab } = useUiStore();
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tab?: string }>).detail;
      if (detail?.tab) openSettingsTab(detail.tab as Parameters<typeof openSettingsTab>[0]);
      openSettingsModal(true);
    };
    window.addEventListener('nebulae:open-settings', handler);
    return () => window.removeEventListener('nebulae:open-settings', handler);
  }, [openSettingsModal, openSettingsTab]);

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
    if (!p) return undefined;
    const norm = (s: string) => s.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
    const filePath = norm(p);
    const torrents = useTorrentStore.getState().torrents;
    let best: { len: number; label: string } | null = null;
    for (const t of torrents) {
      const candidates: string[] = [];
      if (t.content_path) candidates.push(norm(t.content_path));
      if (t.save_path && t.raw_name) candidates.push(norm(`${t.save_path}/${t.raw_name}`));
      if (t.save_path && t.name) candidates.push(norm(`${t.save_path}/${t.name}`));
      for (const root of candidates) {
        if (filePath === root || filePath.startsWith(root + '/')) {
          const mm = t.media_meta;
          const label = mm?.clean_title
            ? (mm.year ? `${mm.clean_title} (${mm.year})` : mm.clean_title)
            : t.name;
          if (!best || root.length > best.len) {
            best = { len: root.length, label };
          }
        }
      }
    }
    return best?.label;
  };
  useEffect(() => {
    const un = listen<PlayerOpenInfo>('player-opened', (e) => {
      const isStale = isStaleEngineGen(e.payload.gen);
      const cur = playerStore.get();
      invoke('log_frontend', {
        message: `[frontend-diag] player-opened received: gen=${e.payload.gen} path=${e.payload.resolved_path} external=${e.payload.external} isStale=${isStale} curOpen=${cur.open} curGen=${cur.gen} curPath=${cur.resolvedPath}`,
      }).catch(() => {});
      if (e.payload.external) return;
      noteEngineGen(e.payload.gen);
      // The event repeats at 0/1.5/4/8s after open: a repeat that outlives
      // a user close belongs to a dead session - never let it reopen the
      // player UI.
      if (isStale) return;
      // The event repeats several times per open; an explicit open already
      // set the correct display title for THIS path (e.g. the import
      // prompt's "Name (Year)") - don't overwrite it with a torrent guess.
      if (cur.open && cur.resolvedPath === e.payload.resolved_path) return;
      invoke('log_frontend', {
        message: `[frontend-diag] playerStore.open invoked from player-opened: gen=${e.payload.gen} path=${e.payload.resolved_path}`,
      }).catch(() => {});
      const isSamePath = cur.open && (
        cur.resolvedPath === e.payload.resolved_path ||
        cur.resolvedPath.startsWith(e.payload.resolved_path) ||
        e.payload.resolved_path.startsWith(cur.resolvedPath)
      );
      const titleToPass = featureLabelForPath(e.payload.resolved_path) || (isSamePath ? cur.title : undefined);
      const yearToPass = isSamePath ? cur.year : undefined;
      playerStore.open(e.payload, titleToPass, yearToPass);
    });
    const unClosed1 = listen('player-closed', () => {
      invoke('log_frontend', { message: `[frontend-diag] player-closed received` }).catch(() => {});
      playerStore.close();
    });
    const unClosed2 = listen('player://closed', () => {
      invoke('log_frontend', { message: `[frontend-diag] player://closed received` }).catch(() => {});
      playerStore.close();
    });
    // Belt-and-braces: poll the engine for a live session while the player
    // UI is closed. Event delivery raced the webview's own loading in the
    // autoplay path (owner: player UI never appeared).
    const iv = window.setInterval(() => {
      const ps = playerStore.get();
      if (ps.open) return;
      invoke<{ resolved_path: string; title: string; is_disc: boolean; gen?: number | null } | null>(
        'player_engine_playback',
      )
        .then((s) => {
          if (!s) return;
          noteEngineGen(s.gen);
          // A response captured before/during a close (the poll was in
          // flight while the session was torn down) must not reopen the UI.
          if (isStaleEngineGen(s.gen)) return;
          if (!playerStore.get().open) {
            invoke('log_frontend', {
              message: `[frontend-diag] playerStore.open adopted via engine poll: gen=${s.gen} path=${s.resolved_path}`,
            }).catch(() => {});
            const cur = playerStore.get();
            const isSamePath = cur.open && (
              cur.resolvedPath === s.resolved_path ||
              cur.resolvedPath.startsWith(s.resolved_path) ||
              s.resolved_path.startsWith(cur.resolvedPath)
            );
            playerStore.open(
              {
                resolved_path: s.resolved_path,
                title: s.title,
                is_disc: s.is_disc,
                external: false,
                gen: s.gen,
              },
              featureLabelForPath(s.resolved_path) || (isSamePath ? cur.title : undefined),
              isSamePath ? cur.year : undefined,
            );
          }
        })
        .catch(() => {});
    }, 1000);
    return () => {
      un.then((f) => f()).catch(() => {});
      unClosed1.then((f) => f()).catch(() => {});
      unClosed2.then((f) => f()).catch(() => {});
      window.clearInterval(iv);
    };
  }, []);
  const { settings, loadSettings, fetchEngineMode, saveSettings, isLoaded: settingsLoaded } = useSettingsStore();
  const { fetchTorrents } = useTorrentStore();
  const { runDoctorTest } = useConnectivityStore();
  const { fetchStatus: fetchVpnStatus } = useVpnStore();
  const { fetchProfile: fetchTrackerProfile } = useTrackerStore();
  const [isDragging, setIsDragging] = useState(false);
  const [pendingMediaPaths, setPendingMediaPaths] = useState<string[]>([]);
  const [mediaImportPrompt, setMediaImportPrompt] = useState<{
    path: string;
    title: string;
    error?: string;
  } | null>(null);
  const [isImportingMedia, setIsImportingMedia] = useState(false);
  const [openPathErrors, setOpenPathErrors] = useState<string[]>([]);
  const [deliveryFailures, setDeliveryFailures] = useState<DeliveryFailure[]>([]);
  const dismissedDeliveryFailuresRef = useRef(new Set<string>());
  const pendingMediaKeysRef = useRef(new Set<string>());
  const activeMediaKeyRef = useRef<string | null>(null);
  const openingDroppedMediaRef = useRef(false);
  const recentPathDispatchRef = useRef(new Map<string, number>());
  const [engineInitError, setEngineInitError] = useState<string | null>(null);
  const [showFirstRunMediaModal, setShowFirstRunMediaModal] = useState(false);
  const [savingMediaMode, setSavingMediaMode] = useState(false);
  const [mediaModeError, setMediaModeError] = useState<string | null>(null);

  // Drag position for Media Mode prompt modal
  const [modalOffset, setModalOffset] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const dragRef = React.useRef<{
    isDragging: boolean;
    startX: number;
    startY: number;
    initialOffsetX: number;
    initialOffsetY: number;
  }>({
    isDragging: false,
    startX: 0,
    startY: 0,
    initialOffsetX: 0,
    initialOffsetY: 0,
  });

  const dismissDeliveryFailures = useCallback(() => {
    setDeliveryFailures((current) => {
      for (const failure of current) dismissedDeliveryFailuresRef.current.add(dismissalKey(failure));
      return [];
    });
  }, []);

  const retryDeliveryFailure = useCallback(async (failure: DeliveryFailure) => {
    if (!failure.token) return;
    clearDismissalsForToken(dismissedDeliveryFailuresRef.current, failure.token);
    setDeliveryFailures((current) => current.filter((f) => f.key !== failure.key));
    try {
      await invoke<string>('redeem_adc_delivery', { requestToken: failure.token });
      await useTorrentStore.getState().fetchTorrents();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setDeliveryFailures((current) =>
        addDeliveryFailure(current, dismissedDeliveryFailuresRef.current, {
          token: failure.token,
          title: failure.title,
          reason: msg,
        }),
      );
    }
  }, []);

  const dispatchOpenPaths = useCallback(async (paths: string[]) => {
    const torrentSources: string[] = [];
    const adcUris: string[] = [];
    const unsupported: string[] = [];
    const now = Date.now();

    for (const rawPath of paths) {
      const source = normalizeOpenPath(rawPath.trim());
      if (!source) continue;
      const sourceName = source.split(/[?#]/, 1)[0].split(/[\\/]/).pop() || source;

      // Separate manual adc:// references from opaque website delivery tokens (E11 / G13 / D12).
      // Only authenticated, account/device-bound website delivery tokens bypass AddTorrentModal.
      if (/^adc:/i.test(source)) {
        if (/^adc:(?:\/\/)?(?:delivery|request)(?:\/|\?|$)/i.test(source)) {
          adcUris.push(source);
        } else {
          torrentSources.push(source);
        }
        continue;
      }

      if (
        /^(magnet|https?|base64|data):/i.test(source) ||
        /\.(torrent|adc)$/i.test(sourceName)
      ) {
        torrentSources.push(source);
        continue;
      }

      const key = openPathKey(source);
      const lastDispatch = recentPathDispatchRef.current.get(key) ?? 0;
      if (now - lastDispatch < 1000) continue;
      recentPathDispatchRef.current.set(key, now);

      try {
        const kind = await invoke<'disc' | 'video' | 'unsupported'>(
          'classify_dropped_media_path',
          { filePath: source },
        );
        if (kind === 'disc' || kind === 'video') {
          if (!pendingMediaKeysRef.current.has(key)) {
            pendingMediaKeysRef.current.add(key);
            setPendingMediaPaths((queued) => [...queued, source]);
          }
        } else {
          unsupported.push(sourceName);
        }
      } catch {
        unsupported.push(sourceName);
        recentPathDispatchRef.current.delete(key);
      }
    }

    for (const [path, timestamp] of recentPathDispatchRef.current) {
      if (now - timestamp >= 5000) recentPathDispatchRef.current.delete(path);
    }

    // Redeem opaque website-created delivery tokens (adc://delivery/<64-hex>).
    // Manual adc://torrent/<id> URIs remain in torrentSources and open AddTorrentModal.
    for (const uri of adcUris) {
      const uriToken = extractDeliveryTokenFromUri(uri);
      // A fresh click on the website is a deliberate retry, so earlier dismissals no longer apply.
      if (uriToken) clearDismissalsForToken(dismissedDeliveryFailuresRef.current, uriToken);
      try {
        await invoke<string>('redeem_adc_delivery', { requestToken: uri });
        await useTorrentStore.getState().fetchTorrents();
        invoke('log_frontend', {
          message: `[adc-delivery] Redeemed website delivery URI ${uri}`,
        }).catch(() => {});
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        invoke('log_frontend', {
          message: `[adc-delivery] Failed to redeem ${uri}: ${msg}`,
        }).catch(() => {});
        setDeliveryFailures((current) =>
          addDeliveryFailure(current, dismissedDeliveryFailuresRef.current, {
            token: uriToken,
            reason: msg,
          }),
        );
      }
    }

    if (torrentSources.length > 0) {
      useUiStore.getState().openAddTorrentWithSources(torrentSources);
    }
    if (unsupported.length > 0) {
      setOpenPathErrors((current) => [...current, ...unsupported].slice(-10));
    }
  }, []);

  useEffect(() => {
    if (!playerOpen && activeMediaKeyRef.current) {
      pendingMediaKeysRef.current.delete(activeMediaKeyRef.current);
      activeMediaKeyRef.current = null;
    }
  }, [playerOpen]);

  useEffect(() => {
    if (playerOpen || mediaImportPrompt || openingDroppedMediaRef.current) return;
    const path = pendingMediaPaths[0];
    if (!path) return;

    const key = openPathKey(path);
    openingDroppedMediaRef.current = true;
    activeMediaKeyRef.current = key;
    setPendingMediaPaths((queued) => queued.slice(1));
    void openInPlayer(path, { deferLibraryImport: true })
      .then((info) => {
        if (info.external || !playerStore.get().open) {
          pendingMediaKeysRef.current.delete(key);
          if (activeMediaKeyRef.current === key) activeMediaKeyRef.current = null;
        }
        setMediaImportPrompt({
          path,
          title: info.title || path.split(/[\\/]/).pop() || 'External media',
        });
      })
      .catch((error: unknown) => {
        pendingMediaKeysRef.current.delete(key);
        activeMediaKeyRef.current = null;
        setOpenPathErrors((current) => [
          ...current,
          `${path.split(/[\\/]/).pop() || path}: ${String(error)}`,
        ].slice(-10));
      })
      .finally(() => {
        openingDroppedMediaRef.current = false;
        // A failed open leaves the player closed, so retrigger the queue for
        // the next item. Successful opens remain paused until playback closes.
        setPendingMediaPaths((queued) => [...queued]);
      });
  }, [mediaImportPrompt, pendingMediaPaths, playerOpen]);

  const importPromptMedia = async () => {
    if (!mediaImportPrompt || isImportingMedia) return;
    setIsImportingMedia(true);
    try {
      await invoke('add_external_media', {
        path: mediaImportPrompt.path,
        title: mediaImportPrompt.title,
      });
      setMediaImportPrompt(null);
    } catch (error: unknown) {
      setMediaImportPrompt({ ...mediaImportPrompt, error: String(error) });
    } finally {
      setIsImportingMedia(false);
    }
  };

  useEffect(() => {
    if (settingsLoaded) {
      if (!settings.media_mode_decision_made) {
        setShowFirstRunMediaModal(true);
      } else {
        setShowFirstRunMediaModal(false);
      }
    }
  }, [settingsLoaded, settings.media_mode_decision_made]);

  // Keep modal reachable and recoverable when window is resized
  useEffect(() => {
    const handleResize = () => {
      setModalOffset((prev) => {
        const maxX = Math.max(0, (window.innerWidth - 460) / 2);
        const maxY = Math.max(0, (window.innerHeight - 300) / 2);
        return {
          x: Math.max(-maxX, Math.min(maxX, prev.x)),
          y: Math.max(-maxY, Math.min(maxY, prev.y)),
        };
      });
    };
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, []);

  const handleMediaChoice = async (enable: boolean) => {
    setSavingMediaMode(true);
    setMediaModeError(null);
    try {
      await saveSettings({
        media_mode_enabled: enable,
        media_mode_decision_made: true,
      });
      localStorage.setItem('nebulae_media_mode_choice', enable ? 'yes' : 'no');
      setViewMode(enable ? 'media' : 'manager');
      setShowFirstRunMediaModal(false);
    } catch (err: unknown) {
      console.error('[media-mode] failed to save choice:', err);
      setMediaModeError(
        typeof err === 'string'
          ? err
          : 'Failed to save media mode decision. Please try again.'
      );
    } finally {
      setSavingMediaMode(false);
    }
  };

  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    try {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch {}
    dragRef.current = {
      isDragging: true,
      startX: e.clientX,
      startY: e.clientY,
      initialOffsetX: modalOffset.x,
      initialOffsetY: modalOffset.y,
    };
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!dragRef.current.isDragging) return;
    const dx = e.clientX - dragRef.current.startX;
    const dy = e.clientY - dragRef.current.startY;
    const maxX = Math.max(0, (window.innerWidth - 460) / 2);
    const maxY = Math.max(0, (window.innerHeight - 300) / 2);
    const targetX = dragRef.current.initialOffsetX + dx;
    const targetY = dragRef.current.initialOffsetY + dy;
    setModalOffset({
      x: Math.max(-maxX, Math.min(maxX, targetX)),
      y: Math.max(-maxY, Math.min(maxY, targetY)),
    });
  };

  const handlePointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (dragRef.current.isDragging) {
      dragRef.current.isDragging = false;
      try {
        (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
      } catch {}
    }
  };

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
    void useLibraryStore.getState().loadPreferences();

    // Listen for live opened URIs / files from macOS LaunchServices or secondary process
    const unlistenFns: Array<() => void> = [];

    // Real-time 500ms push telemetry from Rust engine (sanitizes instantaneous
    // rates and preserves existing detail arrays like files and trackers)
    listen<import('./types').TorrentItem[]>('nebulae://telemetry', (event) => {
      if (event.payload) {
        useTorrentStore.getState().applyTorrentTelemetry(event.payload);
      }
    })
      .then((unlisten) => unlistenFns.push(unlisten))
      .catch(() => {});

    // Relaxed background heartbeat fallback
    const torrentPollInterval = setInterval(() => {
      fetchTorrents();
    }, 5000);

    listen<string>('nebulae://open-uri', (event) => {
      if (event.payload) {
        void dispatchOpenPaths([event.payload]);
      }
    })
      .then((unlisten) => {
        unlistenFns.push(unlisten);
        // Drain only after the live listener is ready, so a second-instance or
        // LaunchServices event cannot land in the gap before subscription.
        invoke<string[]>('get_pending_open_payloads')
          .then((payloads) => {
            if (payloads?.length) void dispatchOpenPaths(payloads);
          })
          .catch(() => {});
      })
      .catch(() => {});

    listen<{ request_token?: string | null; title?: string | null; reason?: string }>(
      'nebulae://adc-delivery-failed',
      (event) => {
        const reason = event.payload?.reason?.trim() || 'ADC website delivery failed.';
        setDeliveryFailures((current) =>
          addDeliveryFailure(current, dismissedDeliveryFailuresRef.current, {
            token: event.payload?.request_token,
            title: event.payload?.title,
            reason,
          }),
        );
      },
    )
      .then((unlisten) => unlistenFns.push(unlisten))
      .catch(() => {});

    listen('nebulae://adc-torrent-added', () => {
      void useTorrentStore.getState().fetchTorrents();
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

    // Native application menu actions (File, Edit, View, Window, Help)
    listen<string>('nebulae://menu-action', (event) => {
      const ui = useUiStore.getState();
      const torrents = useTorrentStore.getState();
      switch (event.payload) {
        case 'menu-about':
          window.dispatchEvent(new CustomEvent('nebulae:open-about'));
          break;
        case 'menu-settings':
          ui.setShowSettings(true);
          break;
        case 'menu-add-torrent':
        case 'menu-add-link':
          ui.setShowAddTorrent(true);
          break;
        case 'menu-create-torrent':
          ui.setShowCreateTorrent(true);
          break;
        case 'menu-resume-torrent':
          if (torrents.selectedId) torrents.resumeTorrent(torrents.selectedId);
          break;
        case 'menu-pause-torrent':
          if (torrents.selectedId) torrents.pauseTorrent(torrents.selectedId);
          break;
        case 'menu-delete-torrent': {
          const sel = torrents.torrents.find((t) => t.id === torrents.selectedId);
          if (sel) ui.setDeleteModalTarget({ id: sel.id, name: sel.name });
          break;
        }
        case 'menu-view-media':
          ui.setViewMode('media');
          break;
        case 'menu-view-manager':
          ui.setViewMode('manager');
          break;
        case 'menu-view-compact':
          ui.setViewMode(ui.viewMode === 'compact' ? 'manager' : 'compact');
          break;
        case 'menu-tools-doctor':
          ui.setShowConnectivityDoctor(true);
          break;
        case 'menu-tools-vpn':
          ui.setShowVpnManager(true);
          break;
        case 'menu-tools-tracker':
          ui.setShowTrackerDashboard(true);
          break;
        case 'menu-help-docs':
          ui.setShowHelp(true);
          break;
        case 'menu-help-discord':
          import('@tauri-apps/plugin-shell').then(({ open }) => {
            open('https://discord.gg/pcHMJTh8G2').catch((e) =>
              console.error('[menu] failed to open Discord invite:', e)
            );
          });
          break;
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

    const onOpenPaths = (event: Event) => {
      const paths = (event as CustomEvent<string[]>).detail;
      if (paths?.length) void dispatchOpenPaths(paths);
    };
    window.addEventListener('nebulae:open-paths', onOpenPaths);

    listen<{ paths?: string[]; position?: { x: number; y: number } }>('tauri://drag-drop', (event) => {
      setIsDragging(false);
      const paths = event.payload?.paths;
      if (paths && paths.length > 0) {
        void dispatchOpenPaths(paths);
      }
    })
      .then((unlisten) => unlistenFns.push(unlisten))
      .catch(() => {});

    listen<string[]>('nebulae://paths-dropped', (event) => {
      setIsDragging(false);
      if (event.payload?.length) {
        void dispatchOpenPaths(event.payload);
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
      window.removeEventListener('nebulae:open-paths', onOpenPaths);
      for (const un of unlistenFns) un();
    };
  }, [loadSettings, fetchEngineMode, fetchTorrents, runDoctorTest, fetchVpnStatus, fetchTrackerProfile, dispatchOpenPaths]);

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
    const files = Array.from(e.dataTransfer.files ?? []);
    const results = await Promise.all(files.map(async (file) => {
      const nativePath = (file as FileWithPath).path;
      if (nativePath) return { source: nativePath, failed: null };
      if (!/\.(torrent|adc)$/i.test(file.name)) {
        return { source: file.name, failed: null };
      }
      try {
        return { source: await readTorrentFileAsDataUrl(file), failed: null };
      } catch {
        return { source: null, failed: file.name };
      }
    }));
    const sources = results.flatMap((result) => (result.source ? [result.source] : []));
    const failedNames = results.flatMap((result) => (result.failed ? [result.failed] : []));
    void dispatchOpenPaths(sources);
    if (failedNames.length > 0) {
      setOpenPathErrors((current) => [
        ...current,
        ...failedNames.map((name) => `Could not read dropped torrent file: ${name}`),
      ].slice(-10));
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
        {viewMode === 'server' && <RemoteServerView />}
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
          <h3 className="text-lg font-bold text-white mb-1">Drop Files to Open or Add</h3>
          <p className="text-xs text-[#a3a3a3] font-mono">Media opens in the player; torrent files and magnet links enter the add queue.</p>
        </div>
      )}

      {/* 5. Modals */}
      <SettingsModal />
      <HelpModal />
      <BugReportModal />
      <PlayerOverlay />
      <AddTorrentModal />
      <CreateTorrentModal />
      <ConnectivityDoctorModal />
      <PortForwardModal />
      <VpnManagerModal />
      <TrackerModal />
      <DeleteTorrentModal />
      <QbitImportModal />

      {/* First-Run Media Mode Choice */}
      {splashDismissed && showFirstRunMediaModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm p-4 pointer-events-auto">
          <div
            className="bg-[#181818] border border-[#333333] rounded-xl p-6 max-w-md w-full shadow-2xl space-y-4"
            style={{
              transform: `translate3d(${modalOffset.x}px, ${modalOffset.y}px, 0)`,
            }}
          >
            {/* Draggable header area */}
            <div
              onPointerDown={handlePointerDown}
              onPointerMove={handlePointerMove}
              onPointerUp={handlePointerUp}
              onPointerCancel={handlePointerUp}
              className="cursor-grab active:cursor-grabbing select-none flex items-center justify-between pb-3 border-b border-[#262626] -mx-2 px-2"
              title="Drag to reposition prompt"
            >
              <div className="flex items-center gap-3">
                <div className="p-2 rounded-lg bg-[#e50914]/10 text-[#e50914] border border-[#e50914]/20 pointer-events-none">
                  <Film className="w-5 h-5" />
                </div>
                <h3 className="text-base font-bold text-white pointer-events-none">
                  Would you like to enable media mode?
                </h3>
              </div>
              <div className="text-[#555555] hover:text-[#888888] p-1">
                <GripHorizontal className="w-4 h-4" />
              </div>
            </div>

            <p className="text-xs text-[#cccccc] leading-relaxed select-text">
              Nebulae’s Media Center lets you play DVDs, Blu-ray, and UHD discs with interactive disc menus, while organizing and managing your media collection in one place.
            </p>

            {mediaModeError && (
              <div className="p-2.5 rounded bg-red-950/40 border border-red-800/60 text-red-300 text-xs">
                {mediaModeError}
              </div>
            )}

            <div className="flex items-center justify-end gap-3 pt-2">
              <button
                type="button"
                disabled={savingMediaMode}
                onClick={() => void handleMediaChoice(false)}
                className="px-4 py-2 rounded-lg bg-[#262626] hover:bg-[#333333] border border-[#333333] text-xs text-[#a3a3a3] hover:text-white font-medium transition disabled:opacity-50"
              >
                No
              </button>
              <button
                type="button"
                autoFocus
                disabled={savingMediaMode}
                onClick={() => void handleMediaChoice(true)}
                className="px-4 py-2 rounded-lg bg-[#e50914] hover:bg-[#f40612] text-xs text-white font-semibold shadow-sm transition ring-1 ring-white/20 disabled:opacity-50"
              >
                {savingMediaMode ? 'Saving…' : 'Yes'}
              </button>
            </div>
          </div>
        </div>
      )}

      {mediaImportPrompt && (
        <div className="fixed inset-0 z-[120] flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
          <div className="w-full max-w-md rounded-xl border border-[#463e31] bg-[#211f1c] p-5 shadow-2xl text-white">
            <h3 className="text-sm font-bold">Add this media to your library?</h3>
            <p className="mt-2 text-xs text-[#d4c4a8] break-words">{mediaImportPrompt.title}</p>
            <p className="mt-2 text-[11px] text-[#a3a3a3] leading-relaxed">
              Playback has started. Adding creates a local library entry only, it does not move or copy the media.
              {pendingMediaPaths.length > 0 && ` ${pendingMediaPaths.length} more item${pendingMediaPaths.length === 1 ? '' : 's'} will open after this player closes.`}
            </p>
            {mediaImportPrompt.error && (
              <p className="mt-3 text-xs text-red-300 break-words">{mediaImportPrompt.error}</p>
            )}
            <div className="mt-5 flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setMediaImportPrompt(null)}
                disabled={isImportingMedia}
                className="px-3 py-2 rounded-lg bg-[#303030] hover:bg-[#3b3b3b] text-xs text-white disabled:opacity-50"
              >
                Not now
              </button>
              <button
                type="button"
                onClick={() => void importPromptMedia()}
                disabled={isImportingMedia}
                className="px-3 py-2 rounded-lg bg-[#a68b5b] hover:bg-[#b99d6b] text-xs font-semibold text-black disabled:opacity-50"
              >
                {isImportingMedia ? 'Adding…' : 'Add to Library'}
              </button>
            </div>
          </div>
        </div>
      )}

      {deliveryFailures.length > 0 && (
        <div
          role="alertdialog"
          aria-labelledby="delivery-failure-title"
          className="fixed inset-0 z-[131] flex items-center justify-center bg-black/70 backdrop-blur-sm p-4"
        >
          <div className="w-full max-w-md rounded-xl border border-[#463e31] bg-[#211f1c] p-5 shadow-2xl text-white">
            <h3 id="delivery-failure-title" className="text-sm font-bold">
              Website download could not be started
            </h3>
            <ul className="mt-3 space-y-3 text-xs text-[#d4c4a8]">
              {deliveryFailures.map((failure) => (
                <li key={failure.key} className="break-words">
                  {failure.title && <div className="font-semibold text-white">{failure.title}</div>}
                  <div>{failure.reason}</div>
                  {failure.token && (
                    <button
                      type="button"
                      onClick={() => void retryDeliveryFailure(failure)}
                      className="mt-1.5 px-2.5 py-1 rounded-md border border-[#463e31] hover:bg-[#2c2924] text-[11px] font-semibold text-[#e8d9bb]"
                    >
                      Retry
                    </button>
                  )}
                </li>
              ))}
            </ul>
            <div className="mt-5 flex justify-end gap-2">
              {deliveryFailures.some((f) => classifyDeliveryFailure(f.reason) === 'remote') && (
                <button
                  type="button"
                  onClick={() => {
                    dismissDeliveryFailures();
                    useUiStore.getState().openSettings('rtorrent');
                  }}
                  className="px-3 py-2 rounded-lg border border-[#463e31] hover:bg-[#2c2924] text-xs font-semibold text-[#e8d9bb]"
                >
                  Open server settings
                </button>
              )}
              {deliveryFailures.some((f) => classifyDeliveryFailure(f.reason) === 'destination') && (
                <button
                  type="button"
                  onClick={() => {
                    dismissDeliveryFailures();
                    useUiStore.getState().openSettings('downloads');
                  }}
                  className="px-3 py-2 rounded-lg border border-[#463e31] hover:bg-[#2c2924] text-xs font-semibold text-[#e8d9bb]"
                >
                  Open download settings
                </button>
              )}
              <button
                type="button"
                onClick={dismissDeliveryFailures}
                className="px-3 py-2 rounded-lg bg-[#a68b5b] hover:bg-[#b99d6b] text-xs font-semibold text-black"
              >
                Dismiss
              </button>
            </div>
          </div>
        </div>
      )}

      {openPathErrors.length > 0 && (
        <div className="fixed inset-0 z-[130] flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
          <div className="w-full max-w-md rounded-xl border border-[#463e31] bg-[#211f1c] p-5 shadow-2xl text-white">
            <h3 className="text-sm font-bold">Some dropped items could not be opened</h3>
            <ul className="mt-3 space-y-1.5 text-xs text-[#d4c4a8]">
              {openPathErrors.map((item, index) => (
                <li key={`${item}-${index}`} className="break-words">{item}</li>
              ))}
            </ul>
            <div className="mt-5 flex justify-end">
              <button
                type="button"
                onClick={() => setOpenPathErrors([])}
                className="px-3 py-2 rounded-lg bg-[#a68b5b] hover:bg-[#b99d6b] text-xs font-semibold text-black"
              >
                Dismiss
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default App;
