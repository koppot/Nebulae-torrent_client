import React, { useState, useEffect, useMemo, useRef, useCallback } from 'react';
import { Settings, ChevronDown, ChevronUp, ChevronsUpDown, Loader2, AlertCircle, RotateCw } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-shell';
import {
  PeerTelemetry, PeeringSettings, PeeringMode, LocalityTier,
  SwarmMember, SwarmMembersResult, TorrentTrackerItem,
} from '../../types';
import { formatSpeed } from '../../lib/formatters';
import { getCachedMembers, setCachedMembers } from '../../utils/swarmMembersCache';
import { useTrackerStore } from '../../stores/trackerStore';
import { useUiStore } from '../../stores/uiStore';

// ── Locality chip ────────────────────────────────────────────────────────────

const LOCALITY_COLORS: Record<LocalityTier, string> = {
  Lan: 'bg-emerald-500/20 text-emerald-400 border-emerald-500/30',
  Asn: 'bg-cyan-500/20 text-cyan-400 border-cyan-500/30',
  Region: 'bg-blue-500/20 text-blue-400 border-blue-500/30',
  Country: 'bg-violet-500/20 text-violet-400 border-violet-500/30',
  Global: 'bg-[#1f1f1f] text-[#a3a3a3] border-[#333333]',
};

const LocalityChip: React.FC<{ tier: LocalityTier }> = ({ tier }) => (
  <span
    className={`inline-flex items-center px-1.5 py-0.5 rounded text-[9px] font-mono font-semibold border flex-shrink-0 ${LOCALITY_COLORS[tier]}`}
  >
    {tier.toUpperCase()}
  </span>
);

// ── Quality bar ──────────────────────────────────────────────────────────────

const QualityBar: React.FC<{ score: number }> = ({ score }) => {
  const pct = Math.round(Math.min(1, Math.max(0, score)) * 100);
  const color =
    pct >= 70 ? '#10b981' : pct >= 40 ? '#f59e0b' : '#e50914';
  return (
    <div className="flex items-center gap-1.5 min-w-[72px]">
      <div className="flex-1 h-1.5 rounded-full bg-[#333333] overflow-hidden">
        <div
          className="h-full rounded-full transition-[width] duration-500"
          style={{ width: `${pct}%`, backgroundColor: color }}
        />
      </div>
      <span className="text-[10px] font-mono text-[#a3a3a3] w-7 text-right">{pct}%</span>
    </div>
  );
};

// ── Protocol flags ───────────────────────────────────────────────────────────

const Flag: React.FC<{ label: string; active: boolean }> = ({ label, active }) => (
  <span
    className={`text-[9px] font-mono px-1 py-0.5 rounded border select-none ${
      active
        ? 'bg-[#1a2e1a] text-emerald-400 border-emerald-700/40'
        : 'text-[#444] border-[#2a2a2a] bg-[#141414]'
    }`}
  >
    {label}
  </span>
);

// ── Sort ─────────────────────────────────────────────────────────────────────

type SortKey =
  | 'client_name'
  | 'ip'
  | 'locality_tier'
  | 'country_code'
  | 'rtt_ms'
  | 'sustained_dl_bps'
  | 'sustained_ul_bps'
  | 'stability_score';

const LOCALITY_ORDER: Record<LocalityTier, number> = {
  Lan: 0,
  Asn: 1,
  Region: 2,
  Country: 3,
  Global: 4,
};

function sortPeers(peers: PeerTelemetry[], key: SortKey, asc: boolean): PeerTelemetry[] {
  return [...peers].sort((a, b) => {
    let cmp = 0;
    switch (key) {
      case 'client_name':
        cmp = a.client_name.localeCompare(b.client_name);
        break;
      case 'ip':
        cmp = a.ip.localeCompare(b.ip);
        break;
      case 'locality_tier':
        cmp = LOCALITY_ORDER[a.locality_tier] - LOCALITY_ORDER[b.locality_tier];
        break;
      case 'country_code':
        cmp = (a.country_code ?? '').localeCompare(b.country_code ?? '');
        break;
      case 'rtt_ms':
        cmp = a.rtt_ms - b.rtt_ms;
        break;
      case 'sustained_dl_bps':
        cmp = a.sustained_dl_bps - b.sustained_dl_bps;
        break;
      case 'sustained_ul_bps':
        cmp = a.sustained_ul_bps - b.sustained_ul_bps;
        break;
      case 'stability_score':
        cmp = a.stability_score - b.stability_score;
        break;
    }
    return asc ? cmp : -cmp;
  });
}

// ── Column header ────────────────────────────────────────────────────────────

const ColHeader: React.FC<{
  label: string;
  sortKey: SortKey;
  current: SortKey;
  asc: boolean;
  onSort: (k: SortKey) => void;
}> = ({ label, sortKey, current, asc, onSort }) => {
  const active = current === sortKey;
  const Icon = active ? (asc ? ChevronUp : ChevronDown) : ChevronsUpDown;
  return (
    <th
      className="py-1.5 px-2 cursor-pointer select-none whitespace-nowrap"
      onClick={() => onSort(sortKey)}
    >
      <span className="flex items-center gap-1 text-[10px] uppercase font-mono text-[#a3a3a3] hover:text-white transition">
        {label}
        <Icon className={`w-3 h-3 flex-shrink-0 ${active ? 'text-[#e50914]' : 'opacity-40'}`} />
      </span>
    </th>
  );
};

// ── Peering Settings panel ───────────────────────────────────────────────────

const MODES: { value: PeeringMode; label: string; desc: string }[] = [
  { value: 'Balanced', label: 'Balanced', desc: 'Equal weight on speed, locality, and stability' },
  { value: 'SpeedFirst', label: 'Speed First', desc: 'Prioritise highest-throughput peers' },
  { value: 'LatencyFirst', label: 'Latency First', desc: 'Prefer low-RTT, geographically close peers' },
  { value: 'LocalFirst', label: 'Local First', desc: 'Strongly prefer LAN / same-ASN peers' },
  { value: 'PrivacyFirst', label: 'Privacy First', desc: 'Disable RTT probing, avoid geo-tagging' },
];

const PeeringSettingsPanel: React.FC<{
  settings: PeeringSettings;
  onSave: (s: PeeringSettings) => Promise<void>;
}> = ({ settings, onSave }) => {
  const [draft, setDraft] = useState<PeeringSettings>({ ...settings });
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  // Sync if parent settings change (e.g. fresh fetch)
  useEffect(() => {
    setDraft({ ...settings });
  }, [settings]);

  const isDirty = JSON.stringify(draft) !== JSON.stringify(settings);

  const handleSave = async () => {
    setSaving(true);
    try {
      await onSave(draft);
      setSaved(true);
      setTimeout(() => setSaved(false), 1800);
    } finally {
      setSaving(false);
    }
  };

  const set = <K extends keyof PeeringSettings>(k: K, v: PeeringSettings[K]) =>
    setDraft((prev) => ({ ...prev, [k]: v }));

  return (
    <div className="space-y-3">
      {/* Mode */}
      <div>
        <span className="text-[10px] uppercase text-[#a3a3a3] block font-mono mb-1">Peering Mode</span>
        <div className="flex flex-wrap gap-1.5">
          {MODES.map(({ value, label, desc }) => (
            <button
              key={value}
              type="button"
              title={desc}
              onClick={() => set('mode', value)}
              className={`px-2.5 py-1 rounded text-[11px] font-medium border transition ${
                draft.mode === value
                  ? 'bg-[#e50914] text-white border-[#e50914]'
                  : 'bg-[#1a1a1a] text-[#a3a3a3] border-[#333333] hover:text-white hover:border-[#555]'
              }`}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      {/* Limits row */}
      <div className="grid grid-cols-2 gap-3">
        <div>
          <label className="text-[10px] uppercase text-[#a3a3a3] block font-mono mb-1">
            Max peers / torrent
          </label>
          <input
            type="number"
            min={1}
            max={500}
            value={draft.max_peers_per_torrent}
            onChange={(e) => set('max_peers_per_torrent', Math.max(1, parseInt(e.target.value) || 1))}
            className="w-full px-2.5 py-1 rounded bg-[#141414] border border-[#333333] text-white font-mono text-xs focus:outline-none focus:border-[#e50914]"
          />
        </div>
        <div>
          <label className="text-[10px] uppercase text-[#a3a3a3] block font-mono mb-1">
            Max global peers
          </label>
          <input
            type="number"
            min={1}
            max={5000}
            value={draft.max_global_peers}
            onChange={(e) => set('max_global_peers', Math.max(1, parseInt(e.target.value) || 1))}
            className="w-full px-2.5 py-1 rounded bg-[#141414] border border-[#333333] text-white font-mono text-xs focus:outline-none focus:border-[#e50914]"
          />
        </div>
      </div>

      {/* Toggles row */}
      <div className="flex flex-wrap gap-x-5 gap-y-2">
        {(
          [
            { key: 'locality_enabled', label: 'Locality scoring' },
            { key: 'prefer_utp', label: 'Prefer µTP' },
            { key: 'require_encryption', label: 'Require encryption' },
            { key: 'probe_rtt_enabled', label: 'RTT probing (opt-in)' },
          ] as { key: keyof PeeringSettings & string; label: string }[]
        ).map(({ key, label }) => {
          const val = draft[key as keyof PeeringSettings] as boolean;
          return (
            <label key={key} className="flex items-center gap-2 cursor-pointer select-none">
              <button
                type="button"
                role="switch"
                aria-checked={val}
                onClick={() => set(key as keyof PeeringSettings, !val as PeeringSettings[typeof key])}
                className={`relative w-8 h-4 rounded-full border transition ${
                  val
                    ? 'bg-[#e50914] border-[#e50914]'
                    : 'bg-[#1a1a1a] border-[#333333]'
                }`}
              >
                <span
                  className={`absolute top-0.5 w-3 h-3 rounded-full bg-white transition-[left] duration-200 ${
                    val ? 'left-4' : 'left-0.5'
                  }`}
                />
              </button>
              <span className="text-[11px] text-[#a3a3a3] font-sans">{label}</span>
            </label>
          );
        })}
      </div>

      {/* RTT probe notice */}
      {draft.probe_rtt_enabled && (
        <p className="text-[10px] text-amber-400/80 font-sans">
          RTT probing is enabled — the engine will make outbound TCP connections to every connected peer to measure latency. Disable this if you are concerned about privacy.
        </p>
      )}

      {/* Save */}
      <div className="flex items-center gap-3">
        <button
          type="button"
          disabled={!isDirty || saving}
          onClick={handleSave}
          className={`px-4 py-1.5 rounded text-xs font-semibold transition ${
            isDirty && !saving
              ? 'bg-[#e50914] hover:bg-[#f40612] text-white'
              : 'bg-[#1f1f1f] text-[#555] cursor-not-allowed border border-[#2a2a2a]'
          }`}
        >
          {saving ? 'Saving…' : saved ? 'Saved ✓' : 'Save Settings'}
        </button>
        {isDirty && !saving && (
          <button
            type="button"
            onClick={() => setDraft({ ...settings })}
            className="text-[11px] text-[#a3a3a3] hover:text-white transition"
          >
            Discard
          </button>
        )}
      </div>
    </div>
  );
};

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes >= 1_099_511_627_776) return `${(bytes / 1_099_511_627_776).toFixed(2)} TiB`;
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(2)} GiB`;
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(1)} MiB`;
  if (bytes >= 1_024) return `${(bytes / 1_024).toFixed(0)} KiB`;
  return `${bytes} B`;
}

// ── ADC member hover card ─────────────────────────────────────────────────────

const AdcMemberCell: React.FC<{ member: SwarmMember }> = ({ member }) => {
  const [open_, setOpen_] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  // Close on outside click.
  useEffect(() => {
    if (!open_) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen_(false);
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open_]);

  const handleUsernameClick = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try { await open(member.profile_url); } catch { /* degrade silently */ }
  };

  return (
    <div ref={ref} className="relative inline-flex items-center">
      <button
        type="button"
        onMouseEnter={() => setOpen_(true)}
        onMouseLeave={() => setOpen_(false)}
        onClick={handleUsernameClick}
        className="text-[#e50914] hover:underline font-sans text-[11px] leading-none truncate max-w-[100px]"
        title={`ADC: ${member.username} — click to open profile`}
      >
        {member.username}
      </button>

      {/* Hover card */}
      {open_ && (
        <div
          className="absolute z-50 bottom-full left-0 mb-1 w-52 rounded border border-[#333333] bg-[#141414] shadow-xl p-2.5 text-[11px] font-sans"
          onMouseEnter={() => setOpen_(true)}
          onMouseLeave={() => setOpen_(false)}
        >
          <p className="text-white font-semibold mb-1.5 truncate">{member.username}</p>
          <div className="space-y-1 text-[#a3a3a3]">
            <div className="flex justify-between">
              <span>Ratio</span>
              <span className={`font-mono ${member.ratio >= 1 ? 'text-emerald-400' : 'text-amber-400'}`}>
                {member.ratio.toFixed(2)}
              </span>
            </div>
            <div className="flex justify-between">
              <span>Upload</span>
              <span className="font-mono text-white">{formatBytes(member.uploaded_bytes)}</span>
            </div>
            <div className="flex justify-between">
              <span>Download</span>
              <span className="font-mono text-white">{formatBytes(member.downloaded_bytes)}</span>
            </div>
            <div className="flex justify-between">
              <span>BP</span>
              <span className="font-mono text-cyan-400">{member.bonus_points.toFixed(0)}</span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

/** True when the tracker URL is an ADC tracker. */
function isAdcTracker(url: string): boolean {
  try {
    return new URL(url).hostname.endsWith('asiandvdclub.org');
  } catch {
    return false;
  }
}

// ── Main component ────────────────────────────────────────────────────────────

interface PeersTabPanelProps {
  peers: PeerTelemetry[];
  isLoading?: boolean;
  error?: string | null;
  onRetry?: () => void;
  torrentStatus?: string;
  settings: PeeringSettings | null;
  fetchSettings: () => Promise<void>;
  saveSettings: (s: PeeringSettings) => Promise<void>;
  isTransmission: boolean;
  /** 40-char lowercase hex. Required for ADC peer lookup. */
  infoHash?: string | null;
  /** Tracker list of the selected torrent, used to detect ADC membership. */
  trackers?: TorrentTrackerItem[];
}

export const PeersTabPanel: React.FC<PeersTabPanelProps> = ({
  peers,
  isLoading = false,
  error = null,
  onRetry,
  torrentStatus,
  settings,
  fetchSettings,
  saveSettings,
  isTransmission,
  infoHash,
  trackers = [],
}) => {
  const [sortKey, setSortKey] = useState<SortKey>('sustained_dl_bps');
  const [sortAsc, setSortAsc] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [adcStatus, setAdcStatus] = useState<string>('idle');
  const [adcRetry, setAdcRetry] = useState(0);
  const credentialRevision = useTrackerStore((s) => s.credentialRevision);
  const setShowTrackerDashboard = useUiStore((s) => s.setShowTrackerDashboard);

  // ── ADC member state ───────────────────────────────────────────────────────
  // Map<"ip:port", SwarmMember> for fast per-row lookup.
  const [adcMap, setAdcMap] = useState<Map<string, SwarmMember>>(new Map());
  // Track which info_hash the current adcMap was fetched for.
  const lastFetchedKey = useRef<string | null>(null);
  const lastIdentity = useRef<string | null>(null);
  const requestId = useRef(0);

  const hasAdcTracker = useCallback(
    () => trackers.some((tr) => isAdcTracker(tr.url)),
    [trackers],
  );

  useEffect(() => {
    const identity = `${infoHash ?? ''}:${credentialRevision}`;
    if (lastIdentity.current !== identity) {
      lastIdentity.current = identity;
      setAdcMap(new Map());
      setAdcStatus('idle');
      lastFetchedKey.current = null;
      requestId.current += 1;
    }

    if (!infoHash || !hasAdcTracker()) {
      setAdcStatus('idle');
      setAdcMap(new Map());
      lastFetchedKey.current = null;
      requestId.current += 1;
      return;
    }
    if (peers.length === 0) {
      setAdcStatus('waiting_for_peers');
      lastFetchedKey.current = null;
      requestId.current += 1;
      return;
    }

    const peerAddrs = peers.map((p) => ({ ip: p.ip, port: p.port }));
    const peerKey = peerAddrs.map((p) => `${p.ip}:${p.port}`).sort().join(',');
    const cacheKey = `${infoHash}:${credentialRevision}:${peerKey}`;
    const fetchKey = `${cacheKey}:${adcRetry}`;
    if (lastFetchedKey.current === fetchKey) return;

    // Check cache first.
    const cached = getCachedMembers(cacheKey);
    if (cached) {
      lastFetchedKey.current = fetchKey;
      setAdcStatus(cached.status);
      const m = new Map<string, SwarmMember>();
      for (const member of cached.matches) m.set(`${member.ip}:${member.port}`, member);
      setAdcMap(m);
      return;
    }

    lastFetchedKey.current = fetchKey;
    setAdcStatus('loading');
    const request = ++requestId.current;

    invoke<SwarmMembersResult>('adc_swarm_members', {
      info_hash: infoHash,
      peers: peerAddrs,
    })
      .then((result) => {
        if (request !== requestId.current) return;
        if (['matched', 'no_matching_peers', 'endpoint_unsupported'].includes(result.status)) {
          setCachedMembers(cacheKey, result);
        }
        setAdcStatus(result.status);
        const m = new Map<string, SwarmMember>();
        for (const member of result.matches) m.set(`${member.ip}:${member.port}`, member);
        setAdcMap(m);
      })
      .catch(() => {
        if (request !== requestId.current) return;
        setAdcStatus('ipc_error');
      });
  }, [infoHash, peers, hasAdcTracker, credentialRevision, adcRetry]);

  // Load settings once on mount if not yet loaded
  useEffect(() => {
    if (!settings) {
      fetchSettings();
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const handleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortAsc((a) => !a);
    } else {
      setSortKey(key);
      setSortAsc(false); // new column defaults to descending
    }
  };

  const sorted = useMemo(() => sortPeers(peers, sortKey, sortAsc), [peers, sortKey, sortAsc]);

  // Keep the column visible for ADC torrents even when credentials need
  // attention, so a failed lookup never silently removes the feature.
  const showAdcCol = hasAdcTracker();
  // colSpan must account for the extra ADC column when shown.
  const colCount = showAdcCol ? 10 : 9;
  const adcNotice = (() => {
    switch (adcStatus) {
      case 'credential_required':
      case 'not_linked':
        return 'ADC credentials are required to identify swarm members.';
      case 'credential_access_denied':
        return 'Keychain access needs authorization. Unlock or relink your ADC account.';
      case 'credential_storage_unavailable':
      case 'credential_storage_error':
        return 'The secure credential store is unavailable.';
      case 'authentication_failed':
        return 'ADC rejected the stored credential. Relink your account.';
      case 'no_matching_peers':
        return 'No connected peers matched an ADC member.';
      case 'endpoint_unsupported':
        return 'The ADC peer-information endpoint is unavailable.';
      case 'rate_limited':
        return 'ADC is rate limiting peer lookups. Retry later.';
      case 'network_error':
      case 'ipc_error':
        return 'Could not reach the ADC peer-information service.';
      case 'parse_error':
      case 'http_error':
        return 'ADC returned an unexpected peer-information response.';
      case 'invalid_info_hash':
        return 'The selected torrent has an invalid info hash.';
      default:
        return null;
    }
  })();

  return (
    <div className="flex flex-col gap-2 h-full">
      {/* Toolbar */}
      <div className="flex items-center justify-between">
        <span className="text-[11px] text-[#a3a3a3] font-mono">
          {peers.length} peer{peers.length !== 1 ? 's' : ''} connected
        </span>
        {isTransmission && (
          <button
            type="button"
            onClick={() => {
              if (!showSettings && !settings) fetchSettings();
              setShowSettings((v) => !v);
            }}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-[11px] font-medium border transition ${
              showSettings
                ? 'bg-[#e50914]/10 text-[#e50914] border-[#e50914]/30'
                : 'bg-[#1a1a1a] text-[#a3a3a3] border-[#333333] hover:text-white'
            }`}
          >
            <Settings className="w-3.5 h-3.5" />
            <span>Peering Settings</span>
          </button>
        )}
      </div>

      {/* Inline Peering Settings panel */}
      {showSettings && settings && (
        <div className="p-3 rounded bg-[#1a1a1a] border border-[#2a2a2a]">
          <PeeringSettingsPanel settings={settings} onSave={saveSettings} />
        </div>
      )}

      {showAdcCol && adcNotice && adcStatus !== 'loading' && (
        <div className="flex items-center justify-between gap-3 px-2.5 py-1.5 rounded border border-[#333333] bg-[#1a1a1a] text-[11px] text-[#a3a3a3]">
          <span>{adcNotice}</span>
          {['credential_required', 'not_linked', 'credential_access_denied', 'credential_storage_unavailable', 'credential_storage_error', 'authentication_failed'].includes(adcStatus) ? (
            <button
              type="button"
              onClick={() => setShowTrackerDashboard(true)}
              className="shrink-0 text-[#e50914] hover:underline"
            >
              Open Tracker settings
            </button>
          ) : (
            ['network_error', 'ipc_error', 'rate_limited'].includes(adcStatus) && (
              <button
                type="button"
                onClick={() => setAdcRetry((value) => value + 1)}
                className="shrink-0 text-[#e50914] hover:underline"
              >
                Retry
              </button>
            )
          )}
        </div>
      )}

      {/* Peer table */}
      <div className="overflow-x-auto border border-[#262626] rounded flex-1">
        <table className="w-full text-left border-collapse text-xs">
          <thead className="bg-[#1f1f1f] border-b border-[#262626]">
            <tr>
              <th className="py-1.5 px-2 text-[10px] uppercase font-mono text-[#a3a3a3]">Flags</th>
              <ColHeader label="Client" sortKey="client_name" current={sortKey} asc={sortAsc} onSort={handleSort} />
              <ColHeader label="IP / Port" sortKey="ip" current={sortKey} asc={sortAsc} onSort={handleSort} />
              <ColHeader label="Locality" sortKey="locality_tier" current={sortKey} asc={sortAsc} onSort={handleSort} />
              <ColHeader label="CC" sortKey="country_code" current={sortKey} asc={sortAsc} onSort={handleSort} />
              {showAdcCol && (
                <th className="py-1.5 px-2 text-[10px] uppercase font-mono text-[#a3a3a3]">ADC</th>
              )}
              <ColHeader label="RTT" sortKey="rtt_ms" current={sortKey} asc={sortAsc} onSort={handleSort} />
              <ColHeader label="Down" sortKey="sustained_dl_bps" current={sortKey} asc={sortAsc} onSort={handleSort} />
              <ColHeader label="Up" sortKey="sustained_ul_bps" current={sortKey} asc={sortAsc} onSort={handleSort} />
              <ColHeader label="Quality" sortKey="stability_score" current={sortKey} asc={sortAsc} onSort={handleSort} />
            </tr>
          </thead>
          <tbody className="divide-y divide-[#262626] font-mono text-[11px]">
            {isLoading && sorted.length === 0 ? (
              <tr>
                <td colSpan={colCount} className="py-8 text-center text-[#a3a3a3] font-sans text-xs">
                  <div className="flex flex-col items-center justify-center gap-2">
                    <Loader2 className="w-5 h-5 animate-spin text-[#e50914]" />
                    <span>Loading connected peers...</span>
                  </div>
                </td>
              </tr>
            ) : error ? (
              <tr>
                <td colSpan={colCount} className="py-8 text-center text-[#e50914] font-sans text-xs">
                  <div className="flex flex-col items-center justify-center gap-2">
                    <div className="flex items-center gap-1.5 text-red-400">
                      <AlertCircle className="w-4 h-4" />
                      <span>Failed to query peer telemetry: {error}</span>
                    </div>
                    {onRetry && (
                      <button
                        type="button"
                        onClick={onRetry}
                        className="mt-1 flex items-center gap-1.5 px-3 py-1 bg-[#262626] hover:bg-[#333333] text-white rounded text-xs transition"
                      >
                        <RotateCw className="w-3 h-3" />
                        <span>Retry</span>
                      </button>
                    )}
                  </div>
                </td>
              </tr>
            ) : sorted.length === 0 ? (
              <tr>
                <td colSpan={colCount} className="py-8 text-center text-[#a3a3a3] font-sans text-xs">
                  {torrentStatus === 'Seeding'
                    ? 'Seeding to swarm — no active downloaders currently connected'
                    : torrentStatus === 'Paused'
                    ? 'Torrent is paused — peer connections closed'
                    : torrentStatus === 'Downloading'
                    ? 'No peers currently connected — waiting for swarm connections'
                    : 'No active peers connected to this torrent'}
                </td>
              </tr>
            ) : (
              sorted.map((p) => {
                const adcKey = `${p.ip}:${p.port}`;
                const member = adcMap.get(adcKey);
                return (
                  <tr key={adcKey} className="hover:bg-[#1f1f1f] transition-colors">
                    {/* Protocol flags */}
                    <td className="py-1.5 px-2">
                      <div className="flex items-center gap-0.5">
                        <Flag label="µTP" active={p.utp} />
                        <Flag label="Enc" active={p.encryption} />
                        <Flag label="v6" active={p.ipv6} />
                        <Flag label="DHT" active={p.dht} />
                        <Flag label="PEX" active={p.pex} />
                      </div>
                    </td>

                    {/* Client name */}
                    <td className="py-1.5 px-2 text-white font-sans max-w-[120px] truncate" title={p.client_name}>
                      {p.client_name || <span className="text-[#555]">Unknown</span>}
                    </td>

                    {/* IP:port — always visible */}
                    <td className="py-1.5 px-2 text-[#a3a3a3] whitespace-nowrap">
                      {p.ip}
                      <span className="text-[#555]">:{p.port}</span>
                    </td>

                    {/* Locality */}
                    <td className="py-1.5 px-2">
                      <LocalityChip tier={p.locality_tier} />
                    </td>

                    {/* Country code — always visible */}
                    <td className="py-1.5 px-2 text-[#a3a3a3] w-10">
                      {p.country_code ? (
                        <span className="text-white font-semibold">{p.country_code}</span>
                      ) : (
                        <span className="text-[#444]">—</span>
                      )}
                    </td>

                    {/* ADC member — only when ADC column is shown */}
                    {showAdcCol && (
                      <td className="py-1.5 px-2 max-w-[110px]">
                        {member ? (
                          <AdcMemberCell member={member} />
                        ) : (
                          <span className="text-[#444]">—</span>
                        )}
                      </td>
                    )}

                    {/* RTT */}
                    <td className="py-1.5 px-2 whitespace-nowrap">
                      {p.rtt_ms > 0 ? (
                        <span
                          className={
                            p.rtt_ms < 30
                              ? 'text-emerald-400'
                              : p.rtt_ms < 100
                              ? 'text-amber-400'
                              : 'text-[#e50914]'
                          }
                        >
                          {p.rtt_ms}ms
                        </span>
                      ) : (
                        <span className="text-[#444]">—</span>
                      )}
                    </td>

                    {/* Down speed */}
                    <td className="py-1.5 px-2 text-white whitespace-nowrap">
                      {formatSpeed(p.sustained_dl_bps / 8)}
                    </td>

                    {/* Up speed */}
                    <td className="py-1.5 px-2 text-[#a3a3a3] whitespace-nowrap">
                      {formatSpeed(p.sustained_ul_bps / 8)}
                    </td>

                    {/* Quality bar */}
                    <td className="py-1.5 px-2">
                      <QualityBar score={p.stability_score} />
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
};

export default PeersTabPanel;
