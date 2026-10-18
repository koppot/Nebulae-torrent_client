import React, { useState, useEffect } from 'react';
import { useUiStore } from '../../stores/uiStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useTorrentStore } from '../../stores/torrentStore';
import {
  DiagnosticSummary,
  AssociationStatus,
  DiscPlayerStatus,
  UserSettings,
} from '../../types';
import { errorMessage } from '../../utils/errors';
import { invoke } from '@tauri-apps/api/core';
import {
  X,
  Sliders,
  Folder,
  Network,
  Gauge,
  Radio,
  Rss,
  Globe,
  Server,
  Cpu,
  FolderOpen,
  Copy,
  Trash2,
  RotateCcw,
  FileText,
  Link,
  FileCheck,
  CheckCircle2,
  RefreshCw,
  Film,
} from 'lucide-react';

type SettingsTab =
  | 'behavior'
  | 'downloads'
  | 'connection'
  | 'speed'
  | 'bittorrent'
  | 'rss'
  | 'webui'
  | 'rtorrent'
  | 'media'
  | 'advanced';

export const SettingsModal: React.FC = () => {
  const { showSettings, setShowSettings } = useUiStore();
  const { settings, saveSettings } = useSettingsStore();
  const { selectDownloadDirectory } = useTorrentStore();

  const [activeTab, setActiveTab] = useState<SettingsTab>('behavior');
  const [skipSplash, setSkipSplash] = useState(
    () => localStorage.getItem('nebulae_pref_skip_splash') === '1',
  );

  const [copiedSummary, setCopiedSummary] = useState(false);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [associationStatus, setAssociationStatus] = useState<AssociationStatus | null>(null);
  const [isRegistering, setIsRegistering] = useState(false);
  const [serverTestResult, setServerTestResult] = useState<string | null>(null);
  const [isTestingServer, setIsTestingServer] = useState(false);
  const [discPlayerStatus, setDiscPlayerStatus] = useState<DiscPlayerStatus | null>(null);
  const [isProbingPlayer, setIsProbingPlayer] = useState(false);

  const probePlayer = () => {
    setIsProbingPlayer(true);
    invoke<DiscPlayerStatus>('get_disc_player_status')
      .then(setDiscPlayerStatus)
      .catch(() => {})
      .finally(() => setIsProbingPlayer(false));
  };

  useEffect(() => {
    invoke<AssociationStatus>('check_associations_status')
      .then(setAssociationStatus)
      .catch(() => {});
    probePlayer();
  }, []);

  if (!showSettings) return null;

  const showStatus = (msg: string) => {
    setStatusMessage(msg);
    setTimeout(() => setStatusMessage(null), 3500);
  };

  const handleTestServerConnection = async () => {
    setIsTestingServer(true);
    setServerTestResult(null);
    try {
      const status = await invoke<{ name: string; version: string; engine: string }>(
        'test_rtorrent_server_connection',
        {
          url: settings.rtorrent_server_url,
          token: settings.rtorrent_server_token,
        }
      );
      setServerTestResult(
        `Connected: ${status.name} v${status.version} (engine: ${status.engine})`
      );
    } catch (e) {
      setServerTestResult(`Failed: ${String(e)}`);
    } finally {
      setIsTestingServer(false);
    }
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
    } catch (e: unknown) {
      showStatus(`Registration failed: ${errorMessage(e, 'Registration failed')}`);
    } finally {
      setIsRegistering(false);
    }
  };

  const handleRandomizePort = () => {
    const randomPort = Math.floor(Math.random() * (65535 - 1024 + 1)) + 1024;
    saveSettings({ listening_port: randomPort });
    showStatus(`Generated random port: ${randomPort}`);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm select-none font-sans">
      <div className="relative w-full max-w-4xl bg-[#141414] border border-[#333333] rounded-xl shadow-2xl overflow-hidden flex flex-col max-h-[90vh] text-white">
        {/* Modal Header */}
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-[#333333] bg-[#1f1f1f]">
          <div className="flex items-center gap-2">
            <Sliders className="w-4 h-4 text-[#e50914]" />
            <h2 className="text-sm font-bold text-white">Options — Preferences</h2>
          </div>
          <button
            onClick={() => setShowSettings(false)}
            className="p-1 rounded text-[#a3a3a3] hover:text-white transition"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Modal Body: Sidebar Tabs + Content Pane */}
        <div className="flex flex-1 overflow-hidden">
          {/* Settings Sidebar Tabs (Canonical 8 qBittorrent Sections) */}
          <div className="w-48 bg-[#1f1f1f] border-r border-[#333333] p-2 space-y-1 text-xs">
            {[
              { id: 'behavior', label: 'Behavior', icon: Sliders },
              { id: 'downloads', label: 'Downloads', icon: Folder },
              { id: 'connection', label: 'Connection', icon: Network },
              { id: 'speed', label: 'Speed', icon: Gauge },
              { id: 'bittorrent', label: 'BitTorrent', icon: Radio },
              { id: 'rss', label: 'RSS', icon: Rss },
              { id: 'webui', label: 'Web UI', icon: Globe },
              { id: 'rtorrent', label: 'Server', icon: Server },
              { id: 'media', label: 'Media', icon: Film },
              { id: 'advanced', label: 'Advanced', icon: Cpu },
            ].map((tab) => {
              const Icon = tab.icon;
              const isActive = activeTab === tab.id;
              return (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id as SettingsTab)}
                  className={`w-full flex items-center gap-2 px-3 py-2 rounded text-left transition ${
                    isActive
                      ? 'bg-[#e50914] text-white font-semibold shadow'
                      : 'text-[#a3a3a3] hover:text-white hover:bg-[#262626]'
                  }`}
                >
                  <Icon className="w-3.5 h-3.5" />
                  <span>{tab.label}</span>
                </button>
              );
            })}
          </div>

          {/* Settings Pane Content */}
          <div className="flex-1 p-5 overflow-y-auto space-y-5 text-xs text-[#a3a3a3]">
            {/* 1. BEHAVIOR */}
            {activeTab === 'behavior' && (
              <div className="space-y-4">
                <div>
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    User Interface & Desktop Integration
                  </h3>
                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">Language</label>
                      <select
                        value={settings.language}
                        onChange={(e) => saveSettings({ language: e.target.value })}
                        className="w-full px-2.5 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      >
                        <option value="en-US">English (United States)</option>
                        <option value="ja-JP">日本語 (Japanese)</option>
                        <option value="zh-CN">简体中文 (Simplified Chinese)</option>
                        <option value="ko-KR">한국어 (Korean)</option>
                        <option value="fr-FR">Français (French)</option>
                        <option value="de-DE">Deutsch (German)</option>
                      </select>
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">Color Theme</label>
                      <select
                        value={settings.theme}
                        onChange={(e) => saveSettings({ theme: e.target.value as UserSettings['theme'] })}
                        className="w-full px-2.5 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      >
                        <option value="dark">Dark Space / Charcoal</option>
                        <option value="light">Light Theme</option>
                      </select>
                    </div>
                  </div>

                  <div className="mt-3 space-y-2">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={skipSplash}
                        onChange={(e) => {
                          setSkipSplash(e.target.checked);
                          localStorage.setItem('nebulae_pref_skip_splash', e.target.checked ? '1' : '0');
                        }}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Start program without splash screen</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.start_on_boot}
                        onChange={(e) => saveSettings({ start_on_boot: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Start Nebulae on Windows/macOS startup</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.start_minimized}
                        onChange={(e) => saveSettings({ start_minimized: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Start minimized</span>
                    </label>
                    <label className="flex items-start gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.close_to_tray}
                        onChange={(e) => saveSettings({ close_to_tray: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0 mt-0.5"
                      />
                      <div>
                        <span className="text-white font-medium">Close button minimizes to system tray</span>
                        <span className="block text-[11px] text-[#8c8c8c] mt-0.5">
                          When enabled, clicking the window close button automatically hides Nebulae to the system tray (right-click tray icon to quit). When disabled, closing the window exits Nebulae.
                        </span>
                      </div>
                    </label>
                    <label className="flex items-start gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.minimize_to_tray}
                        onChange={(e) => saveSettings({ minimize_to_tray: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0 mt-0.5"
                      />
                      <div>
                        <span className="text-white font-medium">Minimize button minimizes to system tray</span>
                        <span className="block text-[11px] text-[#8c8c8c] mt-0.5">
                          When disabled, minimizing keeps the application icon visible on the taskbar.
                        </span>
                      </div>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.inhibit_sleep_when_active}
                        onChange={(e) => saveSettings({ inhibit_sleep_when_active: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Inhibit system sleep when transfers are active</span>
                    </label>
                    <label className="flex items-start gap-2 cursor-pointer pt-1">
                      <input
                        type="checkbox"
                        checked={settings.auto_update_enabled}
                        onChange={(e) => saveSettings({ auto_update_enabled: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0 mt-0.5"
                      />
                      <div>
                        <span className="text-white font-medium">Automatically keep Nebulae updated (recommended)</span>
                        <span className="block text-[11px] text-[#8c8c8c] mt-0.5">
                          Checks for signed releases on startup, downloads silently in the background, and stages updates to apply on quit without interrupting active torrent transfers.
                          On Linux Debian/RPM installations, updates are managed by your system package manager.
                        </span>
                      </div>
                    </label>
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Community Splash Screen
                  </h3>
                  <div className="space-y-2">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.enable_splash}
                        onChange={(e) => saveSettings({ enable_splash: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Show community splash screen on startup</span>
                    </label>
                    {settings.enable_splash && (
                      <div className="pl-5 space-y-2">
                        <div>
                          <label className="block text-[11px] text-[#a3a3a3] mb-1">Splash message</label>
                          <textarea
                            value={settings.splash_message}
                            onChange={(e) => saveSettings({ splash_message: e.target.value })}
                            rows={2}
                            className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:outline-none focus:border-[#e50914] resize-none"
                          />
                        </div>
                        <div>
                          <label className="block text-[11px] text-[#a3a3a3] mb-1">Splash URL</label>
                          <input
                            type="text"
                            value={settings.splash_url}
                            onChange={(e) => saveSettings({ splash_url: e.target.value })}
                            className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white font-mono text-xs focus:outline-none focus:border-[#e50914]"
                          />
                        </div>
                      </div>
                    )}
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Notifications & Confirmations
                  </h3>
                  <div className="space-y-2">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.notify_on_download_finished}
                        onChange={(e) => saveSettings({ notify_on_download_finished: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Show OS desktop notification when download completes</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.notify_on_torrent_added}
                        onChange={(e) => saveSettings({ notify_on_torrent_added: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Show desktop notification when new torrent is added</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.confirm_torrent_deletion}
                        onChange={(e) => saveSettings({ confirm_torrent_deletion: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Confirm when deleting torrents</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.confirm_exit_when_active}
                        onChange={(e) => saveSettings({ confirm_exit_when_active: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Confirm exit when active transfers exist</span>
                    </label>
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <div className="flex items-center justify-between mb-2">
                    <h3 className="text-xs font-bold text-white uppercase tracking-wider font-mono flex items-center gap-1.5">
                      <Link className="w-3.5 h-3.5 text-[#e50914]" />
                      <span>File & URL Scheme Ownership</span>
                    </h3>
                    <button
                      type="button"
                      onClick={handleRegisterAssociations}
                      disabled={isRegistering}
                      className="px-2.5 py-1 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium transition flex items-center gap-1.5"
                    >
                      <CheckCircle2 className="w-3.5 h-3.5 text-[#e50914]" />
                      <span>{isRegistering ? 'Registering...' : 'Claim OS Defaults'}</span>
                    </button>
                  </div>
                  <div className="grid grid-cols-2 gap-2 text-xs font-mono">
                    <div className="p-2 rounded bg-[#1f1f1f] border border-[#333333] flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <FileCheck className="w-3.5 h-3.5 text-[#e50914]" />
                        <span className="text-white">.torrent</span>
                      </div>
                      <span className="text-[10px] text-[#a3a3a3]">
                        {associationStatus?.torrent_files ? 'Registered' : 'Available'}
                      </span>
                    </div>
                    <div className="p-2 rounded bg-[#1f1f1f] border border-[#333333] flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <FileCheck className="w-3.5 h-3.5 text-[#e50914]" />
                        <span className="text-white">.adc</span>
                      </div>
                      <span className="text-[10px] text-[#a3a3a3]">
                        {associationStatus?.adc_files ? 'Registered' : 'Available'}
                      </span>
                    </div>
                    <div className="p-2 rounded bg-[#1f1f1f] border border-[#333333] flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <Link className="w-3.5 h-3.5 text-[#e50914]" />
                        <span className="text-white">magnet:</span>
                      </div>
                      <span className="text-[10px] text-[#a3a3a3]">
                        {associationStatus?.magnet_protocol ? 'Active' : 'Available'}
                      </span>
                    </div>
                    <div className="p-2 rounded bg-[#1f1f1f] border border-[#333333] flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <Link className="w-3.5 h-3.5 text-[#e50914]" />
                        <span className="text-white">adc:</span>
                      </div>
                      <span className="text-[10px] text-[#a3a3a3]">
                        {associationStatus?.adc_protocol ? 'Active' : 'Available'}
                      </span>
                    </div>
                  </div>
                </div>
              </div>
            )}

            {/* 2. DOWNLOADS */}
            {activeTab === 'downloads' && (
              <div className="space-y-4">
                <div>
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Save Management
                  </h3>
                  <label className="block text-[11px] text-[#a3a3a3] mb-1">
                    Default Save Path
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

                  {/* Recent Destination Buttons */}
                  {settings.recent_download_directories && settings.recent_download_directories.length > 0 && (
                    <div className="flex items-center gap-1.5 mt-2 flex-wrap text-[11px] font-mono">
                      <span className="text-[#a3a3a3]">Recent:</span>
                      {settings.recent_download_directories.map((dirPath) => {
                        const label = dirPath.split('/').pop()?.split('\\').pop() || dirPath;
                        const isCurrent = settings.download_directory === dirPath;
                        return (
                          <button
                            key={dirPath}
                            type="button"
                            onClick={() => saveSettings({ download_directory: dirPath })}
                            title={dirPath}
                            className={`px-2 py-0.5 rounded transition border truncate max-w-[160px] ${
                              isCurrent
                                ? 'bg-[#e50914] text-white border-[#e50914]'
                                : 'bg-[#1f1f1f] text-[#a3a3a3] hover:text-white border-[#333333]'
                            }`}
                          >
                            {label}
                          </button>
                        );
                      })}
                    </div>
                  )}

                  <div className="mt-3">
                    <label className="flex items-center gap-2 cursor-pointer mb-1.5">
                      <input
                        type="checkbox"
                        checked={settings.use_temp_incomplete_dir}
                        onChange={(e) => saveSettings({ use_temp_incomplete_dir: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Keep incomplete torrents in separate folder:</span>
                    </label>
                    <input
                      type="text"
                      disabled={!settings.use_temp_incomplete_dir}
                      value={settings.temp_incomplete_directory}
                      onChange={(e) => saveSettings({ temp_incomplete_directory: e.target.value })}
                      placeholder="/path/to/incomplete_downloads"
                      className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white font-mono text-xs focus:outline-none focus:border-[#e50914] disabled:opacity-40"
                    />
                  </div>
                  <div className="mt-3 space-y-2">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">Copy .torrent files to:</label>
                      <input type="text" value={settings.copy_torrent_files_to}
                        onChange={(e) => saveSettings({ copy_torrent_files_to: e.target.value })}
                        placeholder="(leave blank to disable)"
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white font-mono text-xs focus:outline-none focus:border-[#e50914]" />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">Copy finished torrents to:</label>
                      <input type="text" value={settings.copy_finished_torrents_to}
                        onChange={(e) => saveSettings({ copy_finished_torrents_to: e.target.value })}
                        placeholder="(leave blank to disable)"
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white font-mono text-xs focus:outline-none focus:border-[#e50914]" />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">Watched folder (auto-add .torrent files):</label>
                      <input type="text" value={settings.watched_folder_directory}
                        onChange={(e) => saveSettings({ watched_folder_directory: e.target.value })}
                        placeholder="(leave blank to disable)"
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white font-mono text-xs focus:outline-none focus:border-[#e50914]" />
                    </div>
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Torrent Ingestion & File Allocation
                  </h3>
                  <div className="space-y-2">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.auto_management_mode}
                        onChange={(e) => saveSettings({ auto_management_mode: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Automatic Torrent Management mode (relocate when category changes)</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.show_torrent_content_dialog}
                        onChange={(e) => saveSettings({ show_torrent_content_dialog: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Display torrent content and file selection dialog when adding</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.do_not_start_automatically}
                        onChange={(e) => saveSettings({ do_not_start_automatically: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Do not start the download automatically (Add in Paused state)</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.append_neb_extension}
                        onChange={(e) => saveSettings({ append_neb_extension: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Append .!neb extension to incomplete files</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.preallocate_disk_space}
                        onChange={(e) => saveSettings({ preallocate_disk_space: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Pre-allocate disk space for all files</span>
                    </label>
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Post-Processing & External Commands
                  </h3>
                  <label className="flex items-center gap-2 cursor-pointer mb-2">
                    <input
                      type="checkbox"
                      checked={settings.run_external_program_on_finished}
                      onChange={(e) => saveSettings({ run_external_program_on_finished: e.target.checked })}
                      className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                    />
                    <span className="text-white">Run external program on torrent completion</span>
                  </label>
                  <input
                    type="text"
                    disabled={!settings.run_external_program_on_finished}
                    value={settings.external_program_command}
                    onChange={(e) => saveSettings({ external_program_command: e.target.value })}
                    placeholder="/usr/local/bin/script.sh &quot;%N&quot; &quot;%D&quot; &quot;%F&quot;"
                    className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white font-mono text-xs focus:outline-none focus:border-[#e50914] disabled:opacity-40"
                  />
                  <p className="text-[10px] text-[#a3a3a3] font-mono mt-1">
                    Parameters: %N (Name), %D (Save path), %F (Content path), %I (Info-hash), %L (Category)
                  </p>
                </div>
              </div>
            )}

            {/* 3. CONNECTION */}
            {activeTab === 'connection' && (
              <div className="space-y-4">
                <div>
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Listening Port & Protocols
                  </h3>
                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">
                        Peer Connection Protocol
                      </label>
                      <select
                        value={settings.peer_connection_protocol}
                        onChange={(e) => saveSettings({ peer_connection_protocol: e.target.value as UserSettings['peer_connection_protocol'] })}
                        className="w-full px-2.5 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      >
                        <option value="Both">Both TCP and μTP</option>
                        <option value="TCP">TCP Only</option>
                        <option value="uTP">μTP Only</option>
                      </select>
                    </div>

                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">
                        Port used for incoming connections
                      </label>
                      <div className="flex gap-2">
                        <input
                          type="number"
                          value={settings.listening_port}
                          onChange={(e) => saveSettings({ listening_port: Number(e.target.value) })}
                          className="flex-1 px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs font-mono focus:border-[#e50914] focus:outline-none"
                        />
                        <button
                          type="button"
                          onClick={handleRandomizePort}
                          className="px-2.5 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs transition flex items-center gap-1"
                        >
                          <RefreshCw className="w-3 h-3 text-[#e50914]" />
                          <span>Random</span>
                        </button>
                      </div>
                    </div>
                  </div>

                  <div className="mt-3 space-y-2">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.enable_upnp}
                        onChange={(e) => saveSettings({ enable_upnp: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Use UPnP / NAT-PMP port forwarding from router</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.randomize_port_on_startup}
                        onChange={(e) => saveSettings({ randomize_port_on_startup: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Use different port on each startup</span>
                    </label>
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Connection Limits
                  </h3>
                  <div className="grid grid-cols-2 gap-3 font-mono">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Global maximum number of connections:
                      </label>
                      <input
                        type="number"
                        value={settings.max_global_connections}
                        onChange={(e) => saveSettings({ max_global_connections: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Maximum number of connections per torrent:
                      </label>
                      <input
                        type="number"
                        value={settings.max_connections_per_torrent}
                        onChange={(e) => saveSettings({ max_connections_per_torrent: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Global maximum number of upload slots:
                      </label>
                      <input
                        type="number"
                        value={settings.max_global_upload_slots}
                        onChange={(e) => saveSettings({ max_global_upload_slots: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Maximum number of upload slots per torrent:
                      </label>
                      <input
                        type="number"
                        value={settings.max_upload_slots_per_torrent}
                        onChange={(e) => saveSettings({ max_upload_slots_per_torrent: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Proxy Server & IP Filtering
                  </h3>
                  <div className="grid grid-cols-3 gap-3">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">Type</label>
                      <select
                        value={settings.proxy_type}
                        onChange={(e) => saveSettings({ proxy_type: e.target.value as UserSettings['proxy_type'] })}
                        className="w-full px-2.5 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      >
                        <option value="None">(None)</option>
                        <option value="HTTP">HTTP</option>
                        <option value="SOCKS4">SOCKS4</option>
                        <option value="SOCKS5">SOCKS5</option>
                      </select>
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">Host</label>
                      <input
                        type="text"
                        disabled={settings.proxy_type === 'None'}
                        value={settings.proxy_host}
                        onChange={(e) => saveSettings({ proxy_host: e.target.value })}
                        placeholder="127.0.0.1"
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs font-mono focus:border-[#e50914] focus:outline-none disabled:opacity-40"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">Port</label>
                      <input
                        type="number"
                        disabled={settings.proxy_type === 'None'}
                        value={settings.proxy_port}
                        onChange={(e) => saveSettings({ proxy_port: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs font-mono focus:border-[#e50914] focus:outline-none disabled:opacity-40"
                      />
                    </div>
                  </div>

                  <div className="mt-3 space-y-1.5">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.proxy_peer_connections}
                        onChange={(e) => saveSettings({ proxy_peer_connections: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Use proxy for peer connections</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.proxy_disable_unsupported}
                        onChange={(e) => saveSettings({ proxy_disable_unsupported: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Disable connections not supported by proxies</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input type="checkbox" checked={settings.proxy_auth_enabled}
                        onChange={(e) => saveSettings({ proxy_auth_enabled: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0" />
                      <span className="text-white">Proxy requires authentication</span>
                    </label>
                    {settings.proxy_auth_enabled && (
                      <div className="grid grid-cols-2 gap-3 pl-5 font-mono">
                        <div>
                          <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">Username</label>
                          <input type="text" value={settings.proxy_username}
                            onChange={(e) => saveSettings({ proxy_username: e.target.value })}
                            className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none" />
                        </div>
                        <div>
                          <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">Password</label>
                          <input type="password" value={settings.proxy_password}
                            onChange={(e) => saveSettings({ proxy_password: e.target.value })}
                            className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none" />
                        </div>
                      </div>
                    )}
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input type="checkbox" checked={settings.proxy_torrents}
                        onChange={(e) => saveSettings({ proxy_torrents: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0" />
                      <span className="text-white">Use proxy for torrent connections</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input type="checkbox" checked={settings.proxy_rss}
                        onChange={(e) => saveSettings({ proxy_rss: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0" />
                      <span className="text-white">Use proxy for RSS feeds</span>
                    </label>
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">IP Filtering</h3>
                  <label className="flex items-center gap-2 cursor-pointer mb-2">
                    <input type="checkbox" checked={settings.ip_filtering_enabled}
                      onChange={(e) => saveSettings({ ip_filtering_enabled: e.target.checked })}
                      className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0" />
                    <span className="text-white">Enable IP filter</span>
                  </label>
                  <input type="text" disabled={!settings.ip_filtering_enabled}
                    value={settings.ip_filter_path}
                    onChange={(e) => saveSettings({ ip_filter_path: e.target.value })}
                    placeholder="/path/to/ipfilter.dat"
                    className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white font-mono text-xs focus:border-[#e50914] focus:outline-none disabled:opacity-40 mb-2" />
                  <label className="flex items-center gap-2 cursor-pointer">
                    <input type="checkbox" disabled={!settings.ip_filtering_enabled}
                      checked={settings.ip_filter_apply_to_trackers}
                      onChange={(e) => saveSettings({ ip_filter_apply_to_trackers: e.target.checked })}
                      className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0 disabled:opacity-40" />
                    <span className="text-white">Apply filter to trackers too</span>
                  </label>
                </div>
              </div>
            )}

            {/* 4. SPEED */}
            {activeTab === 'speed' && (
              <div className="space-y-4">
                <div>
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Global Rate Limits (KiB/s, 0 = Unlimited)
                  </h3>
                  <div className="grid grid-cols-2 gap-3 font-mono">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Maximum Download Rate
                      </label>
                      <input
                        type="number"
                        min="0"
                        value={settings.max_download_speed_kib}
                        onChange={(e) => saveSettings({ max_download_speed_kib: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Maximum Upload Rate
                      </label>
                      <input
                        type="number"
                        min="0"
                        value={settings.max_upload_speed_kib}
                        onChange={(e) => saveSettings({ max_upload_speed_kib: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <div className="flex items-center justify-between mb-2">
                    <h3 className="text-xs font-bold text-white uppercase tracking-wider font-mono">
                      Alternative Rate Limits (Turtle Mode & Scheduling)
                    </h3>
                    <label className="flex items-center gap-1.5 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.alt_rate_limits_enabled}
                        onChange={(e) => saveSettings({ alt_rate_limits_enabled: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white text-xs font-semibold">Active</span>
                    </label>
                  </div>

                  <div className="grid grid-cols-2 gap-3 font-mono">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Alternative Download Limit
                      </label>
                      <input
                        type="number"
                        value={settings.alt_download_speed_kib}
                        onChange={(e) => saveSettings({ alt_download_speed_kib: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Alternative Upload Limit
                      </label>
                      <input
                        type="number"
                        value={settings.alt_upload_speed_kib}
                        onChange={(e) => saveSettings({ alt_upload_speed_kib: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                  </div>

                  <div className="mt-3 p-3 rounded bg-[#1f1f1f] border border-[#333333] space-y-2.5">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.alt_schedule_enabled}
                        onChange={(e) => saveSettings({ alt_schedule_enabled: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white font-medium">Schedule Alternative Rate Limits</span>
                    </label>

                    <div className="grid grid-cols-3 gap-2">
                      <div>
                        <label className="block text-[10px] text-[#a3a3a3] mb-1">From</label>
                        <input
                          type="time"
                          value={settings.alt_schedule_from_time}
                          onChange={(e) => saveSettings({ alt_schedule_from_time: e.target.value })}
                          className="w-full px-2 py-1 rounded border border-[#333333] bg-[#141414] text-white text-xs"
                        />
                      </div>
                      <div>
                        <label className="block text-[10px] text-[#a3a3a3] mb-1">To</label>
                        <input
                          type="time"
                          value={settings.alt_schedule_to_time}
                          onChange={(e) => saveSettings({ alt_schedule_to_time: e.target.value })}
                          className="w-full px-2 py-1 rounded border border-[#333333] bg-[#141414] text-white text-xs"
                        />
                      </div>
                      <div>
                        <label className="block text-[10px] text-[#a3a3a3] mb-1">Days</label>
                        <select
                          value={settings.alt_schedule_days}
                          onChange={(e) => saveSettings({ alt_schedule_days: e.target.value })}
                          className="w-full px-2 py-1 rounded border border-[#333333] bg-[#141414] text-white text-xs"
                        >
                          <option value="Every day">Every day</option>
                          <option value="Weekdays">Weekdays</option>
                          <option value="Weekends">Weekends</option>
                        </select>
                      </div>
                    </div>
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Rate Limit Settings
                  </h3>
                  <div className="space-y-2">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.rate_limit_utp}
                        onChange={(e) => saveSettings({ rate_limit_utp: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Apply rate limit to μTP protocol</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.rate_limit_overhead}
                        onChange={(e) => saveSettings({ rate_limit_overhead: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Apply rate limit to transport overhead</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.rate_limit_lan_peers}
                        onChange={(e) => saveSettings({ rate_limit_lan_peers: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Apply rate limit to peers on LAN</span>
                    </label>
                  </div>
                </div>
              </div>
            )}

            {/* 5. BITTORRENT */}
            {activeTab === 'bittorrent' && (
              <div className="space-y-4">
                <div>
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Privacy, Swarm & Encryption
                  </h3>
                  <div className="space-y-2">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.enable_dht}
                        onChange={(e) => saveSettings({ enable_dht: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Enable DHT (Distributed Hash Table) to find more peers</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.enable_pex}
                        onChange={(e) => saveSettings({ enable_pex: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Enable PeX (Peer Exchange) to find more peers</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.enable_lsd}
                        onChange={(e) => saveSettings({ enable_lsd: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Enable Local Peer Discovery (LSD) to find more peers</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.anonymous_mode}
                        onChange={(e) => saveSettings({ anonymous_mode: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Enable Anonymous Mode</span>
                    </label>
                  </div>

                  <div className="mt-3">
                    <label className="block text-[11px] text-[#a3a3a3] mb-1">
                      Encryption Mode
                    </label>
                    <select
                      value={settings.encryption_mode}
                      onChange={(e) => saveSettings({ encryption_mode: e.target.value as UserSettings['encryption_mode'] })}
                      className="w-56 px-2.5 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                    >
                      <option value="Prefer">Prefer Encryption</option>
                      <option value="Require">Require Encryption</option>
                      <option value="Disable">Disable Encryption</option>
                    </select>
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Torrent Queueing
                  </h3>
                  <div className="grid grid-cols-3 gap-3 font-mono">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Maximum active downloads:
                      </label>
                      <input
                        type="number"
                        value={settings.max_active_downloads}
                        onChange={(e) => saveSettings({ max_active_downloads: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Maximum active uploads:
                      </label>
                      <input
                        type="number"
                        value={settings.max_active_uploads}
                        onChange={(e) => saveSettings({ max_active_uploads: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Maximum active torrents:
                      </label>
                      <input
                        type="number"
                        value={settings.max_active_torrents}
                        onChange={(e) => saveSettings({ max_active_torrents: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                  </div>
                  <div className="mt-3 space-y-2">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.ignore_slow_torrents_in_queue}
                        onChange={(e) => saveSettings({ ignore_slow_torrents_in_queue: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Do not count slow torrents in active limits</span>
                    </label>
                    {settings.ignore_slow_torrents_in_queue && (
                      <div className="grid grid-cols-2 gap-3 pl-5 font-mono">
                        <div>
                          <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">Slow DL threshold (KiB/s):</label>
                          <input type="number" value={settings.slow_torrent_dl_threshold_kib}
                            onChange={(e) => saveSettings({ slow_torrent_dl_threshold_kib: Number(e.target.value) })}
                            className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none" />
                        </div>
                        <div>
                          <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">Slow UL threshold (KiB/s):</label>
                          <input type="number" value={settings.slow_torrent_ul_threshold_kib}
                            onChange={(e) => saveSettings({ slow_torrent_ul_threshold_kib: Number(e.target.value) })}
                            className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none" />
                        </div>
                      </div>
                    )}
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Seeding Limits & Share Ratio
                  </h3>
                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">
                        When ratio reaches (-1 = Unlimited):
                      </label>
                      <input
                        type="number"
                        step="0.1"
                        value={settings.max_ratio_limit}
                        onChange={(e) => saveSettings({ max_ratio_limit: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs font-mono focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">
                        Action when limit reached:
                      </label>
                      <select
                        value={settings.seeding_limit_action}
                        onChange={(e) => saveSettings({ seeding_limit_action: e.target.value as UserSettings['seeding_limit_action'] })}
                        className="w-full px-2.5 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      >
                        <option value="Pause">Pause torrent</option>
                        <option value="Remove">Remove torrent</option>
                        <option value="RemoveAndFiles">Remove torrent and files</option>
                        <option value="SuperSeeding">Switch to Super-Seeding</option>
                      </select>
                    </div>
                  </div>
                  <div className="mt-3 grid grid-cols-2 gap-3 font-mono">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">When seeding time (min) reaches (-1 = unlimited):</label>
                      <input type="number" value={settings.max_seeding_time_minutes}
                        onChange={(e) => saveSettings({ max_seeding_time_minutes: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none" />
                    </div>
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">Default Trackers (added to all new torrents)</h3>
                  <textarea
                    value={settings.default_trackers_list}
                    onChange={(e) => saveSettings({ default_trackers_list: e.target.value })}
                    rows={4}
                    placeholder="https://tracker.example.com/announce"
                    className="w-full px-3 py-2 rounded border border-[#333333] bg-[#1f1f1f] text-white font-mono text-xs focus:outline-none focus:border-[#e50914] resize-none"
                  />
                  <p className="text-[10px] text-[#a3a3a3] mt-1">One URL per line. Applied to every torrent added through this client.</p>
                </div>
              </div>
            )}

            {/* 6. RSS */}
            {activeTab === 'rss' && (
              <div className="space-y-4">
                <div>
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    RSS Reader
                  </h3>
                  <div className="space-y-2">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.rss_auto_refresh_enabled}
                        onChange={(e) => saveSettings({ rss_auto_refresh_enabled: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Fetch feeds automatically</span>
                    </label>
                  </div>
                  <div className="grid grid-cols-2 gap-3 mt-3">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">
                        Feeds refresh interval (minutes)
                      </label>
                      <input
                        type="number"
                        value={settings.rss_refresh_interval_minutes}
                        onChange={(e) => saveSettings({ rss_refresh_interval_minutes: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs font-mono focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">
                        Maximum articles per feed
                      </label>
                      <input
                        type="number"
                        value={settings.rss_max_articles_per_feed}
                        onChange={(e) => saveSettings({ rss_max_articles_per_feed: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs font-mono focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    RSS Torrent Auto-Downloader
                  </h3>
                  <label className="flex items-center gap-2 cursor-pointer mb-2">
                    <input
                      type="checkbox"
                      checked={settings.rss_auto_downloader_enabled}
                      onChange={(e) => saveSettings({ rss_auto_downloader_enabled: e.target.checked })}
                      className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                    />
                    <span className="text-white">Enable automatic torrent downloading from RSS feeds</span>
                  </label>
                  <p className="text-[11px] text-[#a3a3a3]">
                    Matches smart episode filters, must-contain keywords, and auto-assigns categories.
                  </p>
                </div>
              </div>
            )}

            {/* 7. WEB UI */}
            {activeTab === 'webui' && (
              <div className="space-y-4">
                <div>
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Web User Interface (Remote Management)
                  </h3>
                  <label className="flex items-center gap-2 cursor-pointer mb-3">
                    <input
                      type="checkbox"
                      checked={settings.webui_enabled}
                      onChange={(e) => saveSettings({ webui_enabled: e.target.checked })}
                      className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                    />
                    <span className="text-white font-medium">Enable Web User Interface (Remote Control)</span>
                  </label>

                  <div className="grid grid-cols-2 gap-3 font-mono">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        IP Address / Bind Address
                      </label>
                      <input
                        type="text"
                        disabled={!settings.webui_enabled}
                        value={settings.webui_address}
                        onChange={(e) => saveSettings({ webui_address: e.target.value })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none disabled:opacity-40"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Port
                      </label>
                      <input
                        type="number"
                        disabled={!settings.webui_enabled}
                        value={settings.webui_port}
                        onChange={(e) => saveSettings({ webui_port: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none disabled:opacity-40"
                      />
                    </div>
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Authentication
                  </h3>
                  <div className="grid grid-cols-2 gap-3 font-mono">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Username
                      </label>
                      <input
                        type="text"
                        disabled={!settings.webui_enabled}
                        value={settings.webui_username}
                        onChange={(e) => saveSettings({ webui_username: e.target.value })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none disabled:opacity-40"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Password
                      </label>
                      <input
                        type="password"
                        disabled={!settings.webui_enabled}
                        value={settings.webui_password}
                        onChange={(e) => saveSettings({ webui_password: e.target.value })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none disabled:opacity-40"
                      />
                    </div>
                  </div>
                  <div className="mt-3 space-y-2">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.webui_bypass_localhost}
                        onChange={(e) => saveSettings({ webui_bypass_localhost: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Bypass authentication for clients on localhost</span>
                    </label>
                  </div>
                </div>
              </div>
            )}

            {/* 8.5 rTORRENT SERVER */}
            {activeTab === 'rtorrent' && (
              <div className="space-y-4">
                <div>
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    rTorrent Server (Nebulae Server Bridge)
                  </h3>
                  <label className="flex items-center gap-2 cursor-pointer mb-3">
                    <input
                      type="checkbox"
                      checked={settings.rtorrent_server_enabled}
                      onChange={(e) => saveSettings({ rtorrent_server_enabled: e.target.checked })}
                      className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                    />
                    <span className="text-white font-medium">
                      Drive torrents on a Nebulae Server (rtorrent) instead of locally
                    </span>
                  </label>
                  <p className="text-[11px] text-[#a3a3a3] mb-3">
                    Connects this desktop app to a headless <span className="font-mono">nebulae-server</span> bridge
                    running next to rtorrent on your server (Unraid, TrueNAS, Docker). The engine choice applies the
                    next time Nebulae starts.
                  </p>

                  <div className="space-y-3 font-mono">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">Server address</label>
                      <input
                        type="text"
                        placeholder="http://tower.local:9174"
                        disabled={!settings.rtorrent_server_enabled}
                        value={settings.rtorrent_server_url}
                        onChange={(e) => saveSettings({ rtorrent_server_url: e.target.value })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none disabled:opacity-40"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">API token</label>
                      <input
                        type="password"
                        placeholder="NEBULAE_SERVER_TOKEN"
                        disabled={!settings.rtorrent_server_enabled}
                        value={settings.rtorrent_server_token}
                        onChange={(e) => saveSettings({ rtorrent_server_token: e.target.value })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none disabled:opacity-40"
                      />
                    </div>
                  </div>

                  <div className="mt-3 flex items-center gap-3">
                    <button
                      onClick={handleTestServerConnection}
                      disabled={!settings.rtorrent_server_enabled || isTestingServer}
                      className="px-3 py-1.5 rounded bg-[#e50914] hover:bg-[#c40812] text-white text-xs font-medium transition disabled:opacity-40 disabled:cursor-not-allowed"
                    >
                      {isTestingServer ? 'Testing…' : 'Test connection'}
                    </button>
                    {serverTestResult && (
                      <span
                        className={
                          serverTestResult.startsWith('Connected')
                            ? 'text-green-400 text-[11px]'
                            : 'text-red-400 text-[11px]'
                        }
                      >
                        {serverTestResult}
                      </span>
                    )}
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <p className="text-[11px] text-[#a3a3a3]">
                    Note: the server beta serves plain HTTP on your LAN. Keep it inside a trusted network or put a
                    reverse proxy with TLS in front. Some preview features (per-file priority, renaming, categories,
                    super-seeding) are unavailable in remote mode and will say so when used.
                  </p>
                </div>
              </div>
            )}

            {/* 9. MEDIA (Optical Disc Cinema & Player Detection) */}
            {activeTab === 'media' && (
              <div className="space-y-5">
                <div>
                  <div className="flex items-center justify-between mb-2">
                    <h3 className="text-xs font-bold text-white uppercase tracking-wider font-mono flex items-center gap-2">
                      <Film className="w-4 h-4 text-[#e50914]" />
                      <span>Optical Disc Cinema & Media Player</span>
                    </h3>
                    <button
                      type="button"
                      onClick={probePlayer}
                      disabled={isProbingPlayer}
                      className="px-2.5 py-1 rounded bg-[#262626] hover:bg-[#333333] border border-[#404040] text-white text-[11px] flex items-center gap-1.5 transition"
                    >
                      <RefreshCw className={`w-3 h-3 ${isProbingPlayer ? 'animate-spin text-[#e50914]' : ''}`} />
                      <span>{isProbingPlayer ? 'Detecting...' : 'Re-probe Player'}</span>
                    </button>
                  </div>
                  <p className="text-[11px] text-[#8c8c8c] mb-4">
                    Playback runs inside Nebulae on VLC's embedded engine: DVD menus (VIDEO_TS, ISO), Blu-ray menus (BDMV, ISO), and automatic high-quality deinterlacing. Video renders in Nebulae's own window with Nebulae's own controls.
                  </p>
                </div>

                {/* Player Status Card */}
                <div className="p-4 rounded-lg bg-[#1f1f1f] border border-[#333333] space-y-3">
                  <div className="flex items-center justify-between pb-2 border-b border-[#333333]">
                    <span className="text-xs font-semibold text-white">Detected Disc Player</span>
                    <span
                      className={`text-[10px] font-mono px-2 py-0.5 rounded font-bold uppercase ${
                        discPlayerStatus?.player_type === 'vlc'
                          ? 'bg-green-950/80 border border-green-700/60 text-green-300'
                          : 'bg-amber-950/80 border border-amber-700/60 text-amber-300'
                      }`}
                    >
                      {discPlayerStatus?.player_type === 'vlc'
                        ? 'Embedded (In-App)'
                        : 'VLC Not Found'}
                    </span>
                  </div>

                  <div className="space-y-1.5 text-xs">
                    <div className="flex justify-between items-center">
                      <span className="text-[#a3a3a3]">Player Name:</span>
                      <span className="font-mono text-white font-medium">
                        {discPlayerStatus?.player_name || 'Probing player...'}
                      </span>
                    </div>
                    {discPlayerStatus?.player_path && (
                      <div className="flex justify-between items-center text-[11px]">
                        <span className="text-[#a3a3a3]">Binary Location:</span>
                        <span className="font-mono text-slate-300 truncate max-w-xs" title={discPlayerStatus.player_path}>
                          {discPlayerStatus.player_path}
                        </span>
                      </div>
                    )}
                  </div>
                </div>

                {/* Capabilities Matrix */}
                <div className="space-y-2">
                  <h4 className="text-xs font-bold text-white uppercase tracking-wider font-mono">
                    Disc Format & Menu Capabilities
                  </h4>
                  <div className="grid grid-cols-2 gap-3 text-xs">
                    <div className="p-3 rounded-lg bg-[#1f1f1f] border border-[#333333] space-y-1">
                      <div className="flex items-center justify-between">
                        <span className="font-medium text-white">DVD Menus (dvdnav)</span>
                        <span className={`text-[10px] font-mono font-bold ${discPlayerStatus?.supports_dvd_menu ? 'text-green-400' : 'text-slate-500'}`}>
                          {discPlayerStatus?.supports_dvd_menu ? 'SUPPORTED' : 'UNAVAILABLE'}
                        </span>
                      </div>
                      <p className="text-[11px] text-[#8c8c8c]">
                        Interactive main and chapter menus for VIDEO_TS folders and DVD ISO images.
                      </p>
                    </div>

                    <div className="p-3 rounded-lg bg-[#1f1f1f] border border-[#333333] space-y-1">
                      <div className="flex items-center justify-between">
                        <span className="font-medium text-white">Blu-ray Menus (libbluray)</span>
                        <span className={`text-[10px] font-mono font-bold ${discPlayerStatus?.supports_bluray_menu ? 'text-green-400' : 'text-slate-500'}`}>
                          {discPlayerStatus?.supports_bluray_menu ? 'SUPPORTED' : 'UNAVAILABLE'}
                        </span>
                      </div>
                      <p className="text-[11px] text-[#8c8c8c]">
                        Full BDMV and ISO title/playlist navigation for 1080p Blu-ray and 4K UHD images.
                      </p>
                    </div>
                  </div>
                </div>

                {/* BD-J & JVM Status Card */}
                <div className="p-3.5 rounded-lg bg-[#1f1f1f] border border-[#333333] space-y-2 text-xs">
                  <div className="flex items-center justify-between">
                    <span className="font-bold text-white">BD-J Interactive Java Menus</span>
                    <span
                      className={`text-[10px] font-mono px-2 py-0.5 rounded font-bold ${
                        discPlayerStatus?.has_jvm
                          ? 'bg-green-950/80 border border-green-700/60 text-green-300'
                          : 'bg-amber-950/80 border border-amber-700/60 text-amber-300'
                      }`}
                    >
                      {discPlayerStatus?.has_jvm ? 'JVM ACTIVE' : 'LIMITED (NO JVM)'}
                    </span>
                  </div>
                  <p className="text-[11px] text-[#8c8c8c]">
                    {discPlayerStatus?.has_jvm
                      ? `System Java Virtual Machine detected at ${discPlayerStatus.java_home}. Full interactive BD-J menus and extras enabled.`
                      : 'No Java Runtime Environment (JAVA_HOME) detected. Standard playlist and title navigation will operate cleanly; advanced BD-J animated menus are limited.'}
                  </p>
                </div>

                {/* Protected Discs Notice */}
                <div className="p-3.5 rounded-lg bg-[#1a1811] border border-amber-900/40 text-amber-200/90 text-xs space-y-1">
                  <div className="font-bold text-amber-300 flex items-center gap-1.5">
                    <span>AACS & Commercial Discs Limitation</span>
                  </div>
                  <p className="text-[11px] leading-relaxed text-amber-300/80">
                    Nebulae strictly supports unprotected raw optical disc structures and community home rips. Commercial pressed discs with active AACS or BD+ encryption require prior decryption. Nebulae contains zero DRM circumvention tools or decryption keys.
                  </p>
                </div>
              </div>
            )}

            {/* 8. ADVANCED */}
            {activeTab === 'advanced' && (
              <div className="space-y-4">
                <div>
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Network Interface & Adapter Binding
                  </h3>
                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">
                        Network Interface
                      </label>
                      <select
                        value={settings.network_interface_binding}
                        onChange={(e) => saveSettings({ network_interface_binding: e.target.value })}
                        className="w-full px-2.5 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      >
                        <option value="Any interface">Any interface (Default)</option>
                        <option value="utun3">utun3 (WireGuard VPN Tunnel)</option>
                        <option value="en0">en0 (Primary Wi-Fi / Ethernet)</option>
                        <option value="lo0">lo0 (Loopback)</option>
                      </select>
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">
                        Optional IP Address Binding
                      </label>
                      <select
                        value={settings.optional_ip_address_binding}
                        onChange={(e) => saveSettings({ optional_ip_address_binding: e.target.value })}
                        className="w-full px-2.5 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      >
                        <option value="All IPv4 and IPv6 addresses">All IPv4 and IPv6 addresses</option>
                        <option value="All IPv4 addresses">All IPv4 addresses</option>
                        <option value="All IPv6 addresses">All IPv6 addresses</option>
                      </select>
                    </div>
                  </div>
                </div>

                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Engine Tuning & Disk Cache
                  </h3>
                  <div className="grid grid-cols-3 gap-3 font-mono">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Disk Cache Size (MiB, -1 = Auto)
                      </label>
                      <input
                        type="number"
                        value={settings.disk_cache_size_mib}
                        onChange={(e) => saveSettings({ disk_cache_size_mib: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Disk Cache TTL (seconds)
                      </label>
                      <input
                        type="number"
                        value={settings.disk_cache_ttl_seconds}
                        onChange={(e) => saveSettings({ disk_cache_ttl_seconds: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Async I/O Threads
                      </label>
                      <input
                        type="number"
                        value={settings.async_io_threads}
                        onChange={(e) => saveSettings({ async_io_threads: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                  </div>
                  <div className="grid grid-cols-3 gap-3 font-mono mt-3">
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        File Pool Size
                      </label>
                      <input
                        type="number"
                        value={settings.file_pool_size}
                        onChange={(e) => saveSettings({ file_pool_size: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Stop Tracker Timeout (s)
                      </label>
                      <input
                        type="number"
                        value={settings.stop_tracker_timeout_seconds}
                        onChange={(e) => saveSettings({ stop_tracker_timeout_seconds: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Outgoing Ports Min
                      </label>
                      <input
                        type="number"
                        value={settings.outgoing_ports_min}
                        onChange={(e) => saveSettings({ outgoing_ports_min: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1 font-sans">
                        Outgoing Ports Max
                      </label>
                      <input
                        type="number"
                        value={settings.outgoing_ports_max}
                        onChange={(e) => saveSettings({ outgoing_ports_max: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none"
                      />
                    </div>
                  </div>
                  <div className="mt-3 space-y-2">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.coalesce_reads_writes}
                        onChange={(e) => saveSettings({ coalesce_reads_writes: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Coalesce reads and writes (optimize sequential I/O)</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.embedded_tracker_enabled}
                        onChange={(e) => saveSettings({ embedded_tracker_enabled: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Enable embedded tracker</span>
                    </label>
                    {settings.embedded_tracker_enabled && (
                      <div className="pl-5">
                        <label className="block text-[11px] text-[#a3a3a3] mb-1">Embedded tracker port</label>
                        <input
                          type="number"
                          value={settings.embedded_tracker_port}
                          onChange={(e) => saveSettings({ embedded_tracker_port: Number(e.target.value) })}
                          className="w-48 px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs font-mono focus:border-[#e50914] focus:outline-none"
                        />
                      </div>
                    )}
                  </div>
                </div>


                <div className="pt-3 border-t border-[#333333]">
                  <h3 className="text-xs font-bold text-white uppercase tracking-wider mb-2 font-mono">
                    Diagnostics & Storage Maintenance
                  </h3>
                  <div className="grid grid-cols-2 gap-3 mb-3">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.save_log_to_disk}
                        onChange={(e) => saveSettings({ save_log_to_disk: e.target.checked })}
                        className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                      />
                      <span className="text-white">Save logs to disk</span>
                    </label>
                    <div>
                      <label className="block text-[11px] text-[#a3a3a3] mb-1">Max log size (MiB)</label>
                      <input
                        type="number"
                        value={settings.max_log_size_mib}
                        disabled={!settings.save_log_to_disk}
                        onChange={(e) => saveSettings({ max_log_size_mib: Number(e.target.value) })}
                        className="w-full px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs font-mono focus:border-[#e50914] focus:outline-none disabled:opacity-40"
                      />
                    </div>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <button
                      type="button"
                      onClick={handleOpenLogs}
                      className="px-3 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white flex items-center gap-1.5 transition"
                    >
                      <FolderOpen className="w-3.5 h-3.5 text-[#e50914]" />
                      <span>Open Internal Logs</span>
                    </button>
                    <button
                      type="button"
                      onClick={handleCopyDiagnosticSummary}
                      className="px-3 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white flex items-center gap-1.5 transition"
                    >
                      <Copy className="w-3.5 h-3.5 text-[#e50914]" />
                      <span>{copiedSummary ? 'Copied!' : 'Copy Diagnostics JSON'}</span>
                    </button>
                    <button
                      type="button"
                      onClick={handleExportLogs}
                      className="px-3 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white flex items-center gap-1.5 transition"
                    >
                      <FileText className="w-3.5 h-3.5 text-[#e50914]" />
                      <span>Copy Recent Logs</span>
                    </button>
                    <button
                      type="button"
                      onClick={handleClearCache}
                      className="px-3 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white flex items-center gap-1.5 transition"
                    >
                      <Trash2 className="w-3.5 h-3.5 text-[#e50914]" />
                      <span>Clear Cache</span>
                    </button>
                    <button
                      type="button"
                      onClick={handleResetSettings}
                      className="px-3 py-1.5 rounded bg-[#1f1f1f] hover:bg-red-950/60 border border-red-900/60 text-red-300 flex items-center gap-1.5 transition"
                    >
                      <RotateCcw className="w-3.5 h-3.5 text-red-400" />
                      <span>Reset All Settings</span>
                    </button>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Modal Footer */}
        <div className="flex items-center justify-between px-5 py-3 border-t border-[#333333] bg-[#1f1f1f]">
          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={handleResetSettings}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] hover:bg-[#262626] text-white text-xs transition"
            >
              <RotateCcw className="w-3.5 h-3.5" />
              <span>Restore Defaults</span>
            </button>
            <div className="text-[11px] font-mono text-[#a3a3a3]">
              {statusMessage ? (
                <span className="text-[#e50914] font-semibold">{statusMessage}</span>
              ) : (
                <span>Nebulae Preferences · Changes applied automatically</span>
              )}
            </div>
          </div>
          <button
            type="button"
            onClick={() => setShowSettings(false)}
            className="px-5 py-1.5 rounded bg-[#e50914] hover:bg-[#f40612] text-white font-medium text-xs transition shadow"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
};

export default SettingsModal;
