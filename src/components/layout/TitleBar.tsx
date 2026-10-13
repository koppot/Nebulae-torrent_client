import React from 'react';
import { useUiStore } from '../../stores/uiStore';
import {
  Film,
  Table,
  Plus,
  Settings,
  HelpCircle,
  Sun,
  Moon,
} from 'lucide-react';

export const TitleBar: React.FC = () => {
  const {
    viewMode,
    setViewMode,
    theme,
    toggleTheme,
    setShowSettings,
    setShowHelp,
    setShowAddTorrent,
  } = useUiStore();

  return (
    <header className="h-14 border-b border-white/10 bg-black/60 backdrop-blur-xl flex items-center justify-between px-5 select-none z-30">
      {/* Brand Monogram & App Title */}
      <div className="flex items-center gap-3">
        <img
          src="/src/assets/app-icon.png"
          alt="ADC Icon"
          className="w-8 h-8 rounded-lg shadow-adc-glass object-cover border border-white/15"
        />
        <div>
          <div className="flex items-center gap-1.5 leading-none">
            <span className="text-sm font-semibold text-adc-ivory tracking-tight">Nebula</span>
            <span className="text-[10px] uppercase font-mono px-1.5 py-0.5 rounded bg-adc-crimson/40 border border-adc-crimsonBright/50 text-adc-ivory font-semibold">
              v0.1.0
            </span>
          </div>
          <p className="text-[10px] text-adc-ivory/70 font-mono leading-none mt-1">
            Torrent Client by Asian DVD Club
          </p>
        </div>
      </div>

      {/* Center UI View Mode Switcher (Floating glass pill style) */}
      <div className="flex items-center p-1 rounded-xl bg-white/5 border border-white/10 backdrop-blur-md">
        <button
          onClick={() => setViewMode('media')}
          className={`flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-xs font-semibold transition ${
            viewMode === 'media'
              ? 'adc-btn-crimson'
              : 'text-white/70 hover:text-white hover:bg-white/5'
          }`}
        >
          <Film className="w-3.5 h-3.5" />
          <span>Simple Media</span>
        </button>
        <button
          onClick={() => setViewMode('manager')}
          className={`flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-xs font-semibold transition ${
            viewMode === 'manager'
              ? 'adc-btn-crimson'
              : 'text-white/70 hover:text-white hover:bg-white/5'
          }`}
        >
          <Table className="w-3.5 h-3.5" />
          <span>Torrent Manager</span>
        </button>
      </div>

      {/* Right Controls: Add, Settings, Help, Theme */}
      <div className="flex items-center gap-2">
        <button
          onClick={() => setShowAddTorrent(true)}
          className="adc-btn-crimson flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-xs font-semibold"
        >
          <Plus className="w-3.5 h-3.5" />
          <span>Add Torrent</span>
        </button>

        <div className="h-4 w-[1px] bg-white/10 mx-1"></div>

        <button
          onClick={() => setShowHelp(true)}
          className="adc-btn-glass p-2 rounded-lg text-white/80 hover:text-white"
          title="Installation & User Guide"
        >
          <HelpCircle className="w-4 h-4" />
        </button>

        <button
          onClick={() => setShowSettings(true)}
          className="adc-btn-glass p-2 rounded-lg text-white/80 hover:text-white"
          title="Settings & Diagnostics"
        >
          <Settings className="w-4 h-4" />
        </button>

        <button
          onClick={toggleTheme}
          className="adc-btn-glass p-2 rounded-lg text-white/80 hover:text-adc-gold"
          title="Toggle Theme"
        >
          {theme === 'dark' ? <Sun className="w-4 h-4" /> : <Moon className="w-4 h-4" />}
        </button>
      </div>
    </header>
  );
};
