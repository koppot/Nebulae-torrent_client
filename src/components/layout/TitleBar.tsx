import React from 'react';
import { useUiStore } from '../../stores/uiStore';
import { useTorrentStore } from '../../stores/torrentStore';
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
  Film,
  HardDrive,
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
  } = useUiStore();

  const { selectedId, torrents, resumeTorrent, pauseTorrent, removeTorrent } = useTorrentStore();

  const selectedTorrent = torrents.find((t) => t.id === selectedId);

  const handleResume = () => {
    if (selectedId) resumeTorrent(selectedId);
  };

  const handlePause = () => {
    if (selectedId) pauseTorrent(selectedId);
  };

  const handleRemove = () => {
    if (selectedId) removeTorrent(selectedId);
  };

  return (
    <header className="h-10 border-b border-[#262626] bg-[#141414] flex items-center justify-between px-3 select-none z-30">
      {/* Left: Brand Monogram & Standard Desktop Transfer Controls */}
      <div className="flex items-center gap-1.5">
        {/* Brand Monogram */}
        <div className="flex items-center gap-2 pr-2 border-r border-[#262626]">
          <img
            src="/favicon-boxed.svg"
            alt="Nebulae"
            className="w-5 h-5 rounded object-cover border border-[#333333]"
          />
          <span className="text-xs font-bold text-white tracking-tight">Nebulae</span>
        </div>

        {/* Primary Action Buttons: Add, Create, Resume, Pause, Delete */}
        <button
          onClick={() => setShowAddTorrent(true)}
          className="flex items-center gap-1 px-2.5 py-1 rounded bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold transition shadow-sm"
          title="Add Torrent or Magnet link (Cmd/Ctrl+O)"
        >
          <Plus className="w-3.5 h-3.5" />
          <span>Add</span>
        </button>

        <button
          onClick={() => setShowCreateTorrent(true)}
          className="flex items-center gap-1 px-2 py-1 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium transition"
          title="Create .torrent File (Torrent Creator)"
        >
          <Sparkles className="w-3.5 h-3.5 text-[#e50914]" />
          <span>Create</span>
        </button>

        <button
          onClick={handleResume}
          disabled={!selectedTorrent}
          className="flex items-center gap-1 px-2 py-1 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium disabled:opacity-30 disabled:cursor-not-allowed transition"
          title="Resume Selected Transfer"
        >
          <Play className="w-3 h-3 fill-current" />
          <span>Resume</span>
        </button>

        <button
          onClick={handlePause}
          disabled={!selectedTorrent}
          className="flex items-center gap-1 px-2 py-1 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium disabled:opacity-30 disabled:cursor-not-allowed transition"
          title="Pause Selected Transfer"
        >
          <Pause className="w-3 h-3 fill-current" />
          <span>Pause</span>
        </button>

        <button
          onClick={handleRemove}
          disabled={!selectedTorrent}
          className="flex items-center gap-1 px-2 py-1 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white hover:text-[#e50914] text-xs font-medium disabled:opacity-30 disabled:cursor-not-allowed transition"
          title="Delete Selected Transfer"
        >
          <Trash2 className="w-3 h-3" />
          <span>Delete</span>
        </button>

        {/* Divider */}
        <div className="h-4 w-[1px] bg-[#262626] mx-1" />

        {/* Utility Workbench Dialogs: Doctor, VPN, Tracker, Settings */}
        <button
          onClick={() => setShowConnectivityDoctor(true)}
          className="p-1.5 rounded hover:bg-[#1f1f1f] text-[#a3a3a3] hover:text-white transition"
          title="Connectivity Doctor & Port Status"
        >
          <Activity className="w-3.5 h-3.5" />
        </button>

        <button
          onClick={() => setShowVpnManager(true)}
          className="p-1.5 rounded hover:bg-[#1f1f1f] text-[#a3a3a3] hover:text-white transition"
          title="VPN & Fail-Closed Kill Switch"
        >
          <Shield className="w-3.5 h-3.5" />
        </button>

        <button
          onClick={() => setShowTrackerDashboard(true)}
          className="p-1.5 rounded hover:bg-[#1f1f1f] text-[#a3a3a3] hover:text-white transition"
          title="Asian DVD Club Tracker & Ratio Profile"
        >
          <Radio className="w-3.5 h-3.5" />
        </button>

        <button
          onClick={() => setShowSettings(true)}
          className="p-1.5 rounded hover:bg-[#1f1f1f] text-[#a3a3a3] hover:text-white transition"
          title="Preferences & Torrent Engine Settings"
        >
          <Settings className="w-3.5 h-3.5" />
        </button>

        <button
          onClick={() => setShowHelp(true)}
          className="p-1.5 rounded hover:bg-[#1f1f1f] text-[#a3a3a3] hover:text-white transition"
          title="Documentation & Keyboard Shortcuts"
        >
          <HelpCircle className="w-3.5 h-3.5" />
        </button>
      </div>

      {/* Right: Quick Search Filter & Compact View Switcher */}
      <div className="flex items-center gap-2">
        {/* Compact Quick Filter */}
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
              onClick={() => setSearchQuery('')}
              className="absolute right-1.5 top-1/2 -translate-y-1/2 text-[#a3a3a3] hover:text-white"
            >
              <X className="w-3 h-3" />
            </button>
          )}
        </div>

        {/* Divider */}
        <div className="h-4 w-[1px] bg-[#262626]" />

        {/* Compact View Switcher: Torrents vs Media */}
        <div className="flex items-center p-0.5 rounded bg-[#1f1f1f] border border-[#333333]">
          <button
            onClick={() => setViewMode('manager')}
            className={`flex items-center gap-1 px-2.5 py-0.5 rounded text-[11px] font-medium transition ${
              viewMode === 'manager'
                ? 'bg-[#e50914] text-white'
                : 'text-[#a3a3a3] hover:text-white'
            }`}
            title="Desktop Torrent Workbench"
          >
            <HardDrive className="w-3 h-3" />
            <span>Torrents</span>
          </button>
          <button
            onClick={() => setViewMode('media')}
            className={`flex items-center gap-1 px-2.5 py-0.5 rounded text-[11px] font-medium transition ${
              viewMode === 'media'
                ? 'bg-[#e50914] text-white'
                : 'text-[#a3a3a3] hover:text-white'
            }`}
            title="Media Cinema View"
          >
            <Film className="w-3 h-3" />
            <span>Media</span>
          </button>
        </div>
      </div>
    </header>
  );
};

export default TitleBar;
