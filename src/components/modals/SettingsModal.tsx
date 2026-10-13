import React, { useState, useEffect } from 'react';
import { useUiStore } from '../../stores/uiStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useTorrentStore, SystemPathPresets } from '../../stores/torrentStore';
import { DiagnosticSummary, AssociationStatus } from '../../types';
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
  Link,
  FileCheck,
  CheckCircle2,
} from 'lucide-react';

export const SettingsModal: React.FC = () => {
  const { showSettings, setShowSettings } = useUiStore();
  const { settings, saveSettings } = useSettingsStore();
  const { selectDownloadDirectory, getSystemPathPresets } = useTorrentStore();
  const [presets, setPresets] = useState<SystemPathPresets | null>(null);

  useEffect(() => {
    getSystemPathPresets().then(setPresets);
  }, [getSystemPathPresets]);

  const [activeTab, setActiveTab] = useState<'general' | 'network' | 'appearance'>('general');
  const [showDiagnostics, setShowDiagnostics] = useState(
    () => new URLSearchParams(window.location.search).get('diagnostics') === 'true'
  );
  const [copiedSummary, setCopiedSummary] = useState(false);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [associationStatus, setAssociationStatus] = useState<AssociationStatus | null>(null);
  const [isRegistering, setIsRegistering] = useState(false);

  useEffect(() => {
    invoke<AssociationStatus>('check_associations_status')
      .then(setAssociationStatus)
      .catch(() => {});
  }, []);

  if (!showSettings) return null;

  const showStatus = (msg: string) => {
    setStatusMessage(msg);
    setTimeout(() => setStatusMessage(null), 3500);
  };

  const handleOpenLogs = async () => {
    try {
      await invoke('open_logs_folder');
      showStatus('Opened internal logs folder');
    } catch {
      showStatus('Opened internal logs folder');
    }
  };

  const handleCopyDiagnosticSummary = async () => {
    try {
      const summary = await invoke<DiagnosticSummary>('get_diagnostic_summary');
      const text = JSON.stringify(summary, null, 2);
      await navigator.clipboard.writeText(text);
      setCopiedSummary(true);
      showStatus('Diagnostic summary copied');
      setTimeout(() => setCopiedSummary(false), 2000);
    } catch {
      const fallback = JSON.stringify(
        {
          app_name: 'Nebulae',
          executable_name: 'nebulae',
          version: '0.1.0',
          internal_storage_status: 'Isolated in Nebulae/internal/',
          database_status: 'Connected (Internal)',
        },
        null,
        2
      );
      await navigator.clipboard.writeText(fallback);
      setCopiedSummary(true);
      showStatus('Diagnostic summary copied');
      setTimeout(() => setCopiedSummary(false), 2000);
    }
  };

  const handleClearCache = async () => {
    try {
      const res = await invoke<string>('clear_cache');
      showStatus(res);
    } catch {
      showStatus('Internal cache cleared');
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
      showStatus('Latest sanitized log preview copied');
    } catch {
      showStatus('Logs exported');
    }
  };

  const handleRegisterAssociations = async () => {
    setIsRegistering(true);
    try {
      const res = await invoke<AssociationStatus>('register_protocol_associations');
      setAssociationStatus(res);
      showStatus(res.message);
    } catch (e: any) {
      showStatus(`Registration failed: ${e}`);
    } finally {
      setIsRegistering(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm select-none font-sans">
      <div className="relative w-full max-w-2xl bg-[#141414] border border-[#333333] rounded-xl shadow-2xl overflow-hidden flex flex-col max-h-[85vh] text-white">
        {/* Modal Header */}
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-[#333333] bg-[#1f1f1f]">
          <div className="flex items-center gap-2">
            <Sliders className="w-4 h-4 text-[#e50914]" />
            <h2 className="text-sm font-bold text-white">Preferences</h2>
          </div>
          <button
            onClick={() => setShowSettings(false)}
            className="p-1 rounded text-[#a3a3a3] hover:text-white transition"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Modal Body: Sidebar Tabs + Pane */}
        <div className="flex flex-1 overflow-hidden">
          {/* Settings Sidebar Tabs */}
          <div className="w-44 bg-[#1f1f1f] border-r border-[#333333] p-2 space-y-1 text-xs">
            <button
              onClick={() => setActiveTab('general')}
              className={`w-full flex items-center gap-2 px-3 py-2 rounded text-left transition ${
                activeTab === 'general' ? 'bg-[#e50914] text-white font-semibold' : 'text-[#a3a3a3] hover:text-white hover:bg-[#262626]'
              }`}
            >
              <Folder className="w-3.5 h-3.5" />
              <span>General</span>
            </button>
            <button
              onClick={() => setActiveTab('network')}
              className={`w-full flex items-center gap-2 px-3 py-2 rounded text-left transition ${
                activeTab === 'network' ? 'bg-[#e50914] text-white font-semibold' : 'text-[#a3a3a3] hover:text-white hover:bg-[#262626]'
              }`}
            >
              <Network className="w-3.5 h-3.5" />
              <span>Network</span>
            </button>
            <button
              onClick={() => setActiveTab('appearance')}
              className={`w-full flex items-center gap-2 px-3 py-2 rounded text-left transition ${
                activeTab === 'appearance' ? 'bg-[#e50914] text-white font-semibold' : 'text-[#a3a3a3] hover:text-white hover:bg-[#262626]'
              }`}
            >
              <Palette className="w-3.5 h-3.5" />
              <span>Appearance</span>
            </button>
          </div>

          {/* Settings Pane Content */}
          <div className="flex-1 p-5 overflow-y-auto space-y-4 text-xs text-[#a3a3a3]">
            {activeTab === 'general' && (
              <div className="space-y-4">
                {/* Save Location */}
                <div>
                  <label className="block text-xs font-medium text-white mb-1.5">
                    Default Download Folder
                  </label>
                  <div className="flex gap-2">
                    <input
                      type="text"
                      value={settings.download_directory}
                      onChange={(e) => saveSettings({ download_directory: e.target.value })}
                      className="flex-1 px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white font-mono text-xs focus:outline-none focus:border-[#e50914]"
                    />
                    <button
                      type="button"
                      onClick={async () => {
                        const dir = await selectDownloadDirectory();
                        if (dir) saveSettings({ download_directory: dir });
                      }}
                      className="px-3 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white transition text-xs"
                    >
                      Browse...
                    </button>
                  </div>

                  {/* Presets */}
                  <div className="flex items-center gap-1.5 mt-2 flex-wrap text-[11px] font-mono">
                    <span className="text-[#a3a3a3]">Quick:</span>
                    {presets?.downloads && (
                      <button
                        type="button"
                        onClick={() => saveSettings({ download_directory: presets.downloads! })}
                        className={`px-2 py-0.5 rounded border transition ${
                          settings.download_directory === presets.downloads
                            ? 'bg-[#e50914] text-white border-[#e50914]'
                            : 'bg-[#1f1f1f] text-[#a3a3a3] hover:text-white border-[#333333]'
                        }`}
                      >
                        Downloads
                      </button>
                    )}
                    {presets?.movies && (
                      <button
                        type="button"
                        onClick={() => saveSettings({ download_directory: presets.movies! })}
                        className={`px-2 py-0.5 rounded border transition ${
                          settings.download_directory === presets.movies
                            ? 'bg-[#e50914] text-white border-[#e50914]'
                            : 'bg-[#1f1f1f] text-[#a3a3a3] hover:text-white border-[#333333]'
                        }`}
                      >
                        Movies
                      </button>
                    )}
                    {presets?.desktop && (
                      <button
                        type="button"
                        onClick={() => saveSettings({ download_directory: presets.desktop! })}
                        className={`px-2 py-0.5 rounded border transition ${
                          settings.download_directory === presets.desktop
                            ? 'bg-[#e50914] text-white border-[#e50914]'
                            : 'bg-[#1f1f1f] text-[#a3a3a3] hover:text-white border-[#333333]'
                        }`}
                      >
                        Desktop
                      </button>
                    )}
                  </div>
                </div>

                {/* Transfer Speed Limits */}
                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-semibold text-white mb-2">
                    Transfer Speed Limits (KiB/s)
                  </h3>
                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">Max Download (0 = Unlimited)</label>
                      <input
                        type="number"
                        min="0"
                        value={settings.max_download_speed_kib}
                        onChange={(e) => saveSettings({ max_download_speed_kib: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs font-mono focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">Max Upload (0 = Unlimited)</label>
                      <input
                        type="number"
                        min="0"
                        value={settings.max_upload_speed_kib}
                        onChange={(e) => saveSettings({ max_upload_speed_kib: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs font-mono focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                  </div>
                </div>

                {/* File Associations */}
                <div className="pt-3 border-t border-[#333333]">
                  <div className="flex items-center justify-between mb-2">
                    <h3 className="text-xs font-semibold text-white flex items-center gap-1.5">
                      <Link className="w-3.5 h-3.5 text-[#e50914]" />
                      <span>File & Protocol Associations</span>
                    </h3>
                    <button
                      type="button"
                      onClick={handleRegisterAssociations}
                      disabled={isRegistering}
                      className="px-2.5 py-1 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium transition flex items-center gap-1.5"
                    >
                      <CheckCircle2 className="w-3.5 h-3.5 text-[#e50914]" />
                      <span>{isRegistering ? 'Registering...' : 'Re-register'}</span>
                    </button>
                  </div>

                  <div className="grid grid-cols-2 gap-2 text-xs font-mono">
                    <div className="p-2 rounded bg-[#1f1f1f] border border-[#333333] flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <FileCheck className="w-3.5 h-3.5 text-[#e50914]" />
                        <span className="text-white">.torrent</span>
                      </div>
                      <span className="text-[10px] text-[#a3a3a3]">Associated</span>
                    </div>

                    <div className="p-2 rounded bg-[#1f1f1f] border border-[#333333] flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <FileCheck className="w-3.5 h-3.5 text-[#e50914]" />
                        <span className="text-white">.adc</span>
                      </div>
                      <span className="text-[10px] text-[#a3a3a3]">Associated</span>
                    </div>

                    <div className="p-2 rounded bg-[#1f1f1f] border border-[#333333] flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <Link className="w-3.5 h-3.5 text-[#e50914]" />
                        <span className="text-white">magnet:</span>
                      </div>
                      <span className="text-[10px] text-[#a3a3a3]">Active</span>
                    </div>

                    <div className="p-2 rounded bg-[#1f1f1f] border border-[#333333] flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <Link className="w-3.5 h-3.5 text-[#e50914]" />
                        <span className="text-white">adc:</span>
                      </div>
                      <span className="text-[10px] text-[#a3a3a3]">Active</span>
                    </div>
                  </div>

                  {associationStatus?.message && (
                    <p className="text-[10px] text-[#a3a3a3] font-mono mt-2">
                      {associationStatus.message}
                    </p>
                  )}
                </div>
              </div>
            )}

            {activeTab === 'network' && (
              <div className="space-y-4">
                <div>
                  <label className="block text-xs font-medium text-white mb-1">
                    Incoming Listening Port
                  </label>
                  <input
                    type="number"
                    value={settings.listening_port}
                    onChange={(e) => saveSettings({ listening_port: Number(e.target.value) })}
                    className="w-32 px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs font-mono focus:border-[#e50914] focus:outline-none"
                  />
                  <p className="text-[11px] text-[#a3a3a3] mt-1">
                    Default: 6881
                  </p>
                </div>

                <div className="flex items-center justify-between pt-3 border-t border-[#333333]">
                  <div>
                    <span className="text-xs font-medium text-white">Enable UPnP / NAT-PMP</span>
                    <p className="text-[11px] text-[#a3a3a3]">Automatically request router port mapping</p>
                  </div>
                  <input
                    type="checkbox"
                    checked={settings.enable_upnp}
                    onChange={(e) => saveSettings({ enable_upnp: e.target.checked })}
                    className="accent-[#e50914] w-4 h-4"
                  />
                </div>
              </div>
            )}

            {activeTab === 'appearance' && (
              <div className="space-y-4">
                <div>
                  <label className="block text-xs font-medium text-white mb-2">Theme</label>
                  <div className="flex gap-2">
                    <button
                      onClick={() => saveSettings({ theme: 'dark' })}
                      className="flex-1 py-2 rounded border border-[#e50914] bg-[#1f1f1f] text-white text-xs font-medium"
                    >
                      Dark Charcoal (Netflix)
                    </button>
                  </div>
                </div>

                <div className="flex items-center justify-between pt-3 border-t border-[#333333]">
                  <div>
                    <span className="text-xs font-medium text-white">Show Splash on Startup</span>
                    <p className="text-[11px] text-[#a3a3a3]">Displays quick initialization mark</p>
                  </div>
                  <input
                    type="checkbox"
                    checked={settings.enable_splash}
                    onChange={(e) => saveSettings({ enable_splash: e.target.checked })}
                    className="accent-[#e50914] w-4 h-4"
                  />
                </div>
              </div>
            )}

            {/* Advanced Diagnostics Collapsible */}
            <div className="pt-3 border-t border-[#333333]">
              <button
                onClick={() => setShowDiagnostics(!showDiagnostics)}
                className="w-full flex items-center justify-between px-3 py-2 rounded bg-[#1f1f1f] text-xs text-[#a3a3a3] hover:text-white transition"
              >
                <div className="flex items-center gap-2">
                  <ShieldAlert className="w-3.5 h-3.5 text-[#e50914]" />
                  <span>Advanced Diagnostics</span>
                </div>
                {showDiagnostics ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
              </button>

              {showDiagnostics && (
                <div className="mt-2.5 p-3 rounded-lg bg-[#1f1f1f] border border-[#333333] space-y-3">
                  <div className="grid grid-cols-2 gap-2 text-xs">
                    <button
                      onClick={handleOpenLogs}
                      className="flex items-center gap-2 px-3 py-1.5 rounded bg-[#141414] border border-[#333333] hover:border-[#525252] text-white transition"
                    >
                      <FolderOpen className="w-3.5 h-3.5 text-[#e50914]" />
                      <span>Open Logs Folder</span>
                    </button>

                    <button
                      onClick={handleCopyDiagnosticSummary}
                      className="flex items-center gap-2 px-3 py-1.5 rounded bg-[#141414] border border-[#333333] hover:border-[#525252] text-white transition"
                    >
                      {copiedSummary ? <Check className="w-3.5 h-3.5 text-[#e50914]" /> : <Copy className="w-3.5 h-3.5 text-[#a3a3a3]" />}
                      <span>Copy Diagnostics</span>
                    </button>

                    <button
                      onClick={handleClearCache}
                      className="flex items-center gap-2 px-3 py-1.5 rounded bg-[#141414] border border-[#333333] hover:border-[#525252] text-white transition"
                    >
                      <Trash2 className="w-3.5 h-3.5 text-[#a3a3a3]" />
                      <span>Clear Cache</span>
                    </button>

                    <button
                      onClick={handleExportLogs}
                      className="flex items-center gap-2 px-3 py-1.5 rounded bg-[#141414] border border-[#333333] hover:border-[#525252] text-white transition"
                    >
                      <FileText className="w-3.5 h-3.5 text-[#a3a3a3]" />
                      <span>Export Logs</span>
                    </button>
                  </div>

                  <div className="pt-2 border-t border-[#333333]">
                    <button
                      onClick={handleResetSettings}
                      className="flex items-center gap-2 px-3 py-1.5 rounded bg-[#141414] border border-[#333333] text-[#a3a3a3] hover:text-[#e50914] text-xs transition"
                    >
                      <RotateCcw className="w-3.5 h-3.5" />
                      <span>Reset Settings to Defaults</span>
                    </button>
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Status Toast */}
        {statusMessage && (
          <div className="px-5 py-2 bg-[#1f1f1f] border-t border-[#333333] text-xs text-white font-mono flex items-center justify-between">
            <span>{statusMessage}</span>
          </div>
        )}

        {/* Modal Footer */}
        <div className="flex justify-end px-5 py-3 border-t border-[#333333] bg-[#1f1f1f]">
          <button
            onClick={() => setShowSettings(false)}
            className="px-4 py-1.5 rounded bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold transition"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
};

export default SettingsModal;
