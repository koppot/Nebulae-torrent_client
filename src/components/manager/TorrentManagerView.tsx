import React, { useEffect } from 'react';
import { useTorrentStore } from '../../stores/torrentStore';
import { useUiStore } from '../../stores/uiStore';
import { usePeeringStore } from '../../stores/peeringStore';
import { formatBytes, formatSpeed, formatEta, formatRatio } from '../../lib/formatters';
import { PeeringMode } from '../../types';
import {
  Play,
  Pause,
  Trash2,
  RotateCw,
  Search,
  HardDrive,
  Users,
  Activity,
  Layers,
  Zap,
  Globe,
  Radio,
} from 'lucide-react';

export const TorrentManagerView: React.FC = () => {
  const {
    torrents,
    selectedId,
    setSelectedId,
    pauseTorrent,
    resumeTorrent,
  } = useTorrentStore();

  const {
    activeManagerTab,
    setActiveManagerTab,
    managerCategoryFilter,
    setManagerCategoryFilter,
    searchQuery,
    setSearchQuery,
  } = useUiStore();

  const {
    peers,
    settings: peeringSettings,
    fetchSettings: fetchPeeringSettings,
    saveSettings: savePeeringSettings,
    fetchPeers,
  } = usePeeringStore();

  useEffect(() => {
    fetchPeeringSettings();
    if (selectedId) {
      fetchPeers(selectedId);
    }
  }, [selectedId, fetchPeeringSettings, fetchPeers]);

  const filteredTorrents = torrents.filter((t) => {
    if (searchQuery && !t.name.toLowerCase().includes(searchQuery.toLowerCase())) {
      return false;
    }
    if (managerCategoryFilter === 'downloading') return t.status === 'Downloading';
    if (managerCategoryFilter === 'seeding') return t.status === 'Seeding';
    if (managerCategoryFilter === 'completed') return t.status === 'Completed';
    if (managerCategoryFilter === 'paused') return t.status === 'Paused';
    if (managerCategoryFilter === 'media') return t.is_media;
    return true;
  });

  const selectedTorrent = torrents.find((t) => t.id === selectedId) || torrents[0];

  const handleModeChange = (mode: PeeringMode) => {
    if (peeringSettings) {
      savePeeringSettings({ ...peeringSettings, mode });
    }
  };

  return (
    <div className="flex-1 flex overflow-hidden font-sans select-none text-adc-ivory">
      {/* Sidebar Filters */}
      <aside className="w-56 bg-black/70 border-r border-white/10 backdrop-blur-md flex flex-col p-3 space-y-4 text-xs">
        <div>
          <span className="text-[10px] font-bold uppercase tracking-wider text-adc-muted px-2 block mb-1">
            Status Filters
          </span>
          <div className="space-y-0.5">
            {[
              { id: 'all', label: 'All Transfers', count: torrents.length },
              { id: 'downloading', label: 'Downloading', count: torrents.filter((t) => t.status === 'Downloading').length },
              { id: 'seeding', label: 'Seeding', count: torrents.filter((t) => t.status === 'Seeding').length },
              { id: 'completed', label: 'Completed', count: torrents.filter((t) => t.status === 'Completed').length },
              { id: 'paused', label: 'Paused', count: torrents.filter((t) => t.status === 'Paused').length },
            ].map((item) => (
              <button
                key={item.id}
                onClick={() => setManagerCategoryFilter(item.id)}
                className={`w-full flex items-center justify-between px-3 py-1.5 rounded-lg transition ${
                  managerCategoryFilter === item.id
                    ? 'adc-btn-crimson font-semibold'
                    : 'text-adc-ivory/70 hover:text-white hover:bg-white/5'
                }`}
              >
                <span>{item.label}</span>
                <span className="text-[10px] font-mono opacity-70">{item.count}</span>
              </button>
            ))}
          </div>
        </div>

        <div>
          <span className="text-[10px] font-bold uppercase tracking-wider text-adc-muted px-2 block mb-1">
            Categories
          </span>
          <div className="space-y-0.5">
            {[
              { id: 'media', label: 'Cinema & Video', count: torrents.filter((t) => t.is_media).length },
            ].map((item) => (
              <button
                key={item.id}
                onClick={() => setManagerCategoryFilter(item.id)}
                className={`w-full flex items-center justify-between px-3 py-1.5 rounded-lg transition ${
                  managerCategoryFilter === item.id
                    ? 'adc-btn-crimson font-semibold'
                    : 'text-adc-ivory/70 hover:text-white hover:bg-white/5'
                }`}
              >
                <span>{item.label}</span>
                <span className="text-[10px] font-mono opacity-70">{item.count}</span>
              </button>
            ))}
          </div>
        </div>
      </aside>

      {/* Main Table + Tabbed Bottom Inspector */}
      <div className="flex-1 flex flex-col overflow-hidden bg-black/90">
        {/* Action Toolbar */}
        <div className="h-11 border-b border-white/10 bg-black/50 backdrop-blur-md flex items-center justify-between px-4">
          <div className="flex items-center gap-2">
            <button
              onClick={() => selectedTorrent && resumeTorrent(selectedTorrent.id)}
              className="adc-btn-glass p-1.5 rounded-md text-white/80 hover:text-emerald-400"
              title="Resume"
            >
              <Play className="w-3.5 h-3.5" />
            </button>
            <button
              onClick={() => selectedTorrent && pauseTorrent(selectedTorrent.id)}
              className="adc-btn-glass p-1.5 rounded-md text-white/80 hover:text-amber-400"
              title="Pause"
            >
              <Pause className="w-3.5 h-3.5" />
            </button>
            <button
              className="adc-btn-glass p-1.5 rounded-md text-white/80 hover:text-red-400"
              title="Remove"
            >
              <Trash2 className="w-3.5 h-3.5" />
            </button>
            <button
              className="adc-btn-glass p-1.5 rounded-md text-white/80 hover:text-cyan-400"
              title="Force Recheck"
            >
              <RotateCw className="w-3.5 h-3.5" />
            </button>
          </div>

          {/* Quick Search */}
          <div className="relative w-64">
            <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-white/40" />
            <input
              type="text"
              placeholder="Filter torrents..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full pl-8 pr-3 py-1 bg-white/5 border border-white/15 rounded-lg text-xs text-adc-ivory placeholder:text-white/40 focus:outline-none focus:border-adc-crimson"
            />
          </div>
        </div>

        {/* Data Grid Table */}
        <div className="flex-1 overflow-auto border-b border-white/10">
          <table className="w-full text-left border-collapse text-xs">
            <thead className="sticky top-0 bg-black/95 backdrop-blur border-b border-white/10 text-[11px] font-semibold text-adc-muted uppercase font-mono tracking-wider">
              <tr>
                <th className="py-2.5 px-4">Name</th>
                <th className="py-2.5 px-3">Size</th>
                <th className="py-2.5 px-3 w-36">Progress</th>
                <th className="py-2.5 px-3">Status</th>
                <th className="py-2.5 px-3">Down Speed</th>
                <th className="py-2.5 px-3">Up Speed</th>
                <th className="py-2.5 px-3">ETA</th>
                <th className="py-2.5 px-3">Ratio</th>
                <th className="py-2.5 px-3">Seeds</th>
                <th className="py-2.5 px-3">Peers</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-white/5 font-mono text-adc-ivory">
              {filteredTorrents.length === 0 && (
                <tr>
                  <td colSpan={10} className="py-20 text-center text-adc-muted">
                    <p className="text-sm font-sans font-medium text-white/70 mb-1">No torrents in list</p>
                    <p className="text-xs font-sans text-white/40">
                      Click <strong className="text-adc-gold font-semibold">+ Add Torrent</strong> in the top navigation or drop a <code className="text-white/60">.torrent</code> file to begin
                    </p>
                  </td>
                </tr>
              )}
              {filteredTorrents.map((t) => (
                <tr
                  key={t.id}
                  onClick={() => setSelectedId(t.id)}
                  className={`hover:bg-white/5 cursor-pointer transition ${
                    selectedId === t.id ? 'bg-adc-crimson/20 text-white font-medium' : ''
                  }`}
                >
                  <td className="py-2.5 px-4 font-sans font-medium truncate max-w-xs">{t.name}</td>
                  <td className="py-2.5 px-3 text-adc-muted">{formatBytes(t.total_bytes)}</td>
                  <td className="py-2.5 px-3">
                    <div className="flex items-center gap-2">
                      <div className="flex-1 h-1.5 rounded-full bg-black/80 overflow-hidden border border-white/10">
                        <div
                          className="h-full bg-adc-crimson rounded-full"
                          style={{ width: `${Math.round(t.progress * 100)}%` }}
                        />
                      </div>
                      <span className="text-[10px] w-8 text-right font-mono">{Math.round(t.progress * 100)}%</span>
                    </div>
                  </td>
                  <td className="py-2.5 px-3">
                    <span
                      className={`px-2 py-0.5 rounded text-[10px] font-semibold uppercase ${
                        t.status === 'Downloading'
                          ? 'bg-blue-950/60 text-blue-300 border border-blue-800/50'
                          : t.status === 'Seeding'
                          ? 'bg-emerald-950/60 text-emerald-300 border border-emerald-800/50'
                          : 'bg-black/60 text-adc-muted border border-white/10'
                      }`}
                    >
                      {t.status}
                    </span>
                  </td>
                  <td className="py-2.5 px-3 text-cyan-400">{formatSpeed(t.download_rate)}</td>
                  <td className="py-2.5 px-3 text-amber-400">{formatSpeed(t.upload_rate)}</td>
                  <td className="py-2.5 px-3 text-adc-muted">{formatEta(t.eta_seconds)}</td>
                  <td className="py-2.5 px-3">{formatRatio(t.ratio)}</td>
                  <td className="py-2.5 px-3 text-adc-muted">{t.seeds}</td>
                  <td className="py-2.5 px-3 text-adc-muted">{t.peers}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        {/* Bottom Tabbed Inspector */}
        <div className="h-64 bg-black/95 border-t border-white/10 flex flex-col">
          <div className="flex border-b border-white/10 px-4 pt-2 gap-2 text-xs">
            {[
              { id: 'general', label: 'General', icon: Activity },
              { id: 'files', label: 'Files & Media', icon: HardDrive },
              { id: 'trackers', label: 'Trackers', icon: Layers },
              { id: 'peers', label: 'Smart Peering', icon: Users },
            ].map((tab) => {
              const Icon = tab.icon;
              return (
                <button
                  key={tab.id}
                  onClick={() => setActiveManagerTab(tab.id as any)}
                  className={`flex items-center gap-1.5 px-3 py-1.5 rounded-t-lg transition border-b-2 font-medium ${
                    activeManagerTab === tab.id
                      ? 'border-adc-crimson text-white bg-white/5'
                      : 'border-transparent text-adc-muted hover:text-white'
                  }`}
                >
                  <Icon className="w-3.5 h-3.5" />
                  <span>{tab.label}</span>
                </button>
              );
            })}
          </div>

          <div className="flex-1 p-4 overflow-y-auto text-xs text-adc-ivory">
            {selectedTorrent ? (
              activeManagerTab === 'general' ? (
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4 font-mono">
                  <div>
                    <span className="text-[10px] uppercase text-adc-muted block">Total Size</span>
                    <span className="text-white">{formatBytes(selectedTorrent.total_bytes)}</span>
                  </div>
                  <div>
                    <span className="text-[10px] uppercase text-adc-muted block">Downloaded</span>
                    <span className="text-white">{formatBytes(selectedTorrent.downloaded_bytes)}</span>
                  </div>
                  <div>
                    <span className="text-[10px] uppercase text-adc-muted block">Uploaded</span>
                    <span className="text-white">{formatBytes(selectedTorrent.uploaded_bytes)}</span>
                  </div>
                  <div>
                    <span className="text-[10px] uppercase text-adc-muted block">Share Ratio</span>
                    <span className="text-emerald-400">{formatRatio(selectedTorrent.ratio)}</span>
                  </div>
                </div>
              ) : activeManagerTab === 'files' ? (
                <div className="space-y-1.5 font-mono text-xs">
                  <div className="p-2.5 rounded bg-black/60 border border-white/10 flex justify-between items-center">
                    <div>
                      <span className="text-white font-sans">{selectedTorrent.name}</span>
                      <span className="block text-[10px] text-adc-muted font-mono mt-0.5">Priority: Sequential (Media Playable)</span>
                    </div>
                    <span className="text-adc-muted">{formatBytes(selectedTorrent.total_bytes)} · 100% Ready</span>
                  </div>
                </div>
              ) : activeManagerTab === 'trackers' ? (
                <div className="font-mono text-xs space-y-2">
                  <div className="p-2.5 rounded bg-black/60 border border-white/10 flex justify-between text-adc-muted">
                    <span>https://tracker.asiandvdclub.org:2710/announce</span>
                    <span className="text-emerald-400 font-semibold">Announce OK (HTTP 200 · Next in 23m)</span>
                  </div>
                  <div className="p-2.5 rounded bg-black/60 border border-white/10 flex justify-between text-adc-muted">
                    <span>udp://tracker.opentrackr.org:1337/announce</span>
                    <span className="text-emerald-400 font-semibold">Active (Peers: {selectedTorrent.peers})</span>
                  </div>
                </div>
              ) : (
                /* Enhanced Smart Peering Tab */
                <div className="space-y-3">
                  {/* Peering Mode Selector & Info */}
                  <div className="flex flex-wrap items-center justify-between gap-2 p-2 rounded-lg bg-black/60 border border-white/10">
                    <div className="flex items-center gap-2">
                      <span className="text-xs text-adc-muted flex items-center gap-1">
                        <Zap className="w-3.5 h-3.5 text-adc-crimson-light" />
                        Peering Mode:
                      </span>
                      <select
                        value={peeringSettings?.mode || 'Balanced'}
                        onChange={(e) => handleModeChange(e.target.value as PeeringMode)}
                        className="px-2 py-1 text-xs rounded bg-black border border-white/20 text-white focus:outline-none focus:border-adc-crimson"
                      >
                        <option value="Balanced">Balanced (Default)</option>
                        <option value="SpeedFirst">Speed First</option>
                        <option value="LatencyFirst">Latency First</option>
                        <option value="LocalFirst">Local First (ISP/LAN)</option>
                        <option value="PrivacyFirst">Privacy First (Randomized)</option>
                      </select>
                    </div>

                    <div className="flex items-center gap-3 text-[11px] text-adc-muted">
                      <span className="flex items-center gap-1">
                        <Globe className="w-3 h-3 text-cyan-400" /> Locality Preference: {peeringSettings?.enable_locality_preference ? 'Active' : 'Off'}
                      </span>
                      <span className="flex items-center gap-1">
                        <Radio className="w-3 h-3 text-emerald-400" /> Active Peers: {peers.length}
                      </span>
                    </div>
                  </div>

                  {/* Peering Table */}
                  <div className="overflow-x-auto border border-white/10 rounded-lg">
                    <table className="w-full text-left border-collapse text-xs">
                      <thead className="bg-black/90 border-b border-white/10 text-[10px] uppercase font-mono text-adc-muted">
                        <tr>
                          <th className="py-2 px-3">Client</th>
                          <th className="py-2 px-3">IP & Port</th>
                          <th className="py-2 px-3">Locality</th>
                          <th className="py-2 px-3">Score</th>
                          <th className="py-2 px-3">RTT / Latency</th>
                          <th className="py-2 px-3">Throughput (DL / UL)</th>
                          <th className="py-2 px-3">Flags</th>
                          <th className="py-2 px-3">Stability</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-white/5 font-mono text-[11px]">
                        {peers.map((p) => {
                          const score = Math.round(p.stability_score * 100);
                          return (
                            <tr key={p.peer_id} className="hover:bg-white/5">
                              <td className="py-2 px-3 font-sans text-white">{p.client_name}</td>
                              <td className="py-2 px-3 text-adc-muted">{p.ip}:{p.port}</td>
                              <td className="py-2 px-3">
                                <span className={`px-1.5 py-0.5 rounded text-[10px] font-semibold uppercase ${
                                  p.locality_tier === 'Lan'
                                    ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/30'
                                    : p.locality_tier === 'Country'
                                    ? 'bg-blue-500/20 text-blue-300 border border-blue-500/30'
                                    : 'bg-white/10 text-adc-muted'
                                }`}>
                                  {p.locality_tier} {p.country_code ? `(${p.country_code})` : ''}
                                </span>
                              </td>
                              <td className="py-2 px-3">
                                <span className="font-bold text-emerald-400">{score}/100</span>
                              </td>
                              <td className="py-2 px-3 text-adc-muted">
                                {p.rtt_ms}ms <span className="text-[10px] text-white/40">({p.req_resp_latency_ms}ms resp)</span>
                              </td>
                              <td className="py-2 px-3">
                                <span className="text-cyan-400">{formatSpeed(p.sustained_dl_bps)}</span> /{' '}
                                <span className="text-amber-400">{formatSpeed(p.sustained_ul_bps)}</span>
                              </td>
                              <td className="py-2 px-3 text-[10px] text-adc-muted">
                                {p.utp && <span className="mr-1 text-cyan-300">uTP</span>}
                                {p.encryption && <span className="mr-1 text-emerald-300">ENC</span>}
                                {p.ipv6 && <span className="text-blue-300">IPv6</span>}
                              </td>
                              <td className="py-2 px-3 text-emerald-400">
                                {(p.stability_score * 100).toFixed(0)}%
                              </td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  </div>
                </div>
              )
            ) : (
              <div className="text-adc-muted">No torrent selected</div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
