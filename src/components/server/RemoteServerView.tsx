/**
 * RemoteServerView — Manager-style torrent view for the nebulae-server remote.
 *
 * Data isolation contract:
 *  - ALL data comes from useRemoteServerStore (Tauri invoke-based polling).
 *  - The API token never enters this file or any other frontend code.
 *  - Calls go: frontend → Rust command → nebulae-server REST API.
 *  - NOTHING is read from useTorrentStore, useSettingsStore engine state,
 *    or any other local store.
 *  - Destructive actions (remove, kill switch) require confirmation.
 *
 * Feature parity vs TorrentManagerView:
 *  Columns:    Name, Size, Progress, Status, ↓ Speed, ↑ Speed, ETA, Ratio, Seeds, Peers  ✓
 *  Actions:    Pause/Resume (single), Pause all, Resume all, Remove+delete, Recheck,
 *              Reannounce, Kill switch, Per-torrent limits modal  ✓
 *  Inspector:  General info, Trackers list, Files list  ✓
 *  Gaps vs Manager (not available from nebulae-server API):
 *    - Peers tab (no /api/v1/torrents/{id}/peers endpoint on the server)
 *    - Speed graph (no history feed from server)
 *    - Rename torrent / rename file (server trait returns Err for transmission)
 *    - Sequential download toggle (same)
 *    - Category edit (write; not wired — server returns Err for transmission)
 *    - Poster hover popup (requires library DB on the local machine)
 */

import React, { useState, useMemo, useCallback } from 'react';
import {
  useRemoteServerStore,
  RemoteTorrentItem,
} from '../../stores/remoteServerStore';
import { formatBytes, formatSpeed, formatEta } from '../../lib/formatters';
import {
  HardDrive,
  RefreshCw,
  Play,
  Pause,
  Trash2,
  Zap,
  ZapOff,
  AlertCircle,
  CheckCircle2,
  Loader2,
  WifiOff,
  Lock,
  Server,
  ChevronUp,
  ChevronDown,
  RotateCw,
  Radio,
  Gauge,
} from 'lucide-react';

// ── Helpers ────────────────────────────────────────────────────────────────

function statusBadge(status: string): string {
  switch (status) {
    case 'Downloading':
      return 'bg-blue-900/60 text-blue-300 border border-blue-700/50';
    case 'Seeding':
      return 'bg-emerald-900/60 text-emerald-300 border border-emerald-700/50';
    case 'Paused':
      return 'bg-[#2a2a2a] text-[#a3a3a3] border border-[#333333]';
    case 'Completed':
      return 'bg-emerald-900/40 text-emerald-400 border border-emerald-700/40';
    case 'Checking':
    case 'Queued for checking':
      return 'bg-amber-900/60 text-amber-300 border border-amber-700/50';
    case 'Error':
      return 'bg-red-950/60 text-red-300 border border-red-700/50';
    default:
      return 'bg-[#2a2a2a] text-[#a3a3a3] border border-[#333333]';
  }
}

function progressBarColor(t: RemoteTorrentItem): string {
  if (t.status === 'Error') return 'bg-[#e50914]';
  if (t.status === 'Seeding' || t.status === 'Completed') return 'bg-emerald-500';
  if (t.status === 'Paused') return 'bg-[#555555]';
  if (t.status === 'Checking' || t.status === 'Queued for checking') return 'bg-amber-500';
  return 'bg-white';
}

type SortField = 'name' | 'size' | 'progress' | 'status' | 'down' | 'up' | 'eta' | 'ratio' | 'seeds' | 'peers';

function sortTorrents(
  ts: RemoteTorrentItem[],
  field: SortField,
  dir: 'asc' | 'desc',
): RemoteTorrentItem[] {
  return [...ts].sort((a, b) => {
    let cmp = 0;
    switch (field) {
      case 'name':    cmp = a.name.localeCompare(b.name); break;
      case 'size':    cmp = a.total_bytes - b.total_bytes; break;
      case 'progress':cmp = a.progress - b.progress; break;
      case 'status':  cmp = a.status.localeCompare(b.status); break;
      case 'down':    cmp = a.download_rate - b.download_rate; break;
      case 'up':      cmp = a.upload_rate - b.upload_rate; break;
      case 'eta':     cmp = (a.eta_seconds ?? Infinity) - (b.eta_seconds ?? Infinity); break;
      case 'ratio':   cmp = a.ratio - b.ratio; break;
      case 'seeds':   cmp = a.seeds - b.seeds; break;
      case 'peers':   cmp = a.peers - b.peers; break;
    }
    return dir === 'asc' ? cmp : -cmp;
  });
}

function formatDate(ts: number): string {
  return new Date(ts * 1000).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
}

// ── Modals ────────────────────────────────────────────────────────────────

interface RemoveModalProps {
  torrent: RemoteTorrentItem;
  onConfirm: (deleteFiles: boolean) => void;
  onCancel: () => void;
}
const RemoveModal: React.FC<RemoveModalProps> = ({ torrent, onConfirm, onCancel }) => (
  <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70">
    <div className="bg-[#1f1f1f] border border-[#333333] rounded-lg shadow-2xl w-96 p-5 space-y-4">
      <div className="flex items-center gap-2">
        <AlertCircle className="w-5 h-5 text-[#e50914] flex-shrink-0" />
        <span className="font-semibold text-sm text-white">Remove from remote server</span>
      </div>
      <p className="text-xs text-[#a3a3a3]">
        <span className="text-white font-medium">{torrent.name}</span> will be removed from the
        remote server. Delete downloaded files too?
      </p>
      <div className="flex gap-2 justify-end pt-1">
        <button onClick={onCancel} className="px-3 py-1.5 rounded border border-[#333333] text-[#a3a3a3] hover:text-white text-xs transition">
          Cancel
        </button>
        <button onClick={() => onConfirm(false)} className="px-3 py-1.5 rounded border border-[#444444] bg-[#2a2a2a] hover:bg-[#333333] text-white text-xs transition">
          Remove only
        </button>
        <button onClick={() => onConfirm(true)} className="px-3 py-1.5 rounded bg-[#e50914] hover:bg-[#c40812] text-white text-xs font-medium transition">
          Remove + Delete files
        </button>
      </div>
    </div>
  </div>
);

interface KillSwitchModalProps {
  onConfirm: () => void;
  onCancel: () => void;
}
const KillSwitchModal: React.FC<KillSwitchModalProps> = ({ onConfirm, onCancel }) => (
  <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70">
    <div className="bg-[#1f1f1f] border border-[#333333] rounded-lg shadow-2xl w-96 p-5 space-y-4">
      <div className="flex items-center gap-2">
        <Zap className="w-5 h-5 text-amber-400 flex-shrink-0" />
        <span className="font-semibold text-sm text-white">Engage kill switch on remote server</span>
      </div>
      <p className="text-xs text-[#a3a3a3]">
        This will block all torrent traffic on the remote server until you reset it.
        All active transfers will pause immediately.
      </p>
      <div className="flex gap-2 justify-end pt-1">
        <button onClick={onCancel} className="px-3 py-1.5 rounded border border-[#333333] text-[#a3a3a3] hover:text-white text-xs transition">
          Cancel
        </button>
        <button onClick={onConfirm} className="px-3 py-1.5 rounded bg-amber-600 hover:bg-amber-500 text-white text-xs font-medium transition">
          Engage kill switch
        </button>
      </div>
    </div>
  </div>
);

interface LimitsModalProps {
  torrent: RemoteTorrentItem;
  onConfirm: (dlKib: number, ulKib: number, ratio: number) => void;
  onCancel: () => void;
}
const LimitsModal: React.FC<LimitsModalProps> = ({ torrent, onConfirm, onCancel }) => {
  const [dlKib, setDlKib] = useState(torrent.download_limit_kib.toString());
  const [ulKib, setUlKib] = useState(torrent.upload_limit_kib.toString());
  const [ratio, setRatio] = useState('0');
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70">
      <div className="bg-[#1f1f1f] border border-[#333333] rounded-lg shadow-2xl w-80 p-5 space-y-4">
        <div className="flex items-center gap-2">
          <Gauge className="w-4 h-4 text-[#a3a3a3]" />
          <span className="font-semibold text-sm text-white">Speed limits — remote</span>
        </div>
        <p className="text-[11px] text-[#a3a3a3] truncate">{torrent.name}</p>
        <div className="space-y-2 font-mono text-xs">
          {[
            { label: 'Download limit (KiB/s, 0 = unlimited)', val: dlKib, set: setDlKib },
            { label: 'Upload limit (KiB/s, 0 = unlimited)', val: ulKib, set: setUlKib },
            { label: 'Ratio limit (0 = unlimited)', val: ratio, set: setRatio },
          ].map(({ label, val, set }) => (
            <div key={label}>
              <label className="block text-[10px] text-[#a3a3a3] mb-0.5 font-sans">{label}</label>
              <input
                type="number"
                min="0"
                value={val}
                onChange={(e) => set(e.target.value)}
                className="w-full px-2 py-1 rounded border border-[#333333] bg-[#141414] text-white text-xs focus:border-[#e50914] focus:outline-none"
              />
            </div>
          ))}
        </div>
        <div className="flex gap-2 justify-end pt-1">
          <button onClick={onCancel} className="px-3 py-1.5 rounded border border-[#333333] text-[#a3a3a3] hover:text-white text-xs transition">
            Cancel
          </button>
          <button
            onClick={() => onConfirm(parseInt(dlKib) || 0, parseInt(ulKib) || 0, parseFloat(ratio) || 0)}
            className="px-3 py-1.5 rounded bg-[#e50914] hover:bg-[#c40812] text-white text-xs font-medium transition"
          >
            Apply
          </button>
        </div>
      </div>
    </div>
  );
};

// ── Inspector ─────────────────────────────────────────────────────────────

interface InspectorProps {
  torrent: RemoteTorrentItem;
}
type InspectorTab = 'general' | 'trackers' | 'files';

const Inspector: React.FC<InspectorProps> = ({ torrent }) => {
  const [tab, setTab] = useState<InspectorTab>('general');
  const pct = Math.round(torrent.progress * 100);

  return (
    <div className="flex flex-col h-full">
      {/* Inspector tab bar */}
      <div className="flex border-b border-[#262626] px-3 pt-1 gap-1 text-xs bg-[#1a1a1a] flex-shrink-0">
        {(['general', 'trackers', 'files'] as InspectorTab[]).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={`px-3 py-1.5 rounded-t font-medium capitalize transition ${
              tab === t
                ? 'border-b-2 border-[#e50914] text-white bg-[#141414]'
                : 'text-[#a3a3a3] hover:text-white'
            }`}
          >
            {t === 'general' ? 'General' : t === 'trackers' ? `Trackers (${torrent.trackers.length})` : `Files (${torrent.files.length})`}
          </button>
        ))}
        <span className="ml-2 text-[10px] text-[#555555] font-mono self-center">[remote]</span>
      </div>

      {/* Inspector content */}
      <div className="flex-1 p-3 overflow-y-auto text-xs bg-[#141414]">
        {tab === 'general' && (
          <div className="grid grid-cols-2 gap-x-4 gap-y-1 font-mono text-[11px]">
            <span className="text-[#a3a3a3]">Status</span>
            <span className={`px-1.5 py-0.5 rounded text-[10px] font-semibold uppercase w-fit ${statusBadge(torrent.status)}`}>
              {torrent.status}
            </span>
            <span className="text-[#a3a3a3]">Progress</span>
            <span className="text-white">{pct}%</span>
            <span className="text-[#a3a3a3]">Size</span>
            <span className="text-white">{formatBytes(torrent.total_bytes)}</span>
            <span className="text-[#a3a3a3]">Downloaded</span>
            <span className="text-white">{formatBytes(torrent.downloaded_bytes)}</span>
            <span className="text-[#a3a3a3]">Uploaded</span>
            <span className="text-white">{formatBytes(torrent.uploaded_bytes)}</span>
            <span className="text-[#a3a3a3]">Ratio</span>
            <span className="text-white">{torrent.ratio.toFixed(3)}</span>
            <span className="text-[#a3a3a3]">↓ Rate</span>
            <span className="text-[#06B6D4]">{formatSpeed(torrent.download_rate)}</span>
            <span className="text-[#a3a3a3]">↑ Rate</span>
            <span className="text-[#10B981]">{formatSpeed(torrent.upload_rate)}</span>
            <span className="text-[#a3a3a3]">ETA</span>
            <span className="text-white">{torrent.eta_seconds != null && torrent.eta_seconds > 0 ? formatEta(torrent.eta_seconds) : '—'}</span>
            <span className="text-[#a3a3a3]">Seeds / Peers</span>
            <span className="text-white">{torrent.seeds} / {torrent.peers}</span>
            <span className="text-[#a3a3a3]">↓ Limit</span>
            <span className="text-white">{torrent.download_limit_kib ? `${torrent.download_limit_kib} KiB/s` : 'unlimited'}</span>
            <span className="text-[#a3a3a3]">↑ Limit</span>
            <span className="text-white">{torrent.upload_limit_kib ? `${torrent.upload_limit_kib} KiB/s` : 'unlimited'}</span>
            <span className="text-[#a3a3a3]">Save path</span>
            <span className="text-white truncate font-mono" title={torrent.save_path}>{torrent.save_path}</span>
            <span className="text-[#a3a3a3]">Info hash</span>
            <span className="text-[#777] font-mono text-[10px] truncate">{torrent.info_hash}</span>
            <span className="text-[#a3a3a3]">Category</span>
            <span className="text-white">{torrent.category || '—'}</span>
            <span className="text-[#a3a3a3]">Tags</span>
            <span className="text-white">{torrent.tags.length ? torrent.tags.join(', ') : '—'}</span>
            <span className="text-[#a3a3a3]">Added</span>
            <span className="text-white">{formatDate(torrent.added_on)}</span>
            {torrent.completed_on && (
              <>
                <span className="text-[#a3a3a3]">Completed</span>
                <span className="text-white">{formatDate(torrent.completed_on)}</span>
              </>
            )}
            <span className="text-[#a3a3a3]">Created by</span>
            <span className="text-white truncate">{torrent.created_by || '—'}</span>
            {torrent.comment && (
              <>
                <span className="text-[#a3a3a3]">Comment</span>
                <span className="text-white truncate">{torrent.comment}</span>
              </>
            )}
            <span className="text-[#a3a3a3]">Piece size</span>
            <span className="text-white">{formatBytes(torrent.piece_size)}</span>
            <span className="text-[#a3a3a3]">Pieces</span>
            <span className="text-white">{torrent.total_pieces.toLocaleString()}</span>
          </div>
        )}

        {tab === 'trackers' && (
          <div className="space-y-1">
            {torrent.trackers.length === 0 ? (
              <p className="text-[#a3a3a3] text-xs">No trackers (DHT/PEX only)</p>
            ) : (
              torrent.trackers.map((tr, i) => (
                <div key={i} className="flex items-start gap-2 text-[11px] font-mono py-0.5 border-b border-[#1f1f1f] last:border-0">
                  <span className={`flex-shrink-0 font-semibold ${
                    tr.status === 'Working' ? 'text-emerald-400' :
                    tr.status === 'Not contacted yet' ? 'text-[#555]' : 'text-red-400'
                  }`}>
                    {tr.status}
                  </span>
                  <span className="text-[#a3a3a3] truncate flex-1 text-[10px]">{tr.url}</span>
                  {tr.seeds > 0 && <span className="text-[#a3a3a3] flex-shrink-0 text-[10px]">{tr.seeds}S</span>}
                  {tr.peers > 0 && <span className="text-[#a3a3a3] flex-shrink-0 text-[10px]">{tr.peers}P</span>}
                  {tr.message && <span className="text-amber-400 flex-shrink-0 text-[10px] truncate max-w-[120px]" title={tr.message}>{tr.message}</span>}
                </div>
              ))
            )}
          </div>
        )}

        {tab === 'files' && (
          <div className="space-y-0.5">
            {torrent.files.length === 0 ? (
              <p className="text-[#a3a3a3] text-xs">No file list available</p>
            ) : (
              torrent.files.map((f) => (
                <div key={f.index} className="flex items-center gap-2 text-[11px] font-mono py-0.5">
                  <div className="w-8 h-1.5 bg-[#333] rounded-full flex-shrink-0 overflow-hidden">
                    <div className="h-full bg-white rounded-full" style={{ width: `${Math.round(f.progress * 100)}%` }} />
                  </div>
                  <span className="text-[#a3a3a3] w-7 text-right flex-shrink-0">{Math.round(f.progress * 100)}%</span>
                  <span className="text-white truncate flex-1" title={f.name}>{f.name}</span>
                  <span className="text-[#a3a3a3] flex-shrink-0 text-[10px]">{formatBytes(f.size_bytes)}</span>
                </div>
              ))
            )}
          </div>
        )}
      </div>
    </div>
  );
};

// ── Status banner ─────────────────────────────────────────────────────────

interface StatusBannerProps {
  onRefresh: () => void;
  isRefreshing: boolean;
  openSettings: () => void;
}
const StatusBanner: React.FC<StatusBannerProps> = ({ onRefresh, isRefreshing, openSettings }) => {
  const { connState, errorMessage, serverStatus, lastSuccessAt } = useRemoteServerStore();

  if (connState === 'connected' && serverStatus) {
    const ago = lastSuccessAt ? Math.round((Date.now() - lastSuccessAt) / 1000) : 0;
    return (
      <div className="flex items-center gap-2 px-3 py-1 bg-emerald-950/40 border-b border-emerald-800/30 text-[11px] flex-shrink-0 overflow-hidden">
        <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400 flex-shrink-0" />
        <span className="text-emerald-300 font-medium flex-shrink-0">Connected</span>
        <span className="text-[#a3a3a3] truncate">
          {serverStatus.name} {serverStatus.version} · engine: {serverStatus.engine}
        </span>
        {serverStatus.kill_switch_engaged && (
          <span className="flex-shrink-0 px-1.5 py-0.5 rounded bg-amber-900/60 border border-amber-700/50 text-amber-300 text-[10px] font-semibold uppercase">
            Kill switch active
          </span>
        )}
        <span className="ml-auto text-[#555] flex-shrink-0 tabular-nums">
          {ago < 10 ? 'just now' : `${ago}s ago`}
        </span>
        <button
          onClick={onRefresh}
          disabled={isRefreshing}
          className="p-0.5 rounded hover:bg-white/10 text-[#a3a3a3] hover:text-white transition disabled:opacity-40 flex-shrink-0"
          title="Refresh now"
        >
          <RefreshCw className={`w-3 h-3 ${isRefreshing ? 'animate-spin' : ''}`} />
        </button>
      </div>
    );
  }

  if (connState === 'connecting') {
    return (
      <div className="flex items-center gap-2 px-3 py-1 bg-[#1a1a1a] border-b border-[#262626] text-[11px] flex-shrink-0">
        <Loader2 className="w-3.5 h-3.5 text-[#a3a3a3] animate-spin flex-shrink-0" />
        <span className="text-[#a3a3a3]">Connecting to remote server…</span>
      </div>
    );
  }

  if (connState === 'auth_error') {
    return (
      <div className="flex items-center gap-2 px-3 py-1 bg-red-950/40 border-b border-red-900/40 text-[11px] flex-shrink-0">
        <Lock className="w-3.5 h-3.5 text-red-400 flex-shrink-0" />
        <span className="text-red-300 font-medium flex-shrink-0">Authentication failed</span>
        <span className="text-[#a3a3a3] truncate">{errorMessage}</span>
        <button
          onClick={openSettings}
          className="ml-auto px-2 py-0.5 rounded bg-[#2a2a2a] border border-[#333333] text-white text-[10px] hover:bg-[#333333] transition flex-shrink-0"
        >
          Open Settings
        </button>
      </div>
    );
  }

  if (connState === 'error') {
    return (
      <div className="flex items-center gap-2 px-3 py-1 bg-red-950/30 border-b border-red-900/30 text-[11px] flex-shrink-0">
        <WifiOff className="w-3.5 h-3.5 text-red-400 flex-shrink-0" />
        <span className="text-red-300 font-medium flex-shrink-0">Disconnected</span>
        <span className="text-[#a3a3a3] truncate">{errorMessage}</span>
        <button
          onClick={onRefresh}
          disabled={isRefreshing}
          className="ml-auto px-2 py-0.5 rounded bg-[#2a2a2a] border border-[#333333] text-white text-[10px] hover:bg-[#333333] transition disabled:opacity-40 flex-shrink-0"
        >
          Retry
        </button>
      </div>
    );
  }

  return null;
};

// ── Main view ─────────────────────────────────────────────────────────────

export const RemoteServerView: React.FC = () => {
  const {
    connState,
    torrents,
    selectedId,
    serverStatus,
    selectTorrent,
    refresh,
    pauseTorrent,
    resumeTorrent,
    pauseAll,
    resumeAll,
    removeTorrent,
    triggerKillSwitch,
    resetKillSwitch,
    reannounce,
    recheck,
    setLimits,
  } = useRemoteServerStore();

  const [sortField, setSortField] = useState<SortField>('name');
  const [sortDir, setSortDir] = useState<'asc' | 'desc'>('asc');
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [removeTarget, setRemoveTarget] = useState<RemoteTorrentItem | null>(null);
  const [showKillSwitchModal, setShowKillSwitchModal] = useState(false);
  const [limitsTarget, setLimitsTarget] = useState<RemoteTorrentItem | null>(null);
  const INSPECTOR_HEIGHT = 240;

  const openSettingsModal = useCallback(() => {
    window.dispatchEvent(new CustomEvent('nebulae:open-settings', { detail: { tab: 'rtorrent' } }));
  }, []);

  const selectedTorrent = useMemo(
    () => torrents.find((t) => t.id === selectedId) ?? null,
    [torrents, selectedId],
  );

  const sorted = useMemo(() => sortTorrents(torrents, sortField, sortDir), [torrents, sortField, sortDir]);

  const handleSort = useCallback(
    (field: SortField) => {
      if (sortField === field) setSortDir((d) => (d === 'asc' ? 'desc' : 'asc'));
      else { setSortField(field); setSortDir('asc'); }
    },
    [sortField],
  );

  const handleRefresh = useCallback(async () => {
    setIsRefreshing(true);
    await refresh();
    setIsRefreshing(false);
  }, [refresh]);

  const doAction = useCallback(
    async (fn: () => Promise<string | null>, label: string) => {
      setActionError(null);
      const err = await fn();
      if (err) setActionError(`${label}: ${err}`);
    },
    [],
  );

  const SortIcon = ({ field }: { field: SortField }) => {
    if (sortField !== field) return null;
    return sortDir === 'asc' ? <ChevronUp className="w-3 h-3 inline ml-0.5" /> : <ChevronDown className="w-3 h-3 inline ml-0.5" />;
  };

  const showEmptyNotConfigured = connState === 'disconnected';
  const showConnecting = connState === 'connecting' && torrents.length === 0;

  return (
    <div className="flex flex-col h-full bg-[#0d0d0d] overflow-hidden">
      {/* Status banner */}
      <StatusBanner onRefresh={handleRefresh} isRefreshing={isRefreshing} openSettings={openSettingsModal} />

      {/* Toolbar */}
      <div className="flex items-center gap-1 px-3 h-10 border-b border-[#262626] bg-[#141414] flex-shrink-0 overflow-x-auto">
        {/* Single-torrent actions */}
        <button
          disabled={!selectedTorrent}
          onClick={() =>
            selectedTorrent &&
            doAction(
              () => selectedTorrent.status === 'Paused'
                ? resumeTorrent(selectedTorrent.id)
                : pauseTorrent(selectedTorrent.id),
              selectedTorrent.status === 'Paused' ? 'Resume' : 'Pause',
            )
          }
          className="flex items-center gap-1.5 px-2.5 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium transition disabled:opacity-40 disabled:cursor-not-allowed flex-shrink-0"
          title={selectedTorrent?.status === 'Paused' ? 'Resume' : 'Pause'}
        >
          {selectedTorrent?.status === 'Paused' ? <Play className="w-3.5 h-3.5 fill-current" /> : <Pause className="w-3.5 h-3.5" />}
          <span className="hidden sm:inline">{selectedTorrent?.status === 'Paused' ? 'Resume' : 'Pause'}</span>
        </button>

        <button
          disabled={!selectedTorrent}
          onClick={() => selectedTorrent && setRemoveTarget(selectedTorrent)}
          className="flex items-center gap-1.5 px-2.5 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium transition disabled:opacity-40 disabled:cursor-not-allowed flex-shrink-0"
          title="Remove"
        >
          <Trash2 className="w-3.5 h-3.5 text-[#e50914]" />
          <span className="hidden sm:inline">Remove</span>
        </button>

        <button
          disabled={!selectedTorrent}
          onClick={() => selectedTorrent && doAction(() => recheck(selectedTorrent.id), 'Recheck')}
          className="flex items-center gap-1.5 px-2.5 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium transition disabled:opacity-40 disabled:cursor-not-allowed flex-shrink-0"
          title="Force recheck"
        >
          <RotateCw className="w-3.5 h-3.5" />
          <span className="hidden md:inline">Recheck</span>
        </button>

        <button
          disabled={!selectedTorrent}
          onClick={() => selectedTorrent && doAction(() => reannounce(selectedTorrent.id), 'Reannounce')}
          className="flex items-center gap-1.5 px-2.5 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium transition disabled:opacity-40 disabled:cursor-not-allowed flex-shrink-0"
          title="Force reannounce"
        >
          <Radio className="w-3.5 h-3.5" />
          <span className="hidden md:inline">Reannounce</span>
        </button>

        <button
          disabled={!selectedTorrent}
          onClick={() => selectedTorrent && setLimitsTarget(selectedTorrent)}
          className="flex items-center gap-1.5 px-2.5 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium transition disabled:opacity-40 disabled:cursor-not-allowed flex-shrink-0"
          title="Speed limits"
        >
          <Gauge className="w-3.5 h-3.5" />
          <span className="hidden lg:inline">Limits</span>
        </button>

        <div className="h-5 w-px bg-[#333333] mx-0.5 flex-shrink-0" />

        {/* Global actions */}
        <button
          onClick={() => doAction(pauseAll, 'Pause all')}
          className="flex items-center gap-1 px-2.5 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium transition flex-shrink-0"
          title="Pause all remote"
        >
          <Pause className="w-3.5 h-3.5" />
          <span className="hidden lg:inline">Pause all</span>
        </button>

        <button
          onClick={() => doAction(resumeAll, 'Resume all')}
          className="flex items-center gap-1 px-2.5 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium transition flex-shrink-0"
          title="Resume all remote"
        >
          <Play className="w-3.5 h-3.5 fill-current" />
          <span className="hidden lg:inline">Resume all</span>
        </button>

        <div className="h-5 w-px bg-[#333333] mx-0.5 flex-shrink-0" />

        {/* Kill switch */}
        {serverStatus?.kill_switch_engaged ? (
          <button
            onClick={() => doAction(resetKillSwitch, 'Reset kill switch')}
            className="flex items-center gap-1 px-2.5 py-1.5 rounded bg-amber-900/60 hover:bg-amber-800/60 border border-amber-700/50 text-amber-300 text-xs font-medium transition flex-shrink-0"
            title="Reset remote kill switch"
          >
            <ZapOff className="w-3.5 h-3.5" />
            <span className="hidden xl:inline">Reset Kill Switch</span>
          </button>
        ) : (
          <button
            onClick={() => setShowKillSwitchModal(true)}
            className="flex items-center gap-1 px-2.5 py-1.5 rounded bg-[#1f1f1f] hover:bg-amber-950/40 border border-[#333333] hover:border-amber-700/50 text-[#a3a3a3] hover:text-amber-300 text-xs font-medium transition flex-shrink-0"
            title="Engage remote kill switch"
          >
            <Zap className="w-3.5 h-3.5" />
            <span className="hidden xl:inline">Kill Switch</span>
          </button>
        )}

        {/* Error display */}
        {actionError && (
          <div className="flex items-center gap-1.5 ml-2 text-[11px] text-red-400 overflow-hidden">
            <AlertCircle className="w-3.5 h-3.5 flex-shrink-0" />
            <span className="truncate max-w-[180px]">{actionError}</span>
            <button onClick={() => setActionError(null)} className="text-[#a3a3a3] hover:text-white ml-0.5 flex-shrink-0">×</button>
          </div>
        )}

        {/* Remote badge */}
        <div className="ml-auto flex items-center gap-1 flex-shrink-0">
          <Server className="w-3.5 h-3.5 text-[#555]" />
          <span className="text-[11px] text-[#555] font-mono hidden xl:inline">Remote</span>
        </div>
      </div>

      {/* Main content */}
      {showEmptyNotConfigured ? (
        <div className="flex-1 flex items-center justify-center">
          <div className="flex flex-col items-center gap-3 text-center px-6">
            <WifiOff className="w-10 h-10 text-[#333]" />
            <p className="text-sm font-medium text-white">No remote server configured</p>
            <p className="text-xs text-[#a3a3a3]">
              Enable the remote server connection in{' '}
              <button className="text-[#e50914] hover:underline" onClick={openSettingsModal}>
                Settings → Server
              </button>
              .
            </p>
          </div>
        </div>
      ) : showConnecting ? (
        <div className="flex-1 flex items-center justify-center gap-2 text-[#a3a3a3]">
          <Loader2 className="w-5 h-5 animate-spin" />
          <span className="text-sm">Connecting…</span>
        </div>
      ) : (
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* Table */}
          <div className="flex-1 overflow-auto min-h-0">
            <table className="w-full min-w-[640px] table-fixed text-left border-collapse text-xs">
              <colgroup>
                <col /><col className="w-16" /><col className="w-32" /><col className="w-20" />
                <col className="w-20" /><col className="w-20" /><col className="w-14" />
                <col className="w-14" /><col className="w-12" /><col className="w-12" />
              </colgroup>
              <thead className="sticky top-0 bg-[#1f1f1f] border-b border-[#262626] text-[11px] font-semibold text-[#a3a3a3] uppercase font-mono tracking-wider z-10">
                <tr>
                  {([ ['name','Name'], ['size','Size'], ['progress','Progress'], ['status','Status'],
                      ['down','↓ Speed'], ['up','↑ Speed'], ['eta','ETA'], ['ratio','Ratio'],
                      ['seeds','Seeds'], ['peers','Peers'] ] as [SortField, string][]).map(([f, label]) => (
                    <th
                      key={f}
                      onClick={() => handleSort(f)}
                      className={`py-2 px-2 cursor-pointer hover:text-white select-none ${f === 'name' ? 'text-left px-3' : 'text-right'}`}
                    >
                      {label}<SortIcon field={f} />
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody className="divide-y divide-[#1a1a1a] font-mono">
                {sorted.length === 0 && (
                  <tr>
                    <td colSpan={10} className="py-20 text-center">
                      <div className="flex flex-col items-center gap-2">
                        <HardDrive className="w-8 h-8 text-[#333]" />
                        <p className="text-sm font-sans text-white">No torrents on remote server</p>
                      </div>
                    </td>
                  </tr>
                )}
                {sorted.map((t) => {
                  const isSelected = selectedId === t.id;
                  const pct = Math.round(t.progress * 100);
                  const isError = t.status === 'Error';
                  return (
                    <tr
                      key={t.id}
                      onClick={() => selectTorrent(isSelected ? null : t.id)}
                      className={`cursor-pointer transition select-none ${
                        isSelected ? 'bg-[#1f1f1f] border-l-2 border-[#e50914] text-white'
                        : isError ? 'hover:bg-[#1a1a1a] text-[#e50914]'
                        : 'hover:bg-[#1a1a1a] text-[#a3a3a3] hover:text-white'
                      }`}
                    >
                      <td className={`py-2 px-3 font-sans font-medium truncate ${isError ? 'text-[#e50914]' : 'text-white'}`}>
                        <span className="truncate block" title={t.name}>{t.name}</span>
                      </td>
                      <td className="py-2 px-2 text-right tabular-nums">{formatBytes(t.total_bytes)}</td>
                      <td className="py-2 px-2 overflow-hidden">
                        <div className="flex items-center gap-1.5">
                          <div className="flex-1 h-1.5 rounded-full bg-[#333] overflow-hidden">
                            <div className={`h-full rounded-full ${progressBarColor(t)}`} style={{ width: `${pct}%` }} />
                          </div>
                          <span className={`text-[10px] w-7 text-right flex-shrink-0 tabular-nums ${isError ? 'text-[#e50914]' : 'text-white'}`}>{pct}%</span>
                        </div>
                      </td>
                      <td className="py-2 px-2 text-right">
                        <span className={`px-1 py-0.5 rounded text-[9px] font-semibold uppercase ${statusBadge(t.status)}`}>
                          {t.status.length > 9 ? t.status.slice(0, 8) + '…' : t.status}
                        </span>
                      </td>
                      <td className="py-2 px-2 text-right tabular-nums text-[#06B6D4]">{t.download_rate > 0 ? formatSpeed(t.download_rate) : '—'}</td>
                      <td className="py-2 px-2 text-right tabular-nums text-[#10B981]">{t.upload_rate > 0 ? formatSpeed(t.upload_rate) : '—'}</td>
                      <td className="py-2 px-2 text-right tabular-nums">{t.eta_seconds != null && t.eta_seconds > 0 ? formatEta(t.eta_seconds) : '—'}</td>
                      <td className="py-2 px-2 text-right tabular-nums">{t.ratio.toFixed(2)}</td>
                      <td className="py-2 px-2 text-right tabular-nums">{t.seeds}</td>
                      <td className="py-2 px-2 text-right tabular-nums">{t.peers}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>

          {/* Inspector */}
          {selectedTorrent && (
            <div
              className="border-t border-[#262626] bg-[#141414] flex flex-col flex-shrink-0"
              style={{ height: INSPECTOR_HEIGHT }}
            >
              <Inspector torrent={selectedTorrent} />
            </div>
          )}
        </div>
      )}

      {/* Modals */}
      {removeTarget && (
        <RemoveModal
          torrent={removeTarget}
          onCancel={() => setRemoveTarget(null)}
          onConfirm={(deleteFiles) => {
            void doAction(() => removeTorrent(removeTarget.id, deleteFiles), 'Remove');
            setRemoveTarget(null);
          }}
        />
      )}
      {showKillSwitchModal && (
        <KillSwitchModal
          onCancel={() => setShowKillSwitchModal(false)}
          onConfirm={() => {
            void doAction(triggerKillSwitch, 'Kill switch');
            setShowKillSwitchModal(false);
          }}
        />
      )}
      {limitsTarget && (
        <LimitsModal
          torrent={limitsTarget}
          onCancel={() => setLimitsTarget(null)}
          onConfirm={(dlKib, ulKib, ratio) => {
            void doAction(() => setLimits(limitsTarget.id, dlKib, ulKib, ratio), 'Set limits');
            setLimitsTarget(null);
          }}
        />
      )}
    </div>
  );
};

export default RemoteServerView;
