import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useUiStore } from '../../stores/uiStore';
import { useTorrentStore } from '../../stores/torrentStore';
import { useSettingsStore } from '../../stores/settingsStore';
import {
  Plus,
  Play,
  Pause,
  Trash2,
  Settings,
  HelpCircle,
  Activity,
  Shield,
  Radio,
  Search,
  X,
  Sparkles,
  Minus,
  Square,
  Copy,
  LayoutGrid,
  Film,
  FolderOpen,
  Minimize2,
  Maximize2,
} from 'lucide-react';

export const TitleBar: React.FC = () => {
  const {
    viewMode,
    setViewMode,
    setShowSettings,
    setShowHelp,
    setShowAddTorrent,
    setShowCreateTorrent,
    setShowConnectivityDoctor,
    setShowVpnManager,
    setShowTrackerDashboard,
    searchQuery,
    setSearchQuery,
    setDeleteModalTarget,
  } = useUiStore();

  const { selectedId, torrents, resumeTorrent, pauseTorrent } = useTorrentStore();
  const selectedTorrent = torrents.find((t) => t.id === selectedId);

  const [isMaximized, setIsMaximized] = useState(false);
  const [platform, setPlatform] = useState<'windows' | 'mac' | 'linux'>('windows');

  useEffect(() => {
    const ua = navigator.userAgent.toLowerCase();
    if (ua.includes('mac')) {
      setPlatform('mac');
    } else if (ua.includes('linux')) {
      setPlatform('linux');
    } else {
      setPlatform('windows');
    }

    let unlistenResize: (() => void) | undefined;
    (async () => {
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        const appWindow = getCurrentWindow();
        setIsMaximized(await appWindow.isMaximized());
        unlistenResize = await appWindow.onResized(async () => {
          setIsMaximized(await appWindow.isMaximized());
        });
      } catch {
        // Fallback for non-tauri contexts
      }
    })();

    return () => {
      if (unlistenResize) unlistenResize();
    };
  }, []);

  const handleResume = () => {
    if (selectedId) resumeTorrent(selectedId);
  };

  const handlePause = () => {
    if (selectedId) pauseTorrent(selectedId);
  };

  const handleRemove = () => {
    if (selectedTorrent) {
      setDeleteModalTarget({ id: selectedTorrent.id, name: selectedTorrent.name });
    }
  };

  const { settings } = useSettingsStore();

  const handleMinimize = async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().minimize();
    } catch (e) {
      console.warn('Minimize not supported in web context', e);
    }
  };

  const handleToggleMaximize = async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const appWindow = getCurrentWindow();
      await appWindow.toggleMaximize();
      setIsMaximized(await appWindow.isMaximized());
    } catch (e) {
      console.warn('Toggle maximize not supported in web context', e);
    }
  };

  const handleClose = async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const win = getCurrentWindow();
      if (settings.close_to_tray) {
        // Owner directive: the X button minimizes to the taskbar/dock (the
        // engine keeps running); only explicit Quit ends the session.
        await win.minimize();
      } else {
        await win.close();
      }
    } catch (e) {
      console.warn('Close not supported in web context', e);
    }
  };

  return (
    <header
      data-tauri-drag-region
      onMouseDown={(e) => {
        // Tauri's startDragging (async IPC) fails to move borderless windows
        // on current macOS; the backend synthesizes a native mouse-down and
        // hands it to performWindowDragWithEvent on the main thread.
        if (e.button === 0 && (e.target as HTMLElement).closest('button') === null) {
          invoke('window_begin_drag').catch(() => {});
        }
      }}
      onDoubleClick={handleToggleMaximize}
      className="relative h-10 border-b border-[#262626] bg-[#141414] flex items-center justify-between px-3 select-none z-30 flex-shrink-0"
    >
      {/* Centered application name (owner: Movist Pro-style top bar),
          with the full client name on its own line BELOW the bar. */}
      <div className="absolute left-1/2 -translate-x-1/2 pointer-events-none">
        <span className="text-xs font-bold text-white tracking-tight">Nebulae</span>
        <div className="absolute left-1/2 -translate-x-1/2 top-full whitespace-nowrap text-[10px] text-[#a3a3a3] font-normal tracking-tight">
          Client by Asian DVD Club
        </div>
      </div>
      {/* Left: macOS Traffic Lights (if on Mac) + Unified Client Brand & Action Controls */}
      <div className="flex items-center gap-1.5 min-w-0" data-tauri-drag-region>
        {/* macOS Native-Style Window Traffic Lights */}
        {platform === 'mac' && (
          <div className="flex items-center pr-2.5 pl-0 flex-shrink-0">
            {/* 24px hit areas around 12px dots — the dots alone were too
                small to click reliably (owner: "buttons are not functional"). */}
            <button
              type="button"
              onClick={handleClose}
              className="w-6 h-6 flex items-center justify-center group transition"
              title="Minimize to Dock"
            >
              <span className="w-3 h-3 rounded-full bg-[#ff5f56] group-hover:brightness-90 flex items-center justify-center transition">
                <X className="w-2 h-2 text-black/60 opacity-0 group-hover:opacity-100" />
              </span>
            </button>
            <button
              type="button"
              onClick={handleMinimize}
              className="w-6 h-6 flex items-center justify-center group transition"
              title="Minimize"
            >
              <span className="w-3 h-3 rounded-full bg-[#ffbd2e] group-hover:brightness-90 flex items-center justify-center transition">
                <Minus className="w-2 h-2 text-black/60 opacity-0 group-hover:opacity-100" />
              </span>
            </button>
            <button
              type="button"
              onClick={handleToggleMaximize}
              className="w-6 h-6 flex items-center justify-center group transition"
              title="Zoom / Maximize"
            >
              <span className="w-3 h-3 rounded-full bg-[#27c93f] group-hover:brightness-90 flex items-center justify-center transition">
                <Square className="w-1.5 h-1.5 text-black/60 opacity-0 group-hover:opacity-100" />
              </span>
            </button>
          </div>
        )}


        {/* Primary Action Buttons: Add, Create, Resume, Pause, Delete —
            MANAGER ONLY (owner): none of the transfer actions belong to
            the Media Center. */}
        <div className={`flex items-center gap-1 flex-shrink-0 ${viewMode === 'media' ? 'hidden' : ''}`}>
          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); setShowAddTorrent(true); }}
            className="flex items-center gap-1 px-2.5 py-1 rounded bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold transition shadow-sm"
            title="Add Torrent or Magnet link (Cmd/Ctrl+O)"
          >
            <Plus className="w-3.5 h-3.5" />
            <span>Add</span>
          </button>

          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); setShowCreateTorrent(true); }}
            className="flex items-center gap-1 px-2 py-1 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium transition"
            title="Create .torrent File (Torrent Creator)"
          >
            <Sparkles className="w-3.5 h-3.5 text-[#e50914]" />
            <span>Create</span>
          </button>

          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); handleResume(); }}
            disabled={!selectedTorrent}
            className="flex items-center gap-1 px-2 py-1 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium disabled:opacity-30 disabled:cursor-not-allowed transition"
            title="Resume Selected Transfer"
          >
            <Play className="w-3 h-3 fill-current" />
            <span>Resume</span>
          </button>

          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); handlePause(); }}
            disabled={!selectedTorrent}
            className="flex items-center gap-1 px-2 py-1 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium disabled:opacity-30 disabled:cursor-not-allowed transition"
            title="Pause Selected Transfer"
          >
            <Pause className="w-3 h-3 fill-current" />
            <span>Pause</span>
          </button>

          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); handleRemove(); }}
            disabled={!selectedTorrent}
            className="flex items-center gap-1 px-2 py-1 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white hover:text-[#e50914] text-xs font-medium disabled:opacity-30 disabled:cursor-not-allowed transition"
            title="Delete Selected Transfer"
          >
            <Trash2 className="w-3 h-3" />
            <span>Delete</span>
          </button>

          {/* Brand between the action and utility icons (owner). */}
          <div className="flex items-baseline gap-1.5 mx-2 flex-shrink-0" data-tauri-drag-region>
            <span className="text-xs font-bold text-white tracking-tight">Nebulae</span>
            <span className="text-[11px] text-[#a3a3a3] font-normal tracking-tight hidden lg:inline">
              — Client by Asian DVD Club
            </span>
          </div>

          {/* Divider */}
          <div className="h-4 w-[1px] bg-[#262626] mx-1" />

          {/* Utility Workbench Dialogs: Doctor, VPN, Tracker, Settings, Help */}
          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); setShowConnectivityDoctor(true); }}
            className="p-1.5 rounded hover:bg-[#1f1f1f] text-[#a3a3a3] hover:text-white transition"
            title="Connectivity Doctor & Port Status"
          >
            <Activity className="w-3.5 h-3.5" />
          </button>

          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); setShowVpnManager(true); }}
            className="p-1.5 rounded hover:bg-[#1f1f1f] text-[#a3a3a3] hover:text-white transition"
            title="VPN & Fail-Closed Kill Switch"
          >
            <Shield className="w-3.5 h-3.5" />
          </button>

          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); setShowTrackerDashboard(true); }}
            className="p-1.5 rounded hover:bg-[#1f1f1f] text-[#a3a3a3] hover:text-white transition"
            title="Asian DVD Club Tracker & Ratio Profile"
          >
            <Radio className="w-3.5 h-3.5" />
          </button>

          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); setShowSettings(true); }}
            className="p-1.5 rounded hover:bg-[#1f1f1f] text-[#a3a3a3] hover:text-white transition"
            title="Preferences & Torrent Engine Settings"
          >
            <Settings className="w-3.5 h-3.5" />
          </button>

          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); setShowHelp(true); }}
            className="p-1.5 rounded hover:bg-[#1f1f1f] text-[#a3a3a3] hover:text-white transition"
            title="Documentation & Keyboard Shortcuts"
          >
            <HelpCircle className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* Middle: Expansive Draggable Region */}
      <div
        data-tauri-drag-region
        onDoubleClick={handleToggleMaximize}
        className="flex-1 h-full min-w-4 cursor-default"
      />

      {/* Right: Mode Switcher, Quick Search Filter & Desktop Window Controls */}
      <div className="flex items-center gap-2 flex-shrink-0" data-tauri-drag-region>
        {/* Media-mode controls (owner): Search library, Player preferences
            and Add Movie live in the TOP BAR; SimpleMediaView receives
            them via window events. */}
        {viewMode === 'media' && (
          <>
            <div
              className="flex items-center gap-2 px-2.5 py-1 rounded-lg bg-[#1f1f1f] border border-[#333333] w-52 focus-within:border-[#e50914]/50 transition"
              title="Search the library by release or folder name"
            >
              <Search className="w-3.5 h-3.5 text-[#737373]" />
              <input
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="Search library..."
                className="bg-transparent text-xs outline-none flex-1 text-white placeholder:text-[#737373] min-w-0"
              />
              {searchQuery && (
                <button
                  onClick={(e) => { e.stopPropagation(); setSearchQuery(''); }}
                  className="text-[#737373] hover:text-white transition"
                  title="Clear search"
                >
                  <X className="w-3 h-3" />
                </button>
              )}
            </div>
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                window.dispatchEvent(new CustomEvent('nebulae:player-prefs'));
              }}
              className="p-1.5 rounded hover:bg-[#1f1f1f] text-[#a3a3a3] hover:text-white transition"
              title="Player preferences — preferred audio and subtitle languages"
            >
              <Settings className="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                window.dispatchEvent(new CustomEvent('nebulae:add-movie'));
              }}
              className="p-1.5 rounded hover:bg-[#1f1f1f] text-[#a3a3a3] hover:text-white transition"
              title="Add Movie — pick the release root folder (all discs) or an ISO image"
            >
              <FolderOpen className="w-4 h-4" />
            </button>
          </>
        )}
        {/* UI Mode Switcher (Media / Manager / Compact) */}
        <div className="flex items-center p-0.5 rounded-lg bg-[#1f1f1f] border border-[#333333] text-xs">
          <button
            type="button"
            onClick={() => setViewMode('media')}
            className={`flex items-center gap-1.5 px-2 py-0.5 rounded text-[11px] font-medium transition ${
              viewMode === 'media'
                ? 'bg-[#e50914] text-white font-semibold shadow-xs'
                : 'text-[#a3a3a3] hover:text-white'
            }`}
            title="Simple Media Mode (Asian Cinema Gallery)"
          >
            <Film className="w-3 h-3" />
            <span className="hidden xl:inline">Media</span>
          </button>

          <button
            type="button"
            onClick={() => setViewMode('manager')}
            className={`flex items-center gap-1.5 px-2 py-0.5 rounded text-[11px] font-medium transition ${
              viewMode === 'manager'
                ? 'bg-[#e50914] text-white font-semibold shadow-xs'
                : 'text-[#a3a3a3] hover:text-white'
            }`}
            title="Torrent Manager Mode (Detailed Swarm Grid)"
          >
            <LayoutGrid className="w-3 h-3" />
            <span className="hidden xl:inline">Manager</span>
          </button>

          <button
            type="button"
            onClick={() => setViewMode(viewMode === 'compact' ? 'manager' : 'compact')}
            className={`flex items-center gap-1.5 px-2 py-0.5 rounded text-[11px] font-medium transition ${
              viewMode === 'compact'
                ? 'bg-[#e50914] text-white font-semibold shadow-xs'
                : 'text-[#a3a3a3] hover:text-white'
            }`}
            title={
              viewMode === 'compact'
                ? 'Expand — Restore Full Manager View'
                : 'Compact Mini-Bar Mode'
            }
          >
            {viewMode === 'compact' ? (
              <Maximize2 className="w-3 h-3" />
            ) : (
              <Minimize2 className="w-3 h-3" />
            )}
            <span className="hidden xl:inline">
              {viewMode === 'compact' ? 'Expand' : 'Compact'}
            </span>
          </button>
        </div>

        {/* Compact Quick Filter — transfers only (manager/compact); the
            media mode uses the Search library box instead. */}
        {viewMode !== 'media' && (
        <div className="relative w-44">
          <Search className="w-3 h-3 absolute left-2 top-1/2 -translate-y-1/2 text-[#a3a3a3]" />
          <input
            type="text"
            placeholder="Search transfers..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full pl-6 pr-6 py-0.5 bg-[#1f1f1f] border border-[#333333] rounded text-[11px] text-white placeholder:text-[#a3a3a3] focus:outline-none focus:border-[#e50914] transition"
          />
          {searchQuery && (
            <button
              type="button"
              onClick={() => setSearchQuery('')}
              className="absolute right-1.5 top-1/2 -translate-y-1/2 text-[#a3a3a3] hover:text-white"
            >
              <X className="w-3 h-3" />
            </button>
          )}
        </div>
        )}

        {/* Windows / Linux Desktop Window Control Buttons */}
        {platform !== 'mac' && (
          <div className="flex items-center ml-1 border-l border-[#262626] pl-1 h-10 -mr-3">
            {/* Minimize */}
            <button
              type="button"
              onClick={handleMinimize}
              className="h-10 w-11 flex items-center justify-center text-[#a3a3a3] hover:text-white hover:bg-[#222222] transition"
              title="Minimize"
            >
              <Minus className="w-3.5 h-3.5" />
            </button>

            {/* Maximize / Restore */}
            <button
              type="button"
              onClick={handleToggleMaximize}
              className="h-10 w-11 flex items-center justify-center text-[#a3a3a3] hover:text-white hover:bg-[#222222] transition"
              title={isMaximized ? 'Restore' : 'Maximize'}
            >
              {isMaximized ? (
                <Copy className="w-3 h-3 rotate-180" />
              ) : (
                <Square className="w-3 h-3" />
              )}
            </button>

            {/* Close */}
            <button
              type="button"
              onClick={handleClose}
              className="h-10 w-11 flex items-center justify-center text-[#a3a3a3] hover:text-white hover:bg-[#e81123] transition"
              title="Close"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        )}
      </div>
    </header>
  );
};

export default TitleBar;
