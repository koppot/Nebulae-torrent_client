import React, { useEffect } from 'react';
import { useTorrentStore } from '../../stores/torrentStore';
import { useUiStore } from '../../stores/uiStore';
import { usePeeringStore } from '../../stores/peeringStore';
import { formatBytes, formatSpeed, formatEta, formatRatio } from '../../lib/formatters';
import {
  Play,
  Pause,
  Trash2,
  Plus,
  Search,
  HardDrive,
  Users,
  Activity,
  Layers,
  FileText,
} from 'lucide-react';

export const TorrentManagerView: React.FC = () => {
  const {
    torrents,
    selectedId,
    setSelectedId,
    pauseTorrent,
    resumeTorrent,
    removeTorrent,
  } = useTorrentStore();

  const {
    activeManagerTab,
    setActiveManagerTab,
    managerCategoryFilter,
    setManagerCategoryFilter,
    searchQuery,
    setSearchQuery,
    setShowAddTorrent,
  } = useUiStore();

  const { peers, fetchPeers } = usePeeringStore();

  useEffect(() => {
    if (selectedId) {
      fetchPeers(selectedId);
    }
  }, [selectedId, fetchPeers]);

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

  const handleResumeSelected = () => {
    if (selectedTorrent) resumeTorrent(selectedTorrent.id);
  };

  const handlePauseSelected = () => {
    if (selectedTorrent) pauseTorrent(selectedTorrent.id);
  };

  const handleRemoveSelected = () => {
    if (selectedTorrent) removeTorrent(selectedTorrent.id);
  };

  return (
    <div className="flex-1 flex overflow-hidden font-sans select-none bg-[#141414] text-white">
      {/* 1. Left Sidebar: ruTorrent / qBittorrent Categories */}
      <aside className="w-52 bg-[#141414] border-r border-[#333333] flex flex-col p-2 space-y-4 text-xs">
        <div>
          <span className="text-[10px] font-bold uppercase tracking-wider text-[#a3a3a3] px-2 block mb-1">
            Transfers
          </span>
          <div className="space-y-0.5">
            {[
              { id: 'all', label: 'All Torrents', count: torrents.length },
              { id: 'downloading', label: 'Downloading', count: torrents.filter((t) => t.status === 'Downloading').length },
              { id: 'seeding', label: 'Seeding', count: torrents.filter((t) => t.status === 'Seeding').length },
              { id: 'completed', label: 'Completed', count: torrents.filter((t) => t.status === 'Completed').length },
              { id: 'paused', label: 'Paused', count: torrents.filter((t) => t.status === 'Paused').length },
            ].map((item) => (
              <button
                key={item.id}
                onClick={() => setManagerCategoryFilter(item.id)}
                className={`w-full flex items-center justify-between px-2.5 py-1.5 rounded text-left transition ${
                  managerCategoryFilter === item.id
                    ? 'bg-[#e50914] text-white font-semibold'
                    : 'text-[#a3a3a3] hover:text-white hover:bg-[#1f1f1f]'
                }`}
              >
                <span>{item.label}</span>
                <span className="text-[10px] font-mono opacity-80">{item.count}</span>
              </button>
            ))}
          </div>
        </div>

        <div>
          <span className="text-[10px] font-bold uppercase tracking-wider text-[#a3a3a3] px-2 block mb-1">
            Categories
          </span>
          <div className="space-y-0.5">
            {[
              { id: 'media', label: 'Video & Cinema', count: torrents.filter((t) => t.is_media).length },
            ].map((item) => (
              <button
                key={item.id}
                onClick={() => setManagerCategoryFilter(item.id)}
                className={`w-full flex items-center justify-between px-2.5 py-1.5 rounded text-left transition ${
                  managerCategoryFilter === item.id
                    ? 'bg-[#e50914] text-white font-semibold'
                    : 'text-[#a3a3a3] hover:text-white hover:bg-[#1f1f1f]'
                }`}
              >
                <span>{item.label}</span>
                <span className="text-[10px] font-mono opacity-80">{item.count}</span>
              </button>
            ))}
          </div>
        </div>
      </aside>

      {/* 2. Main Right Area: Action Toolbar + Table + Tabbed Inspector */}
      <div className="flex-1 flex flex-col overflow-hidden bg-[#141414]">
        {/* Top Action Toolbar */}
        <div className="h-10 border-b border-[#333333] bg-[#1f1f1f] flex items-center justify-between px-3">
          <div className="flex items-center gap-1">
            <button
              onClick={() => setShowAddTorrent(true)}
              className="p-1.5 rounded hover:bg-[#262626] text-white hover:text-[#e50914] transition"
              title="Add Torrent"
            >
              <Plus className="w-4 h-4" />
            </button>
            <button
              onClick={handleResumeSelected}
              disabled={!selectedTorrent}
              className="p-1.5 rounded hover:bg-[#262626] text-white hover:text-[#e50914] disabled:opacity-30 transition"
              title="Resume Selected"
            >
              <Play className="w-4 h-4" />
            </button>
            <button
              onClick={handlePauseSelected}
              disabled={!selectedTorrent}
              className="p-1.5 rounded hover:bg-[#262626] text-white hover:text-[#e50914] disabled:opacity-30 transition"
              title="Pause Selected"
            >
              <Pause className="w-4 h-4" />
            </button>
            <button
              onClick={handleRemoveSelected}
              disabled={!selectedTorrent}
              className="p-1.5 rounded hover:bg-[#262626] text-white hover:text-[#e50914] disabled:opacity-30 transition"
              title="Delete Selected"
            >
              <Trash2 className="w-4 h-4" />
            </button>
          </div>

          {/* Search Input */}
          <div className="relative w-56">
            <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-[#a3a3a3]" />
            <input
              type="text"
              placeholder="Search transfers..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full pl-8 pr-2.5 py-1 bg-[#141414] border border-[#333333] rounded text-xs text-white placeholder:text-[#a3a3a3] focus:outline-none focus:border-[#e50914]"
            />
          </div>
        </div>

        {/* Data Table: Classic ruTorrent Grid */}
        <div className="flex-1 overflow-auto">
          <table className="w-full text-left border-collapse text-xs">
            <thead className="sticky top-0 bg-[#1f1f1f] border-b border-[#333333] text-[11px] font-semibold text-[#a3a3a3] uppercase font-mono tracking-wider">
              <tr>
                <th className="py-2 px-3">Name</th>
                <th className="py-2 px-2.5 w-20">Size</th>
                <th className="py-2 px-2.5 w-36">Progress</th>
                <th className="py-2 px-2.5 w-24">Status</th>
                <th className="py-2 px-2.5 w-24">Down Speed</th>
                <th className="py-2 px-2.5 w-24">Up Speed</th>
                <th className="py-2 px-2.5 w-20">ETA</th>
                <th className="py-2 px-2.5 w-16">Ratio</th>
                <th className="py-2 px-2.5 w-14">Seeds</th>
                <th className="py-2 px-2.5 w-14">Peers</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[#262626] font-mono">
              {filteredTorrents.length === 0 && (
                <tr>
                  <td colSpan={10} className="py-24 text-center">
                    <div className="flex flex-col items-center justify-center space-y-2">
                      <HardDrive className="w-8 h-8 text-[#333333]" />
                      <p className="text-sm font-sans font-medium text-white">No torrents in list</p>
                      <p className="text-xs font-sans text-[#a3a3a3]">
                        Drop a <span className="text-white font-mono">.torrent</span> file or click <span className="text-[#e50914] font-semibold">+ Add</span> to begin
                      </p>
                    </div>
                  </td>
                </tr>
              )}
              {filteredTorrents.map((t) => {
                const isSelected = selectedId === t.id;
                const progressPct = Math.round(t.progress * 100);
                return (
                  <tr
                    key={t.id}
                    onClick={() => setSelectedId(t.id)}
                    className={`cursor-pointer transition select-none ${
                      isSelected
                        ? 'bg-[#1f1f1f] border-l-2 border-[#e50914] text-white'
                        : 'hover:bg-[#1a1a1a] text-[#a3a3a3] hover:text-white'
                    }`}
                  >
                    <td className="py-2 px-3 font-sans font-medium truncate max-w-xs text-white">
                      {t.name}
                    </td>
                    <td className="py-2 px-2.5">{formatBytes(t.total_bytes)}</td>
                    <td className="py-2 px-2.5">
                      <div className="flex items-center gap-2">
                        <div className="flex-1 h-1.5 rounded-full bg-[#333333] overflow-hidden">
                          <div
                            className="h-full bg-[#e50914] rounded-full transition-all"
                            style={{ width: `${progressPct}%` }}
                          />
                        </div>
                        <span className="text-[10px] w-8 text-right font-mono text-white">{progressPct}%</span>
                      </div>
                    </td>
                    <td className="py-2 px-2.5">
                      <span className="text-[11px] font-sans text-white">
                        {t.status}
                      </span>
                    </td>
                    <td className="py-2 px-2.5 text-white">{formatSpeed(t.download_rate)}</td>
                    <td className="py-2 px-2.5">{formatSpeed(t.upload_rate)}</td>
                    <td className="py-2 px-2.5">{formatEta(t.eta_seconds)}</td>
                    <td className="py-2 px-2.5 text-white">{formatRatio(t.ratio)}</td>
                    <td className="py-2 px-2.5">{t.seeds}</td>
                    <td className="py-2 px-2.5">{t.peers}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>

        {/* 3. Bottom Inspector: Classic ruTorrent Tabs */}
        <div className="h-56 bg-[#1f1f1f] border-t border-[#333333] flex flex-col">
          {/* Tab Headers */}
          <div className="flex border-b border-[#333333] px-3 pt-1.5 gap-1 text-xs">
            {[
              { id: 'general', label: 'General', icon: Activity },
              { id: 'files', label: 'Files', icon: FileText },
              { id: 'trackers', label: 'Trackers', icon: Layers },
              { id: 'peers', label: 'Peers', icon: Users },
            ].map((tab) => {
              const Icon = tab.icon;
              const isActive = activeManagerTab === tab.id;
              return (
                <button
                  key={tab.id}
                  onClick={() => setActiveManagerTab(tab.id as any)}
                  className={`flex items-center gap-1.5 px-3 py-1.5 rounded-t font-medium transition ${
                    isActive
                      ? 'border-b-2 border-[#e50914] text-white bg-[#141414]'
                      : 'text-[#a3a3a3] hover:text-white'
                  }`}
                >
                  <Icon className="w-3.5 h-3.5" />
                  <span>{tab.label}</span>
                </button>
              );
            })}
          </div>

          {/* Tab Panels */}
          <div className="flex-1 p-3 overflow-y-auto text-xs bg-[#141414]">
            {selectedTorrent ? (
              activeManagerTab === 'general' ? (
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4 font-mono text-xs">
                  <div className="space-y-1">
                    <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">Size</span>
                    <span className="text-white">{formatBytes(selectedTorrent.total_bytes)}</span>
                  </div>
                  <div className="space-y-1">
                    <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">Downloaded</span>
                    <span className="text-white">{formatBytes(selectedTorrent.downloaded_bytes)}</span>
                  </div>
                  <div className="space-y-1">
                    <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">Uploaded</span>
                    <span className="text-white">{formatBytes(selectedTorrent.uploaded_bytes)}</span>
                  </div>
                  <div className="space-y-1">
                    <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">Share Ratio</span>
                    <span className="text-white font-bold">{formatRatio(selectedTorrent.ratio)}</span>
                  </div>
                </div>
              ) : activeManagerTab === 'files' ? (
                <div className="space-y-1 font-mono text-xs">
                  <div className="p-2 rounded bg-[#1f1f1f] border border-[#333333] flex justify-between items-center">
                    <span className="text-white truncate font-sans">{selectedTorrent.name}</span>
                    <span className="text-[#a3a3a3]">{formatBytes(selectedTorrent.total_bytes)} · Complete</span>
                  </div>
                </div>
              ) : activeManagerTab === 'trackers' ? (
                <div className="font-mono text-xs space-y-1.5">
                  <div className="p-2 rounded bg-[#1f1f1f] border border-[#333333] flex justify-between text-xs">
                    <span className="text-[#a3a3a3]">https://tracker.asiandvdclub.org:2710/announce</span>
                    <span className="text-white font-semibold">Announce OK</span>
                  </div>
                  <div className="p-2 rounded bg-[#1f1f1f] border border-[#333333] flex justify-between text-xs">
                    <span className="text-[#a3a3a3]">udp://tracker.opentrackr.org:1337/announce</span>
                    <span className="text-white font-semibold">Peers: {selectedTorrent.peers}</span>
                  </div>
                </div>
              ) : (
                /* Peers Tab */
                <div className="overflow-x-auto border border-[#333333] rounded">
                  <table className="w-full text-left border-collapse text-xs">
                    <thead className="bg-[#1f1f1f] border-b border-[#333333] text-[10px] uppercase font-mono text-[#a3a3a3]">
                      <tr>
                        <th className="py-1.5 px-2.5">Client</th>
                        <th className="py-1.5 px-2.5">IP & Port</th>
                        <th className="py-1.5 px-2.5">Down Speed</th>
                        <th className="py-1.5 px-2.5">Up Speed</th>
                        <th className="py-1.5 px-2.5">Score</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-[#262626] font-mono text-[11px]">
                      {peers.length === 0 ? (
                        <tr>
                          <td colSpan={5} className="py-4 text-center text-[#a3a3a3]">No active peers connected</td>
                        </tr>
                      ) : (
                        peers.map((p) => (
                          <tr key={p.peer_id} className="hover:bg-[#1f1f1f]">
                            <td className="py-1.5 px-2.5 text-white font-sans">{p.client_name}</td>
                            <td className="py-1.5 px-2.5 text-[#a3a3a3]">{p.ip}:{p.port}</td>
                            <td className="py-1.5 px-2.5 text-white">{formatSpeed(p.sustained_dl_bps)}</td>
                            <td className="py-1.5 px-2.5 text-[#a3a3a3]">{formatSpeed(p.sustained_ul_bps)}</td>
                            <td className="py-1.5 px-2.5 text-white font-semibold">{(p.stability_score * 100).toFixed(0)}%</td>
                          </tr>
                        ))
                      )}
                    </tbody>
                  </table>
                </div>
              )
            ) : (
              <div className="text-[#a3a3a3] text-xs">No torrent selected</div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default TorrentManagerView;
