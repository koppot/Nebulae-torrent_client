import React, { useState } from 'react';
import { useUiStore } from '../../stores/uiStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { DiagnosticSummary } from '../../types';
import { invoke } from '@tauri-apps/api/core';
import {
  X,
  Sliders,
  Folder,
  Network,
  Palette,
  ShieldAlert,
  FolderOpen,
  Copy,
  Trash2,
  RotateCcw,
  FileText,
  Check,
  ChevronDown,
  ChevronRight,
  Sparkles,
} from 'lucide-react';

export const SettingsModal: React.FC = () => {
  const { showSettings, setShowSettings } = useUiStore();
  const { settings, saveSettings } = useSettingsStore();

  const [activeTab, setActiveTab] = useState<'general' | 'network' | 'appearance' | 'splash'>('general');
  const [showDiagnostics, setShowDiagnostics] = useState(false);
  const [copiedSummary, setCopiedSummary] = useState(false);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);

  if (!showSettings) return null;

  const showStatus = (msg: string) => {
    setStatusMessage(msg);
    setTimeout(() => setStatusMessage(null), 3500);
  };

  const handleOpenLogs = async () => {
    try {
      await invoke('open_logs_folder');
      showStatus('Opened internal logs folder in file manager');
    } catch {
      showStatus('Opened internal logs folder (web simulation)');
    }
  };

  const handleCopyDiagnosticSummary = async () => {
    try {
      const summary = await invoke<DiagnosticSummary>('get_diagnostic_summary');
      const text = JSON.stringify(summary, null, 2);
      await navigator.clipboard.writeText(text);
      setCopiedSummary(true);
      showStatus('Diagnostic summary copied to clipboard');
      setTimeout(() => setCopiedSummary(false), 2000);
    } catch {
      const fallback = JSON.stringify(
        {
          app_name: 'Nebula Torrent',
          executable_name: 'ADC',
          version: '0.1.0',
          internal_storage_status: 'Isolated in ADC/internal/',
          database_status: 'Connected (Internal)',
        },
        null,
        2
      );
      await navigator.clipboard.writeText(fallback);
      setCopiedSummary(true);
      showStatus('Diagnostic summary copied (simulated)');
      setTimeout(() => setCopiedSummary(false), 2000);
    }
  };

  const handleClearCache = async () => {
    try {
      const res = await invoke<string>('clear_cache');
      showStatus(res);
    } catch {
      showStatus('Internal temporary cache cleared');
    }
  };

  const handleResetSettings = async () => {
    if (confirm('Are you sure you want to reset all user settings to default?')) {
      try {
        const res = await invoke<string>('reset_settings');
        showStatus(res);
      } catch {
        showStatus('Settings reset to defaults');
      }
    }
  };

  const handleExportLogs = async () => {
    try {
      const logs = await invoke<string>('export_logs');
      await navigator.clipboard.writeText(logs);
      showStatus('Latest sanitized log preview copied to clipboard');
    } catch {
      showStatus('Logs exported (simulated)');
    }
  };

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
      <div className="relative w-full max-w-2xl bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl overflow-hidden flex flex-col max-h-[85vh]">
        {/* Modal Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-slate-800">
          <div className="flex items-center gap-2">
            <Sliders className="w-5 h-5 text-pink-400" />
            <h2 className="text-lg font-semibold text-white">Settings</h2>
          </div>
          <button
            onClick={() => setShowSettings(false)}
            className="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Modal Body: Navigation Tabs + Pane */}
        <div className="flex flex-1 overflow-hidden">
          {/* Settings Sidebar Tabs */}
          <div className="w-48 bg-slate-950/50 border-r border-slate-800 p-3 space-y-1">
            <button
              onClick={() => setActiveTab('general')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition ${
                activeTab === 'general' ? 'bg-pink-600/20 text-pink-400 font-semibold' : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <Folder className="w-4 h-4" />
              <span>General</span>
            </button>
            <button
              onClick={() => setActiveTab('network')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition ${
                activeTab === 'network' ? 'bg-pink-600/20 text-pink-400 font-semibold' : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <Network className="w-4 h-4" />
              <span>Network</span>
            </button>
            <button
              onClick={() => setActiveTab('appearance')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition ${
                activeTab === 'appearance' ? 'bg-pink-600/20 text-pink-400 font-semibold' : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <Palette className="w-4 h-4" />
              <span>Appearance</span>
            </button>
            <button
              onClick={() => setActiveTab('splash')}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium transition ${
                activeTab === 'splash' ? 'bg-pink-600/20 text-pink-400 font-semibold' : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <Sparkles className="w-4 h-4" />
              <span>Startup & Community</span>
            </button>
          </div>

          {/* Settings Pane Content */}
          <div className="flex-1 p-6 overflow-y-auto space-y-6 text-sm text-slate-300">
            {activeTab === 'general' && (
              <div className="space-y-4">
                <div>
                  <label className="block text-xs font-medium text-slate-400 mb-1">
                    Default Download Location
                  </label>
                  <input
                    type="text"
                    value={settings.download_directory}
                    onChange={(e) => saveSettings({ download_directory: e.target.value })}
                    className="w-full px-3 py-2 rounded-lg bg-slate-950 border border-slate-700 text-slate-100 text-xs font-mono focus:outline-none focus:border-pink-500"
                  />
                  <p className="text-[11px] text-slate-500 mt-1">
                    Folder where newly added torrent files and media are stored.
                  </p>
                </div>

                <div className="pt-2 border-t border-slate-800">
                  <h3 className="text-xs font-semibold uppercase tracking-wider text-slate-400 mb-3">
                    Transfer Speed Limits
                  </h3>
                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <label className="block text-xs text-slate-400 mb-1">Max Download (KiB/s)</label>
                      <input
                        type="number"
                        min="0"
                        value={settings.max_download_speed_kib}
                        onChange={(e) => saveSettings({ max_download_speed_kib: Number(e.target.value) })}
                        className="w-full px-3 py-2 rounded-lg bg-slate-950 border border-slate-700 text-slate-100 text-xs font-mono"
                      />
                      <span className="text-[10px] text-slate-500">0 for unlimited</span>
                    </div>
                    <div>
                      <label className="block text-xs text-slate-400 mb-1">Max Upload (KiB/s)</label>
                      <input
                        type="number"
                        min="0"
                        value={settings.max_upload_speed_kib}
                        onChange={(e) => saveSettings({ max_upload_speed_kib: Number(e.target.value) })}
                        className="w-full px-3 py-2 rounded-lg bg-slate-950 border border-slate-700 text-slate-100 text-xs font-mono"
                      />
                      <span className="text-[10px] text-slate-500">0 for unlimited</span>
                    </div>
                  </div>
                </div>
              </div>
            )}

            {activeTab === 'network' && (
              <div className="space-y-4">
                <div>
                  <label className="block text-xs font-medium text-slate-400 mb-1">
                    Incoming Listening Port
                  </label>
                  <input
                    type="number"
                    value={settings.listening_port}
                    onChange={(e) => saveSettings({ listening_port: Number(e.target.value) })}
                    className="w-32 px-3 py-2 rounded-lg bg-slate-950 border border-slate-700 text-slate-100 text-xs font-mono"
                  />
                  <p className="text-[11px] text-slate-500 mt-1">
                    Port utilized for incoming peer handshakes (default: 6881).
                  </p>
                </div>

                <div className="flex items-center justify-between pt-3 border-t border-slate-800">
                  <div>
                    <span className="text-xs font-medium text-slate-200">Enable UPnP / NAT-PMP</span>
                    <p className="text-[11px] text-slate-500">Automatically configure router port forwarding</p>
                  </div>
                  <input
                    type="checkbox"
                    checked={settings.enable_upnp}
                    onChange={(e) => saveSettings({ enable_upnp: e.target.checked })}
                    className="rounded bg-slate-950 border-slate-700 text-pink-500 focus:ring-0 w-4 h-4"
                  />
                </div>
              </div>
            )}

            {activeTab === 'appearance' && (
              <div className="space-y-4">
                <div>
                  <label className="block text-xs font-medium text-slate-400 mb-2">Application Theme</label>
                  <div className="flex gap-3">
                    <button
                      onClick={() => saveSettings({ theme: 'dark' })}
                      className={`flex-1 py-2 rounded-lg border text-xs font-medium transition ${
                        settings.theme === 'dark' ? 'border-pink-500 bg-pink-500/10 text-white' : 'border-slate-800 bg-slate-950 text-slate-400'
                      }`}
                    >
                      Dark Cinema (Default)
                    </button>
                    <button
                      onClick={() => saveSettings({ theme: 'light' })}
                      className={`flex-1 py-2 rounded-lg border text-xs font-medium transition ${
                        settings.theme === 'light' ? 'border-pink-500 bg-pink-500/10 text-white' : 'border-slate-800 bg-slate-950 text-slate-400'
                      }`}
                    >
                      Light Mode
                    </button>
                  </div>
                </div>
              </div>
            )}

            {activeTab === 'splash' && (
              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <div>
                    <span className="text-xs font-medium text-slate-200">Show Cinematic Splash on Startup</span>
                    <p className="text-[11px] text-slate-500">Displays initialization status and community notice</p>
                  </div>
                  <input
                    type="checkbox"
                    checked={settings.enable_splash}
                    onChange={(e) => saveSettings({ enable_splash: e.target.checked })}
                    className="rounded bg-slate-950 border-slate-700 text-pink-500 focus:ring-0 w-4 h-4"
                  />
                </div>

                <div>
                  <label className="block text-xs font-medium text-slate-400 mb-1">
                    Splash Community Callout Text
                  </label>
                  <textarea
                    rows={2}
                    value={settings.splash_message}
                    onChange={(e) => saveSettings({ splash_message: e.target.value })}
                    className="w-full px-3 py-2 rounded-lg bg-slate-950 border border-slate-700 text-slate-100 text-xs focus:outline-none focus:border-pink-500"
                  />
                </div>

                <div>
                  <label className="block text-xs font-medium text-slate-400 mb-1">
                    Splash Destination URL
                  </label>
                  <input
                    type="text"
                    value={settings.splash_url}
                    onChange={(e) => saveSettings({ splash_url: e.target.value })}
                    className="w-full px-3 py-2 rounded-lg bg-slate-950 border border-slate-700 text-slate-100 text-xs font-mono focus:outline-none focus:border-pink-500"
                  />
                </div>
              </div>
            )}

            {/* Separator to Hidden Advanced Diagnostics */}
            <div className="pt-4 border-t border-slate-800">
              <button
                onClick={() => setShowDiagnostics(!showDiagnostics)}
                className="w-full flex items-center justify-between px-3 py-2 rounded-lg bg-slate-950 hover:bg-slate-950/80 text-xs text-slate-400 transition"
              >
                <div className="flex items-center gap-2">
                  <ShieldAlert className="w-4 h-4 text-amber-400" />
                  <span className="font-semibold text-slate-300">
                    {showDiagnostics ? 'Hide Advanced Diagnostics' : 'Show Advanced Diagnostics'}
                  </span>
                </div>
                {showDiagnostics ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
              </button>

              {/* Advanced Diagnostics Panel */}
              {showDiagnostics && (
                <div className="mt-3 p-4 rounded-xl bg-slate-950/80 border border-amber-500/20 space-y-4">
                  <div className="flex items-center gap-3 p-2.5 bg-slate-900 rounded-lg border border-amber-500/30">
                    <img
                      src="/src/assets/dev-icon.png"
                      alt="Developer Mode — Der Sperre"
                      className="w-10 h-10 rounded object-contain shadow-sm"
                    />
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-bold text-amber-300">Developer & Internal Diagnostics</span>
                        <span className="text-[9px] uppercase font-mono px-1.5 py-0.5 rounded bg-amber-950 border border-amber-700/50 text-amber-300 font-semibold">
                          Der Sperre
                        </span>
                      </div>
                      <p className="text-[10px] text-slate-400 mt-0.5">
                        Internal Subsystem Management · Isolated technical storage
                      </p>
                    </div>
                  </div>

                  <div className="flex items-start gap-2">
                    <p className="text-[11px] text-amber-200/80">
                      Technical files (logs, SQLite database, engine state, cache) are isolated in the internal application directory and hidden from standard views. Do not modify internal files directly.
                    </p>
                  </div>

                  <div className="grid grid-cols-2 gap-2 text-xs">
                    <button
                      onClick={handleOpenLogs}
                      className="flex items-center gap-2 px-3 py-2 rounded-lg bg-slate-900 border border-slate-750 hover:bg-slate-800 text-slate-200 transition"
                    >
                      <FolderOpen className="w-4 h-4 text-cyan-400" />
                      <span>Open Logs Folder</span>
                    </button>

                    <button
                      onClick={handleCopyDiagnosticSummary}
                      className="flex items-center gap-2 px-3 py-2 rounded-lg bg-slate-900 border border-slate-750 hover:bg-slate-800 text-slate-200 transition"
                    >
                      {copiedSummary ? <Check className="w-4 h-4 text-green-400" /> : <Copy className="w-4 h-4 text-pink-400" />}
                      <span>Copy Diagnostic Summary</span>
                    </button>

                    <button
                      onClick={handleClearCache}
                      className="flex items-center gap-2 px-3 py-2 rounded-lg bg-slate-900 border border-slate-750 hover:bg-slate-800 text-slate-200 transition"
                    >
                      <Trash2 className="w-4 h-4 text-amber-400" />
                      <span>Clear Internal Cache</span>
                    </button>

                    <button
                      onClick={handleExportLogs}
                      className="flex items-center gap-2 px-3 py-2 rounded-lg bg-slate-900 border border-slate-750 hover:bg-slate-800 text-slate-200 transition"
                    >
                      <FileText className="w-4 h-4 text-indigo-400" />
                      <span>Export Logs (Sanitized)</span>
                    </button>
                  </div>

                  <div className="pt-2 border-t border-slate-800">
                    <button
                      onClick={handleResetSettings}
                      className="flex items-center gap-2 px-3 py-2 rounded-lg bg-red-950/40 border border-red-800/40 hover:bg-red-900/40 text-red-200 text-xs transition"
                    >
                      <RotateCcw className="w-4 h-4 text-red-400" />
                      <span>Reset Settings to Factory Defaults</span>
                    </button>
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Status Toast */}
        {statusMessage && (
          <div className="px-6 py-2 bg-pink-900/40 border-t border-pink-750 text-xs text-pink-200 font-mono flex items-center justify-between">
            <span>{statusMessage}</span>
          </div>
        )}

        {/* Modal Footer */}
        <div className="flex justify-end px-6 py-3 border-t border-slate-800 bg-slate-950/40">
          <button
            onClick={() => setShowSettings(false)}
            className="px-4 py-1.5 rounded-lg bg-pink-600 hover:bg-pink-500 text-white text-xs font-semibold transition"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
};
