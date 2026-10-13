import React from 'react';
import { useUiStore } from '../../stores/uiStore';
import { useVpnStore } from '../../stores/vpnStore';
import { useConnectivityStore } from '../../stores/connectivityStore';
import { useTrackerStore } from '../../stores/trackerStore';
import {
  Film,
  Table,
  Plus,
  Settings,
  HelpCircle,
  Sun,
  Moon,
  Activity,
  Shield,
  Award,
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
    setShowConnectivityDoctor,
    setShowVpnManager,
    setShowTrackerDashboard,
  } = useUiStore();

  const { status: vpnStatus } = useVpnStore();
  const { report: connReport } = useConnectivityStore();
  const { profile: trackerProfile } = useTrackerStore();

  return (
    <header className="h-14 border-b border-white/10 bg-black/70 backdrop-blur-xl flex items-center justify-between px-5 select-none z-30">
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

      {/* Right Controls: Connectivity Doctor, VPN, Tracker, Add, Settings, Help, Theme */}
      <div className="flex items-center gap-2">
        {/* Connectivity Doctor Quick-Button */}
        <button
          onClick={() => setShowConnectivityDoctor(true)}
          className="adc-btn-glass flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs text-white/90 hover:text-white hover:border-emerald-500/40 transition"
          title="Connectivity Doctor & Port Status"
        >
          <Activity className="w-3.5 h-3.5 text-emerald-400" />
          <span className="hidden sm:inline font-mono text-[11px]">
            {connReport?.port_status === 'Open' ? '6881 Open' : 'Doctor'}
          </span>
        </button>

        {/* VPN Quick-Button */}
        <button
          onClick={() => setShowVpnManager(true)}
          className={`adc-btn-glass flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs transition ${
            vpnStatus?.connected
              ? 'border-emerald-500/40 text-emerald-300'
              : 'text-white/80 hover:text-white'
          }`}
          title="ADC-Only VPN Management"
        >
          <Shield className="w-3.5 h-3.5 text-adc-crimson-light" />
          <span className="hidden sm:inline font-mono text-[11px]">
            {vpnStatus?.mode === 'AdcOnlyVpn'
              ? 'ADC-VPN'
              : vpnStatus?.mode === 'SystemVpn'
              ? 'Sys-VPN'
              : 'No VPN'}
          </span>
        </button>

        {/* AsianDVDClub Tracker Quick-Button */}
        <button
          onClick={() => setShowTrackerDashboard(true)}
          className="adc-btn-glass flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs text-white/90 hover:text-white hover:border-amber-500/40 transition"
          title="AsianDVDClub Tracker & Ratio Coach"
        >
          <Award className="w-3.5 h-3.5 text-amber-400" />
          <span className="hidden sm:inline font-mono text-[11px]">
            {trackerProfile?.is_linked ? `Ratio: ${trackerProfile.ratio.toFixed(2)}` : 'ADC Link'}
          </span>
        </button>

        <div className="h-4 w-[1px] bg-white/10 mx-0.5"></div>

        <button
          onClick={() => setShowAddTorrent(true)}
          className="adc-btn-crimson flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-xs font-semibold"
        >
          <Plus className="w-3.5 h-3.5" />
          <span className="hidden md:inline">Add Torrent</span>
        </button>

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
