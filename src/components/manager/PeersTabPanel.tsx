import React, { useState, useEffect, useMemo } from 'react';
import { Settings, ChevronDown, ChevronUp, ChevronsUpDown } from 'lucide-react';
import { PeerTelemetry, PeeringSettings, PeeringMode, LocalityTier } from '../../types';
import { formatSpeed } from '../../lib/formatters';

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

// ── Main component ────────────────────────────────────────────────────────────

interface PeersTabPanelProps {
  peers: PeerTelemetry[];
  settings: PeeringSettings | null;
  fetchSettings: () => Promise<void>;
  saveSettings: (s: PeeringSettings) => Promise<void>;
  isTransmission: boolean;
}

export const PeersTabPanel: React.FC<PeersTabPanelProps> = ({
  peers,
  settings,
  fetchSettings,
  saveSettings,
  isTransmission,
}) => {
  const [sortKey, setSortKey] = useState<SortKey>('sustained_dl_bps');
  const [sortAsc, setSortAsc] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

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
              <ColHeader label="RTT" sortKey="rtt_ms" current={sortKey} asc={sortAsc} onSort={handleSort} />
              <ColHeader label="Down" sortKey="sustained_dl_bps" current={sortKey} asc={sortAsc} onSort={handleSort} />
              <ColHeader label="Up" sortKey="sustained_ul_bps" current={sortKey} asc={sortAsc} onSort={handleSort} />
              <ColHeader label="Quality" sortKey="stability_score" current={sortKey} asc={sortAsc} onSort={handleSort} />
            </tr>
          </thead>
          <tbody className="divide-y divide-[#262626] font-mono text-[11px]">
            {sorted.length === 0 ? (
              <tr>
                <td colSpan={9} className="py-6 text-center text-[#a3a3a3] font-sans text-xs">
                  No active peers connected yet
                </td>
              </tr>
            ) : (
              sorted.map((p) => (
                <tr key={`${p.ip}:${p.port}`} className="hover:bg-[#1f1f1f] transition-colors">
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

                  {/* IP:port */}
                  <td className="py-1.5 px-2 text-[#a3a3a3] whitespace-nowrap">
                    {p.ip}
                    <span className="text-[#555]">:{p.port}</span>
                  </td>

                  {/* Locality */}
                  <td className="py-1.5 px-2">
                    <LocalityChip tier={p.locality_tier} />
                  </td>

                  {/* Country code */}
                  <td className="py-1.5 px-2 text-[#a3a3a3] w-10">
                    {p.country_code ? (
                      <span className="text-white font-semibold">{p.country_code}</span>
                    ) : (
                      <span className="text-[#444]">—</span>
                    )}
                  </td>

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
                    {formatSpeed(p.sustained_dl_bps)}
                  </td>

                  {/* Up speed */}
                  <td className="py-1.5 px-2 text-[#a3a3a3] whitespace-nowrap">
                    {formatSpeed(p.sustained_ul_bps)}
                  </td>

                  {/* Quality bar */}
                  <td className="py-1.5 px-2">
                    <QualityBar score={p.stability_score} />
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
};

export default PeersTabPanel;
