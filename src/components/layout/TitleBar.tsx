import React from 'react';
import { useUiStore } from '../../stores/uiStore';
import {
  Film,
  HardDrive,
  Plus,
  Settings,
  HelpCircle,
  Activity,
  Shield,
  Radio,
} from 'lucide-react';

export const TitleBar: React.FC = () => {
  const {
    viewMode,
    setViewMode,
    setShowSettings,
    setShowHelp,
    setShowAddTorrent,
    setShowConnectivityDoctor,
    setShowVpnManager,
    setShowTrackerDashboard,
  } = useUiStore();

  return (
    <header className="h-14 border-b border-[#333333] bg-[#141414] flex items-center justify-between px-5 select-none z-30">
      {/* Brand Monogram & App Title */}
      <div className="flex items-center gap-3">
        <img
          src="/app-icon.png"
          alt="Nebulae"
          className="w-8 h-8 rounded-lg object-cover border border-[#333333]"
        />
        <div>
          <div className="flex items-center gap-2 leading-none">
            <span className="text-sm font-bold text-white tracking-tight">Nebulae</span>
            <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-[#e50914] text-white font-semibold">
              v0.1.0
            </span>
          </div>
          <p className="text-[10px] text-[#a3a3a3] font-mono leading-none mt-1">
            Torrent Client by Asian DVD Club
          </p>
        </div>
      </div>

      {/* Center View Switcher: Media vs Torrents */}
      <div className="flex items-center p-1 rounded-lg bg-[#1f1f1f] border border-[#333333]">
        <button
          onClick={() => setViewMode('media')}
          className={`flex items-center gap-2 px-4 py-1.5 rounded-md text-xs font-semibold transition ${
            viewMode === 'media'
              ? 'bg-[#e50914] text-white shadow'
              : 'text-[#a3a3a3] hover:text-white'
          }`}
        >
          <Film className="w-3.5 h-3.5" />
          <span>Media</span>
        </button>
        <button
          onClick={() => setViewMode('manager')}
          className={`flex items-center gap-2 px-4 py-1.5 rounded-md text-xs font-semibold transition ${
            viewMode === 'manager'
              ? 'bg-[#e50914] text-white shadow'
              : 'text-[#a3a3a3] hover:text-white'
          }`}
        >
          <HardDrive className="w-3.5 h-3.5" />
          <span>Torrents</span>
        </button>
      </div>

      {/* Right Controls: Action Icons */}
      <div className="flex items-center gap-2">
        <button
          onClick={() => setShowAddTorrent(true)}
          className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-md bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold transition shadow"
          title="Add Torrent"
        >
          <Plus className="w-4 h-4" />
          <span>Add</span>
        </button>

        <div className="h-4 w-[1px] bg-[#333333] mx-1"></div>

        <button
          onClick={() => setShowConnectivityDoctor(true)}
          className="p-2 rounded-md bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-[#a3a3a3] hover:text-white transition"
          title="Connectivity & Port Status"
        >
          <Activity className="w-4 h-4" />
        </button>

        <button
          onClick={() => setShowVpnManager(true)}
          className="p-2 rounded-md bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-[#a3a3a3] hover:text-white transition"
          title="VPN & Security Settings"
        >
          <Shield className="w-4 h-4" />
        </button>

        <button
          onClick={() => setShowTrackerDashboard(true)}
          className="p-2 rounded-md bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-[#a3a3a3] hover:text-white transition"
          title="Tracker & Ratio Dashboard"
        >
          <Radio className="w-4 h-4" />
        </button>

        <button
          onClick={() => setShowHelp(true)}
          className="p-2 rounded-md bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-[#a3a3a3] hover:text-white transition"
          title="Help & Documentation"
        >
          <HelpCircle className="w-4 h-4" />
        </button>

        <button
          onClick={() => setShowSettings(true)}
          className="p-2 rounded-md bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-[#a3a3a3] hover:text-white transition"
          title="Preferences"
        >
          <Settings className="w-4 h-4" />
        </button>
      </div>
    </header>
  );
};

export default TitleBar;
