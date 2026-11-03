import React, { useState, useEffect, useMemo, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useUiStore } from '../../stores/uiStore';
import {
  QbitLocationDto,
  ScanResult,
  ImportOptions,
  ImportSummaryReport,
  QbitProcessCheck,
} from '../../types';
import { formatBytes } from '../../lib/formatters';
import {
  X,
  Search,
  Folder,
  AlertTriangle,
  CheckCircle2,
  Copy,
  FolderOpen,
  RefreshCw,
  Trash2,
  Download,
  AlertCircle,
  FileCheck,
  Shield,
} from 'lucide-react';

type Step = 'confirm_start' | 'locating' | 'browse' | 'scanning' | 'selection' | 'process_blocker' | 'second_confirm' | 'importing' | 'summary';

export const QbitImportModal: React.FC = () => {
  const { showQbitImport, setShowQbitImport } = useUiStore();

  const [step, setStep] = useState<Step>('confirm_start');
  const [dataDir, setDataDir] = useState<string>('');
  const [locationDto, setLocationDto] = useState<QbitLocationDto | null>(null);
  const [scanResult, setScanResult] = useState<ScanResult | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  // Selection list state
  const [selectedHashes, setSelectedHashes] = useState<Set<string>>(new Set());
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [categoryFilter, setCategoryFilter] = useState<string>('all');
  const [removeAfterImport, setRemoveAfterImport] = useState<boolean>(false); // Always starts false
  const [skipRecheck, setSkipRecheck] = useState<boolean>(false);

  // Summary state
  const [summaryReport, setSummaryReport] = useState<ImportSummaryReport | null>(null);
  const [copiedLog, setCopiedLog] = useState<boolean>(false);

  // Reset state when opening
  useEffect(() => {
    if (showQbitImport) {
      setStep('confirm_start');
      setDataDir('');
      setLocationDto(null);
      setScanResult(null);
      setErrorMsg(null);
      setSelectedHashes(new Set());
      setSearchQuery('');
      setCategoryFilter('all');
      setRemoveAfterImport(false);
      setSkipRecheck(false);
      setSummaryReport(null);
      setCopiedLog(false);
    }
  }, [showQbitImport]);

  // Start auto-location
  const handleStartLocating = async () => {
    setStep('locating');
    setErrorMsg(null);
    try {
      const loc = await invoke<QbitLocationDto>('qbit_auto_locate');
      setLocationDto(loc);
      setDataDir(loc.data_dir);
      await runScan(loc.data_dir);
    } catch {
      // Auto-location failed => open folder picker step
      setStep('browse');
    }
  };

  // Run scan on a validated directory
  const runScan = async (targetDir: string) => {
    setStep('scanning');
    setErrorMsg(null);
    try {
      const res = await invoke<ScanResult>('qbit_scan_torrents', { dir: targetDir });
      setScanResult(res);

      // Select all eligible torrents by default (exclude already in Nebula)
      const initial = new Set<string>();
      res.torrents.forEach((t) => {
        if (!t.already_in_nebula) {
          initial.add(t.info_hash.toLowerCase());
        }
      });
      setSelectedHashes(initial);
      setStep('selection');
    } catch (e: unknown) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setStep('browse');
    }
  };

  // Handle custom folder selection
  const handleBrowseFolder = async () => {
    try {
      const chosen = await invoke<string | null>('select_download_directory');
      if (chosen) {
        setDataDir(chosen);
        const loc = await invoke<QbitLocationDto>('qbit_validate_dir', { dir: chosen });
        setLocationDto(loc);
        await runScan(chosen);
      }
    } catch (e: unknown) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
    }
  };

  // Categories list for filter dropdown
  const categories = useMemo(() => {
    if (!scanResult) return [];
    const set = new Set<string>();
    scanResult.torrents.forEach((t) => {
      if (t.category && t.category !== 'None') set.add(t.category);
    });
    return Array.from(set).sort();
  }, [scanResult]);

  // Filtered torrents
  const filteredTorrents = useMemo(() => {
    if (!scanResult) return [];
    return scanResult.torrents.filter((t) => {
      if (categoryFilter !== 'all' && t.category !== categoryFilter) return false;
      if (!searchQuery.trim()) return true;
      const q = searchQuery.toLowerCase();
      return (
        t.name.toLowerCase().includes(q) ||
        t.save_path.toLowerCase().includes(q) ||
        t.category.toLowerCase().includes(q)
      );
    });
  }, [scanResult, categoryFilter, searchQuery]);

  // Toggle selection
  const toggleTorrent = (hash: string, disabled: boolean) => {
    if (disabled) return;
    const lower = hash.toLowerCase();
    setSelectedHashes((prev) => {
      const next = new Set(prev);
      if (next.has(lower)) {
        next.delete(lower);
      } else {
        next.add(lower);
      }
      return next;
    });
  };

  const handleSelectAll = useCallback(() => {
    if (!scanResult) return;
    setSelectedHashes((prev) => {
      const next = new Set(prev);
      filteredTorrents.forEach((t) => {
        if (!t.already_in_nebula) {
          next.add(t.info_hash.toLowerCase());
        }
      });
      return next;
    });
  }, [scanResult, filteredTorrents]);

  const handleSelectNone = () => {
    if (!scanResult) return;
    if (categoryFilter !== 'all' || searchQuery.trim()) {
      setSelectedHashes((prev) => {
        const next = new Set(prev);
        filteredTorrents.forEach((t) => {
          next.delete(t.info_hash.toLowerCase());
        });
        return next;
      });
    } else {
      setSelectedHashes(new Set());
    }
  };

  // Handle clicking "Import" on selection screen
  const handleProceedFromSelection = async () => {
    if (selectedHashes.size === 0) return;

    if (removeAfterImport) {
      // Step 5b Preconditions: check if qBittorrent is running
      const proc = await invoke<QbitProcessCheck>('qbit_check_running');
      if (proc.is_running) {
        setStep('process_blocker');
        return;
      }
      // Show Second Confirmation
      setStep('second_confirm');
      return;
    }

    // Direct import without removal
    await executeImport(false);
  };

  // Execute import
  const executeImport = async (doRemove: boolean) => {
    setStep('importing');
    setErrorMsg(null);
    try {
      const opts: ImportOptions = {
        info_hashes: Array.from(selectedHashes),
        skip_recheck: skipRecheck,
        remove_from_qbit: doRemove,
      };
      const report = await invoke<ImportSummaryReport>('qbit_execute_import', {
        options: opts,
      });
      setSummaryReport(report);
      setStep('summary');
    } catch (e: unknown) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setStep('selection');
    }
  };

  // Copy log to clipboard
  const handleCopyLog = async () => {
    if (!summaryReport) return;
    try {
      await navigator.clipboard.writeText(summaryReport.log_preview);
      setCopiedLog(true);
      setTimeout(() => setCopiedLog(false), 2000);
    } catch {}
  };

  // Open backup folder in OS file manager
  const handleOpenBackupFolder = async () => {
    if (!summaryReport?.backup_dir) return;
    try {
      await invoke('open_torrent_folder', { path: summaryReport.backup_dir });
    } catch {}
  };

  // Keyboard navigation
  useEffect(() => {
    if (!showQbitImport) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (step !== 'importing') setShowQbitImport(false);
      } else if ((e.metaKey || e.ctrlKey) && e.key === 'a') {
        if (step === 'selection') {
          e.preventDefault();
          handleSelectAll();
        }
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [showQbitImport, step, handleSelectAll, setShowQbitImport]);

  if (!showQbitImport) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm select-none font-sans">
      <div className="relative w-full max-w-4xl bg-[#141414] border border-[#333333] rounded-xl shadow-2xl overflow-hidden flex flex-col max-h-[92vh] text-white">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-[#333333] bg-[#1f1f1f]">
          <div className="flex items-center gap-2">
            <Download className="w-4 h-4 text-[#e50914]" />
            <h2 className="text-sm font-bold text-white font-mono">
              Import from qBittorrent
            </h2>
          </div>
          {step !== 'importing' && (
            <button
              onClick={() => setShowQbitImport(false)}
              className="p-1 rounded text-[#a3a3a3] hover:text-white transition"
              aria-label="Close"
            >
              <X className="w-4 h-4" />
            </button>
          )}
        </div>

        {/* Body content based on step */}
        <div className="p-5 overflow-y-auto flex-1 text-xs">
          {/* STEP 1: INITIAL CONFIRMATION (R2) */}
          {step === 'confirm_start' && (
            <div className="space-y-4 max-w-xl mx-auto py-4">
              <div className="flex items-start gap-3 p-4 rounded-lg bg-[#1f1f1f] border border-[#333333]">
                <FileCheck className="w-5 h-5 text-[#e50914] flex-shrink-0 mt-0.5" />
                <div className="space-y-1">
                  <p className="text-sm font-semibold text-white">
                    Copy qBittorrent torrents with their settings into Nebula?
                  </p>
                  <p className="text-xs text-[#a3a3a3]">
                    Existing download paths and file renames will be preserved. Your downloaded payload files will never be moved or modified.
                  </p>
                </div>
              </div>

              <div className="flex justify-end gap-2 pt-2">
                <button
                  type="button"
                  onClick={() => setShowQbitImport(false)}
                  className="px-4 py-2 rounded bg-[#262626] hover:bg-[#333333] text-white font-medium transition"
                >
                  No
                </button>
                <button
                  type="button"
                  autoFocus
                  onClick={handleStartLocating}
                  className="px-4 py-2 rounded bg-[#e50914] hover:bg-[#b80710] text-white font-medium transition shadow-lg shadow-red-900/20"
                >
                  Yes, scan for torrents
                </button>
              </div>
            </div>
          )}

          {/* STEP 2: LOCATING & SCANNING */}
          {(step === 'locating' || step === 'scanning') && (
            <div className="py-16 flex flex-col items-center justify-center space-y-3">
              <RefreshCw className="w-8 h-8 text-[#e50914] animate-spin" />
              <p className="text-sm font-medium text-white">
                {step === 'locating' ? 'Locating qBittorrent directory...' : 'Scanning resume database and torrents...'}
              </p>
              <p className="text-xs text-[#a3a3a3] font-mono">
                {dataDir || 'Checking standard application directories'}
              </p>
            </div>
          )}

          {/* STEP 2b: BROWSE FOLDER (R3 FALLBACK) */}
          {step === 'browse' && (
            <div className="space-y-4 max-w-xl mx-auto py-4">
              <div className="p-4 rounded-lg bg-red-950/20 border border-red-800/30 flex items-start gap-3">
                <AlertCircle className="w-5 h-5 text-red-400 flex-shrink-0 mt-0.5" />
                <div className="space-y-1">
                  <p className="text-xs font-semibold text-red-200">
                    qBittorrent directory not found automatically
                  </p>
                  <p className="text-xs text-red-300/80">
                    {errorMsg || 'Please select the folder containing torrents.db or BT_backup.'}
                  </p>
                  <p className="text-[11px] text-[#a3a3a3] pt-1">
                    On macOS: If accessing ~/Library/Application Support is restricted, click &quot;Browse…&quot; to grant access via the system dialog, or grant Full Disk Access to Nebulae in System Settings &gt; Privacy &amp; Security.
                    On Windows: If accessing restricted profile locations, verify Nebulae has read access.
                  </p>
                </div>
              </div>

              <div className="space-y-2">
                <label className="block text-xs font-semibold text-white">
                  qBittorrent Data Directory:
                </label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    value={dataDir}
                    onChange={(e) => setDataDir(e.target.value)}
                    placeholder="/Users/dev/Library/Application Support/qBittorrent"
                    className="flex-1 px-3 py-2 rounded bg-[#1f1f1f] border border-[#333333] text-white font-mono text-xs focus:outline-none focus:border-[#e50914]"
                  />
                  <button
                    type="button"
                    onClick={handleBrowseFolder}
                    className="px-3 py-2 rounded bg-[#262626] hover:bg-[#333333] text-white font-medium flex items-center gap-1.5 transition"
                  >
                    <Folder className="w-3.5 h-3.5 text-[#e50914]" />
                    <span>Browse…</span>
                  </button>
                </div>
              </div>

              <div className="flex justify-end gap-2 pt-2">
                <button
                  type="button"
                  onClick={() => setShowQbitImport(false)}
                  className="px-4 py-2 rounded bg-[#262626] hover:bg-[#333333] text-white transition"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  disabled={!dataDir.trim()}
                  onClick={() => runScan(dataDir.trim())}
                  className="px-4 py-2 rounded bg-[#e50914] hover:bg-[#b80710] disabled:opacity-40 text-white font-medium transition"
                >
                  Scan Folder
                </button>
              </div>
            </div>
          )}

          {/* STEP 3: SELECTION LIST (R4 & R4b) */}
          {step === 'selection' && scanResult && (
            <div className="space-y-3 flex flex-col h-full">
              {/* Top Bar: Live Counter & Filters */}
              <div className="flex flex-wrap items-center justify-between gap-2 pb-2 border-b border-[#262626]">
                <div className="flex items-center gap-3">
                  <span className="font-mono font-bold text-white text-xs">
                    {selectedHashes.size} of {scanResult.torrents.filter((t) => !t.already_in_nebula).length} selected
                  </span>
                  <div className="flex items-center gap-1 text-[11px]">
                    <button
                      type="button"
                      onClick={handleSelectAll}
                      className="px-2 py-0.5 rounded bg-[#262626] hover:bg-[#333333] text-white transition"
                    >
                      Select all
                    </button>
                    <button
                      type="button"
                      onClick={handleSelectNone}
                      className="px-2 py-0.5 rounded bg-[#262626] hover:bg-[#333333] text-[#a3a3a3] hover:text-white transition"
                    >
                      Select none
                    </button>
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  {categories.length > 0 && (
                    <select
                      value={categoryFilter}
                      onChange={(e) => setCategoryFilter(e.target.value)}
                      className="px-2 py-1 rounded bg-[#1f1f1f] border border-[#333333] text-white text-xs focus:outline-none"
                    >
                      <option value="all">All Categories</option>
                      {categories.map((c) => (
                        <option key={c} value={c}>{c}</option>
                      ))}
                    </select>
                  )}

                  <div className="relative w-56">
                    <Search className="w-3.5 h-3.5 absolute left-2 top-1/2 -translate-y-1/2 text-[#737373]" />
                    <input
                      type="text"
                      value={searchQuery}
                      onChange={(e) => setSearchQuery(e.target.value)}
                      placeholder="Filter torrents..."
                      className="w-full pl-7 pr-2 py-1 rounded bg-[#1f1f1f] border border-[#333333] text-white text-xs focus:outline-none focus:border-[#e50914]"
                    />
                  </div>
                </div>
              </div>

              {/* Warnings Banner */}
              {scanResult.warnings.length > 0 && (
                <div className="p-2.5 rounded bg-yellow-950/20 border border-yellow-800/40 text-yellow-300 text-[11px] flex items-center gap-2">
                  <AlertTriangle className="w-4 h-4 flex-shrink-0" />
                  <div className="space-y-0.5">
                    {scanResult.warnings.map((w, idx) => (
                      <p key={idx}>{w}</p>
                    ))}
                  </div>
                </div>
              )}

              {/* Virtualized/Scrollable Table */}
              <div className="border border-[#262626] rounded-lg overflow-y-auto max-h-[46vh] bg-[#1a1a1a]">
                <table className="w-full text-left border-collapse text-xs">
                  <thead className="sticky top-0 bg-[#222222] border-b border-[#262626] text-[10px] font-semibold text-[#a3a3a3] uppercase font-mono z-10">
                    <tr>
                      <th className="py-2 px-3 w-8 text-center">
                        <span className="sr-only">Select</span>
                      </th>
                      <th className="py-2 px-2.5">Name</th>
                      <th className="py-2 px-2 w-20">Size</th>
                      <th className="py-2 px-2 w-48">Save Path</th>
                      <th className="py-2 px-2 w-28">Progress / State</th>
                      <th className="py-2 px-2 w-20">Category</th>
                      <th className="py-2 px-2.5 w-28">Status</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-[#262626] font-mono text-[11px]">
                    {filteredTorrents.length === 0 ? (
                      <tr>
                        <td colSpan={7} className="py-12 text-center text-[#737373]">
                          {scanResult.torrents.length === 0
                            ? '0 torrents found in qBittorrent profile'
                            : 'No matching torrents found'}
                        </td>
                      </tr>
                    ) : (
                      filteredTorrents.map((t) => {
                        const isChecked = selectedHashes.has(t.info_hash.toLowerCase());
                        const isDisabled = t.already_in_nebula;

                        return (
                          <tr
                            key={t.info_hash}
                            onClick={() => toggleTorrent(t.info_hash, isDisabled)}
                            className={`cursor-pointer transition select-none ${
                              isDisabled
                                ? 'opacity-40 bg-[#161616] cursor-not-allowed'
                                : isChecked
                                ? 'bg-[#e50914]/10 hover:bg-[#e50914]/15'
                                : 'hover:bg-[#222222]'
                            }`}
                          >
                            <td className="py-2 px-3 text-center" onClick={(e) => e.stopPropagation()}>
                              <input
                                type="checkbox"
                                disabled={isDisabled}
                                checked={isChecked}
                                onChange={() => toggleTorrent(t.info_hash, isDisabled)}
                                className="rounded border-[#404040] bg-[#1f1f1f] text-[#e50914] focus:ring-0 cursor-pointer disabled:cursor-not-allowed"
                                aria-label={`Select ${t.name}`}
                              />
                            </td>
                            <td className="py-2 px-2.5 font-sans font-medium text-white truncate max-w-xs" title={t.name}>
                              {t.name}
                            </td>
                            <td className="py-2 px-2 text-[#a3a3a3] whitespace-nowrap">
                              {formatBytes(t.size_bytes)}
                            </td>
                            <td className="py-2 px-2 text-[#a3a3a3] truncate max-w-[12rem]" title={t.save_path}>
                              {t.save_path}
                            </td>
                            <td className="py-2 px-2 whitespace-nowrap">
                              <span className="text-white">{(t.progress * 100).toFixed(0)}%</span>{' '}
                              <span className="text-[#737373] text-[10px]">({t.state})</span>
                            </td>
                            <td className="py-2 px-2 text-[#a3a3a3] truncate max-w-[5rem]">
                              {t.category || '—'}
                            </td>
                            <td className="py-2 px-2.5 whitespace-nowrap">
                              {t.already_in_nebula ? (
                                <span className="px-1.5 py-0.5 rounded text-[10px] bg-neutral-800 text-neutral-400 border border-neutral-700">
                                  Already in Nebula
                                </span>
                              ) : t.missing_path ? (
                                <span className="px-1.5 py-0.5 rounded text-[10px] bg-yellow-950/60 text-yellow-400 border border-yellow-700/50" title="Directory path does not exist on this machine">
                                  Missing path
                                </span>
                              ) : (
                                <span className="px-1.5 py-0.5 rounded text-[10px] bg-emerald-950/60 text-emerald-400 border border-emerald-700/50">
                                  Ready
                                </span>
                              )}
                            </td>
                          </tr>
                        );
                      })
                    )}
                  </tbody>
                </table>
              </div>

              {/* Bottom Options (R4b Removal & Skip Recheck) */}
              <div className="pt-2 space-y-2 border-t border-[#262626]">
                <label className="flex items-center gap-2 cursor-pointer text-xs text-[#a3a3a3] hover:text-white transition">
                  <input
                    type="checkbox"
                    checked={skipRecheck}
                    onChange={(e) => setSkipRecheck(e.target.checked)}
                    className="rounded border-[#404040] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                  />
                  <span>Trust qBittorrent resume data (skip full hash recheck)</span>
                </label>

                {/* R4b Removal Option: Always starts unchecked, disabled when 0 selected */}
                <div className={`p-2.5 rounded border transition ${
                  removeAfterImport
                    ? 'bg-red-950/20 border-red-800/50'
                    : 'bg-[#1a1a1a] border-[#2a2a2a]'
                }`}>
                  <label className={`flex items-start gap-2.5 cursor-pointer ${selectedHashes.size === 0 || !scanResult.is_writable ? 'opacity-40 cursor-not-allowed' : ''}`}>
                    <input
                      type="checkbox"
                      disabled={selectedHashes.size === 0 || !scanResult.is_writable}
                      checked={removeAfterImport}
                      onChange={(e) => setRemoveAfterImport(e.target.checked)}
                      className="rounded border-[#404040] bg-[#1f1f1f] text-red-500 focus:ring-0 mt-0.5"
                    />
                    <div>
                      <span className="font-semibold text-white flex items-center gap-1.5">
                        <Trash2 className="w-3.5 h-3.5 text-red-400" />
                        <span>Remove the selected torrents from qBittorrent after a successful import</span>
                      </span>
                      <p className="text-[11px] text-[#8c8c8c] mt-0.5">
                        Only the torrent entries are removed from qBittorrent. Your downloaded files are never deleted. A backup is created first.
                      </p>
                    </div>
                  </label>
                </div>
              </div>

              {/* Action Buttons */}
              <div className="flex justify-between items-center pt-2">
                <span className="text-[11px] text-[#737373] font-mono">
                  Source: {scanResult.storage_format} {locationDto ? `(${locationDto.source_description})` : ''}
                </span>

                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() => setShowQbitImport(false)}
                    className="px-4 py-2 rounded bg-[#262626] hover:bg-[#333333] text-white transition font-medium"
                  >
                    Cancel
                  </button>
                  <button
                    type="button"
                    disabled={selectedHashes.size === 0}
                    onClick={handleProceedFromSelection}
                    className="px-5 py-2 rounded bg-[#e50914] hover:bg-[#b80710] disabled:opacity-40 text-white font-medium transition shadow-lg shadow-red-900/20"
                  >
                    Import {selectedHashes.size > 0 ? `(${selectedHashes.size})` : ''}
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* STEP 4a: PROCESS BLOCKER (5b STEP 1) */}
          {step === 'process_blocker' && (
            <div className="space-y-4 max-w-lg mx-auto py-6">
              <div className="p-4 rounded-lg bg-red-950/30 border border-red-800/50 flex items-start gap-3">
                <AlertTriangle className="w-6 h-6 text-red-400 flex-shrink-0 mt-0.5" />
                <div className="space-y-1.5">
                  <h3 className="text-sm font-bold text-white">
                    qBittorrent must be closed to remove torrents safely
                  </h3>
                  <p className="text-xs text-red-200/90 leading-relaxed">
                    qBittorrent holds active locks and rewrites its resume database upon closing. To prevent database corruption or resurrected torrents, please quit qBittorrent before proceeding with removal.
                  </p>
                </div>
              </div>

              <div className="flex flex-col gap-2 pt-2">
                <button
                  type="button"
                  autoFocus
                  onClick={async () => {
                    const proc = await invoke<QbitProcessCheck>('qbit_check_running');
                    if (!proc.is_running) {
                      setStep('second_confirm');
                    }
                  }}
                  className="w-full py-2.5 rounded bg-[#e50914] hover:bg-[#b80710] text-white font-medium transition"
                >
                  I closed it — re-check
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setRemoveAfterImport(false);
                    executeImport(false);
                  }}
                  className="w-full py-2 rounded bg-[#262626] hover:bg-[#333333] text-white transition font-medium"
                >
                  Import without removing
                </button>
                <button
                  type="button"
                  onClick={() => setStep('selection')}
                  className="w-full py-1.5 text-xs text-[#a3a3a3] hover:text-white transition"
                >
                  Back to selection
                </button>
              </div>
            </div>
          )}

          {/* STEP 4b: SECOND CONFIRMATION (5b STEP 2) */}
          {step === 'second_confirm' && (
            <div className="space-y-4 max-w-xl mx-auto py-6">
              <div className="p-4 rounded-lg bg-red-950/30 border border-red-700/60 flex items-start gap-3">
                <Shield className="w-6 h-6 text-red-400 flex-shrink-0 mt-0.5" />
                <div className="space-y-1.5">
                  <h3 className="text-sm font-bold text-white uppercase tracking-wider font-mono">
                    Permanent Entry Removal Warning
                  </h3>
                  <p className="text-xs text-white leading-relaxed">
                    <span className="font-bold text-red-400">{selectedHashes.size} torrents</span> will be imported into Nebula and then <span className="font-bold text-red-400">REMOVED from qBittorrent</span>. Downloaded payload files will NOT be deleted. A full backup will be verified before any deletion.
                  </p>
                </div>
              </div>

              <div className="flex justify-end gap-2 pt-4 border-t border-[#262626]">
                <button
                  type="button"
                  onClick={() => setStep('selection')}
                  className="px-4 py-2 rounded bg-[#262626] hover:bg-[#333333] text-white transition"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  autoFocus
                  onClick={() => {
                    setRemoveAfterImport(false);
                    executeImport(false);
                  }}
                  className="px-4 py-2 rounded bg-[#333333] hover:bg-[#404040] text-white font-medium transition"
                >
                  Import only
                </button>
                <button
                  type="button"
                  onClick={() => executeImport(true)}
                  className="px-5 py-2 rounded bg-red-600 hover:bg-red-700 text-white font-bold transition shadow-lg shadow-red-900/30"
                >
                  Import and remove
                </button>
              </div>
            </div>
          )}

          {/* STEP 5: IMPORTING PROGRESS */}
          {step === 'importing' && (
            <div className="py-16 flex flex-col items-center justify-center space-y-3">
              <RefreshCw className="w-8 h-8 text-[#e50914] animate-spin" />
              <p className="text-sm font-medium text-white">
                Importing selected torrents into Nebula...
              </p>
              <p className="text-xs text-[#a3a3a3]">
                Preserving save paths, mapped files, and scheduling verification queue
              </p>
            </div>
          )}

          {/* STEP 6: SUMMARY REPORT (R10) */}
          {step === 'summary' && summaryReport && (
            <div className="space-y-4 max-w-2xl mx-auto py-2">
              <div className="flex items-center gap-3 p-3 rounded-lg bg-emerald-950/20 border border-emerald-800/40">
                <CheckCircle2 className="w-6 h-6 text-emerald-400 flex-shrink-0" />
                <div>
                  <h3 className="text-sm font-bold text-white">
                    qBittorrent Import Completed
                  </h3>
                  <p className="text-xs text-emerald-300/90 font-mono">
                    {summaryReport.imported_count} imported • {summaryReport.skipped.length} skipped • {summaryReport.failed.length} failed
                    {summaryReport.removed_count > 0 ? ` • ${summaryReport.removed_count} removed from qBittorrent` : ''}
                  </p>
                </div>
              </div>

              {/* Removal stats & Backup Folder */}
              {summaryReport.backup_dir && (
                <div className="p-3 rounded-lg bg-[#1f1f1f] border border-[#333333] flex items-center justify-between">
                  <div className="space-y-0.5 truncate mr-3">
                    <span className="text-[10px] uppercase text-[#a3a3a3] font-sans block">
                      Backup Location (Manifest + Original Files)
                    </span>
                    <span className="text-white font-mono text-xs truncate block" title={summaryReport.backup_dir}>
                      {summaryReport.backup_dir}
                    </span>
                  </div>
                  <button
                    type="button"
                    onClick={handleOpenBackupFolder}
                    className="px-3 py-1.5 rounded bg-[#2a2a2a] hover:bg-[#333333] text-white flex items-center gap-1.5 transition flex-shrink-0"
                  >
                    <FolderOpen className="w-3.5 h-3.5 text-[#e50914]" />
                    <span>Open folder</span>
                  </button>
                </div>
              )}

              {/* Unsupported qBittorrent Settings Notice */}
              {summaryReport.unsupported_settings && summaryReport.unsupported_settings.length > 0 && (
                <div className="p-3 rounded-lg bg-[#1f1f1f] border border-[#333333] space-y-1.5 text-xs">
                  <span className="text-[10px] uppercase font-bold text-[#a3a3a3] block font-sans">
                    Unsupported qBittorrent Settings (Preserved or Safely Mapped)
                  </span>
                  {summaryReport.unsupported_settings.map((s, idx) => (
                    <div key={idx} className="text-[#a3a3a3] font-mono text-[11px]">
                      • {s}
                    </div>
                  ))}
                </div>
              )}

              {/* Skipped or Failed Torrents list if any */}
              {(summaryReport.skipped.length > 0 || summaryReport.failed.length > 0 || summaryReport.not_removed.length > 0) && (
                <div className="space-y-2 max-h-48 overflow-y-auto border border-[#2a2a2a] rounded p-2.5 bg-[#1a1a1a]">
                  {summaryReport.failed.map((f, i) => (
                    <div key={i} className="text-[11px] text-red-400 font-mono">
                      [FAILED] {f.name}: {f.error}
                    </div>
                  ))}
                  {summaryReport.skipped.map((s, i) => (
                    <div key={i} className="text-[11px] text-[#a3a3a3] font-mono">
                      [SKIPPED] {s.name}: {s.reason}
                    </div>
                  ))}
                  {summaryReport.not_removed.map((nr, i) => (
                    <div key={i} className="text-[11px] text-yellow-400 font-mono">
                      [NOT REMOVED] {nr.name}: {nr.reason}
                    </div>
                  ))}
                </div>
              )}

              {/* Action Buttons */}
              <div className="flex justify-between items-center pt-2 border-t border-[#262626]">
                <button
                  type="button"
                  onClick={handleCopyLog}
                  className="px-3 py-1.5 rounded bg-[#262626] hover:bg-[#333333] text-white flex items-center gap-1.5 transition"
                >
                  <Copy className="w-3.5 h-3.5 text-[#a3a3a3]" />
                  <span>{copiedLog ? 'Log Copied!' : 'Copy log'}</span>
                </button>

                <button
                  type="button"
                  autoFocus
                  onClick={() => setShowQbitImport(false)}
                  className="px-5 py-2 rounded bg-[#e50914] hover:bg-[#b80710] text-white font-medium transition"
                >
                  Done
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
