import React from 'react';
import { useTorrentStore } from '../../stores/torrentStore';
import { useUiStore } from '../../stores/uiStore';
import { formatBytes, formatSpeed, formatEta, formatRatio } from '../../lib/formatters';
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

  return (
    <div className="flex-1 flex overflow-hidden font-sans select-none">
      {/* Sidebar Filters */}
      <aside className="w-56 bg-black/60 border-r border-white/10 backdrop-blur-md flex flex-col p-3 space-y-4 text-xs">
        <div>
          <span className="text-[10px] font-bold uppercase tracking-wider text-white/50 px-2 block mb-1">
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
                    : 'text-white/70 hover:text-white hover:bg-white/5'
                }`}
              >
                <span>{item.label}</span>
                <span className="text-[10px] font-mono opacity-70">{item.count}</span>
              </button>
            ))}
          </div>
        </div>

        <div>
          <span className="text-[10px] font-bold uppercase tracking-wider text-white/50 px-2 block mb-1">
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
                    : 'text-white/70 hover:text-white hover:bg-white/5'
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
      <div className="flex-1 flex flex-col overflow-hidden bg-black/80">
        {/* Action Toolbar */}
        <div className="h-11 border-b border-white/10 bg-black/40 backdrop-blur-md flex items-center justify-between px-4">
          <div className="flex items-center gap-2">
            <button
              onClick={() => selectedTorrent && resumeTorrent(selectedTorrent.id)}
              className="adc-btn-glass p-1.5 rounded-md text-white/80 hover:text-adc-emerald"
              title="Resume"
            >
              <Play className="w-3.5 h-3.5" />
            </button>
            <button
              onClick={() => selectedTorrent && pauseTorrent(selectedTorrent.id)}
              className="adc-btn-glass p-1.5 rounded-md text-white/80 hover:text-adc-gold"
              title="Pause"
            >
              <Pause className="w-3.5 h-3.5" />
            </button>
            <button
              className="adc-btn-glass p-1.5 rounded-md text-white/80 hover:text-adc-crimsonGlow"
              title="Remove"
            >
              <Trash2 className="w-3.5 h-3.5" />
            </button>
            <button
              className="adc-btn-glass p-1.5 rounded-md text-white/80 hover:text-adc-ice"
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
              className="w-full pl-8 pr-3 py-1 bg-white/5 border border-white/15 rounded-lg text-xs text-adc-ivory placeholder:text-white/40 focus:outline-none focus:border-adc-crimsonBright"
            />
          </div>
        </div>

        {/* Data Grid Table */}
        <div className="flex-1 overflow-auto border-b border-slate-800">
          <table className="w-full text-left border-collapse text-xs">
            <thead className="sticky top-0 bg-slate-950/90 backdrop-blur border-b border-slate-800 text-[11px] font-semibold text-slate-400 uppercase font-mono tracking-wider">
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
            <tbody className="divide-y divide-slate-800/60 font-mono text-slate-300">
              {filteredTorrents.map((t) => (
                <tr
                  key={t.id}
                  onClick={() => setSelectedId(t.id)}
                  className={`hover:bg-slate-800/40 cursor-pointer transition ${
                    selectedId === t.id ? 'bg-pink-950/30 text-white' : ''
                  }`}
                >
                  <td className="py-2.5 px-4 font-sans font-medium truncate max-w-xs">{t.name}</td>
                  <td className="py-2.5 px-3 text-slate-400">{formatBytes(t.total_bytes)}</td>
                  <td className="py-2.5 px-3">
                    <div className="flex items-center gap-2">
                      <div className="flex-1 h-1.5 rounded-full bg-slate-950 overflow-hidden">
                        <div
                          className="h-full bg-pink-500 rounded-full"
                          style={{ width: `${Math.round(t.progress * 100)}%` }}
                        ></div>
                      </div>
                      <span className="text-[10px] w-8 text-right">{Math.round(t.progress * 100)}%</span>
                    </div>
                  </td>
                  <td className="py-2.5 px-3">
                    <span
                      className={`px-2 py-0.5 rounded text-[10px] font-semibold uppercase ${
                        t.status === 'Downloading'
                          ? 'bg-pink-950 text-pink-400 border border-pink-800/50'
                          : t.status === 'Seeding'
                          ? 'bg-emerald-950 text-emerald-400 border border-emerald-800/50'
                          : 'bg-slate-800 text-slate-400'
                      }`}
                    >
                      {t.status}
                    </span>
                  </td>
                  <td className="py-2.5 px-3 text-cyan-400">{formatSpeed(t.download_rate)}</td>
                  <td className="py-2.5 px-3 text-amber-400">{formatSpeed(t.upload_rate)}</td>
                  <td className="py-2.5 px-3 text-slate-400">{formatEta(t.eta_seconds)}</td>
                  <td className="py-2.5 px-3">{formatRatio(t.ratio)}</td>
                  <td className="py-2.5 px-3 text-slate-400">{t.seeds}</td>
                  <td className="py-2.5 px-3 text-slate-400">{t.peers}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        {/* Bottom Tabbed Inspector */}
        <div className="h-56 bg-slate-950/90 border-t border-slate-800 flex flex-col">
          <div className="flex border-b border-slate-800 px-4 pt-2 gap-2 text-xs">
            {[
              { id: 'general', label: 'General', icon: Activity },
              { id: 'files', label: 'Files', icon: HardDrive },
              { id: 'trackers', label: 'Trackers', icon: Layers },
              { id: 'peers', label: 'Peers', icon: Users },
            ].map((tab) => {
              const Icon = tab.icon;
              return (
                <button
                  key={tab.id}
                  onClick={() => setActiveManagerTab(tab.id as any)}
                  className={`flex items-center gap-1.5 px-3 py-1.5 rounded-t-lg transition border-b-2 font-medium ${
                    activeManagerTab === tab.id
                      ? 'border-pink-500 text-pink-400 bg-slate-900'
                      : 'border-transparent text-slate-400 hover:text-slate-200'
                  }`}
                >
                  <Icon className="w-3.5 h-3.5" />
                  <span>{tab.label}</span>
                </button>
              );
            })}
          </div>

          <div className="flex-1 p-4 overflow-y-auto text-xs text-slate-300">
            {selectedTorrent ? (
              activeManagerTab === 'general' ? (
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4 font-mono">
                  <div>
                    <span className="text-[10px] uppercase text-slate-500 block">Total Size</span>
                    <span className="text-white">{formatBytes(selectedTorrent.total_bytes)}</span>
                  </div>
                  <div>
                    <span className="text-[10px] uppercase text-slate-500 block">Downloaded</span>
                    <span className="text-white">{formatBytes(selectedTorrent.downloaded_bytes)}</span>
                  </div>
                  <div>
                    <span className="text-[10px] uppercase text-slate-500 block">Uploaded</span>
                    <span className="text-white">{formatBytes(selectedTorrent.uploaded_bytes)}</span>
                  </div>
                  <div>
                    <span className="text-[10px] uppercase text-slate-500 block">Share Ratio</span>
                    <span className="text-emerald-400">{formatRatio(selectedTorrent.ratio)}</span>
                  </div>
                </div>
              ) : activeManagerTab === 'files' ? (
                <div className="space-y-1 font-mono text-xs">
                  <div className="p-2 rounded bg-slate-900 flex justify-between items-center">
                    <span>{selectedTorrent.name}</span>
                    <span className="text-slate-500">{formatBytes(selectedTorrent.total_bytes)} · 100% Complete</span>
                  </div>
                </div>
              ) : activeManagerTab === 'trackers' ? (
                <div className="font-mono text-xs space-y-1">
                  <div className="p-2 rounded bg-slate-900 flex justify-between text-slate-400">
                    <span>udp://tracker.opentrackr.org:1337/announce</span>
                    <span className="text-emerald-400 font-semibold">Working (Seeders: {selectedTorrent.seeds})</span>
                  </div>
                </div>
              ) : (
                <div className="font-mono text-xs space-y-1">
                  <div className="p-2 rounded bg-slate-900 flex justify-between text-slate-400">
                    <span>192.0.2.14:51413 [rqbit/v1.0]</span>
                    <span className="text-cyan-400">DL: 1.2 MiB/s · UL: 340 KiB/s</span>
                  </div>
                </div>
              )
            ) : (
              <div className="text-slate-500">No torrent selected</div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
