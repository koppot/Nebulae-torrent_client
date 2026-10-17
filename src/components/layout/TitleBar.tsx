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
  const [, setPlatform] = useState<'windows' | 'mac' | 'linux'>('windows');

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
      className="relative border-b border-[#262626] bg-[#141414] flex flex-col select-none z-30 flex-shrink-0"
    >
      <div className="flex items-center justify-between px-3 h-10" data-tauri-drag-region>
        <div className="flex items-baseline gap-1.5 flex-shrink-0" data-tauri-drag-region>
          <span className="text-xs font-bold text-white tracking-tight">Nebulae</span>
          <span className="text-[11px] text-[#a3a3a3] font-normal tracking-tight hidden lg:inline">
            — Client by Asian DVD Club
          </span>
        </div>
        <div className="flex items-center gap-2 flex-shrink-0" data-tauri-drag-region>
        {/* Single Mode Toggle (owner): the button shows the mode it
            switches TO. Manager -> "Media", Media -> "Manager". */}
        <button
          type="button"
          onClick={() => setViewMode(viewMode === 'media' ? 'manager' : 'media')}
          className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-[#1f1f1f] border border-[#333333] text-[11px] font-semibold text-[#a3a3a3] hover:text-white hover:border-[#e50914]/50 transition"
          title={viewMode === 'media' ? 'Switch to the Torrent Manager' : 'Switch to the Media Center'}
        >
          {viewMode === 'media' ? (
            <LayoutGrid className="w-3 h-3" />
          ) : (
            <Film className="w-3 h-3" />
          )}
          <span>{viewMode === 'media' ? 'Manager' : 'Media'}</span>
        </button>

          <div className="flex items-center ml-1 border-l border-[#262626] pl-1 h-10 -mr-3">
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

            {/* Compact mini-bar (owner): icon-only, left of the close X. */}
            <button
              type="button"
              onClick={() => setViewMode(viewMode === 'compact' ? 'manager' : 'compact')}
              className="h-10 w-11 flex items-center justify-center text-[#a3a3a3] hover:text-white hover:bg-[#333333] transition"
              title={viewMode === 'compact' ? 'Expand — Restore Full Manager View' : 'Compact Mini-Bar Mode'}
            >
              {viewMode === 'compact' ? (
                <Maximize2 className="w-3.5 h-3.5" />
              ) : (
                <Minimize2 className="w-3.5 h-3.5" />
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
            </button>          </div>
        </div>
      </div>
      <div className="flex items-center justify-between gap-2 px-3 h-10" data-tauri-drag-region>
        {/* Left slot (owner): the torrent controls float left on row 2. */}
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
        <div className="flex items-center gap-2 flex-shrink-0">
        {viewMode === 'media' && (
          <>
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
          </>
        )}
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

        </div>

        {/* Minimize (owner): second row, right end. */}
        <div className="flex items-center ml-1 border-l border-[#262626] pl-1 h-10">
            {/* Minimize */}
            <button
              type="button"
              onClick={handleMinimize}
              className="h-10 w-11 flex items-center justify-center text-[#a3a3a3] hover:text-white hover:bg-[#222222] transition"
              title="Minimize"
            >
              <Minus className="w-3.5 h-3.5" />
            </button>

        </div>
      </div>
    </header>
  );
};

export default TitleBar;
