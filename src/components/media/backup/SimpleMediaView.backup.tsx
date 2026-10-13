import React from 'react';
import { useTorrentStore } from '../../../stores/torrentStore';
import { useUiStore } from '../../../stores/uiStore';
import { formatBytes, formatSpeed } from '../../../lib/formatters';
import { TorrentItem } from '../../../types';
import {
  Play,
  Pause,
  Plus,
  Film,
} from 'lucide-react';

export const SimpleMediaView: React.FC = () => {
  const { torrents, pauseTorrent, resumeTorrent } = useTorrentStore();
  const { setShowAddTorrent } = useUiStore();

  const mediaTorrents = torrents.filter((t: TorrentItem) => t.is_media || t.category === 'Movies');
  const displayItems = mediaTorrents.length > 0 ? mediaTorrents : torrents;

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-6 bg-[#141414] text-white">
      {/* Header Bar */}
      <div className="flex items-center justify-between pb-4 border-b border-[#333333]">
        <div className="flex items-center gap-3">
          <Film className="w-5 h-5 text-[#e50914]" />
          <div>
            <h2 className="text-base font-bold text-white tracking-tight">Media Library</h2>
            <p className="text-xs text-[#a3a3a3]">Instant streaming & sequential downloads</p>
          </div>
        </div>
        <button
          onClick={() => setShowAddTorrent(true)}
          className="flex items-center gap-2 px-4 py-2 rounded-md bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold transition shadow"
        >
          <Plus className="w-4 h-4" />
          <span>Add Media</span>
        </button>
      </div>

      {/* Empty State */}
      {displayItems.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-28 text-center space-y-3">
          <div className="p-4 rounded-full bg-[#1f1f1f] border border-[#333333] text-[#a3a3a3]">
            <Film className="w-10 h-10" />
          </div>
          <h3 className="text-base font-bold text-white">Your Media Library is Empty</h3>
          <p className="text-xs text-[#a3a3a3] max-w-sm">
            Drop any .torrent or .adc file into this window to start streaming or seeding.
          </p>
          <button
            onClick={() => setShowAddTorrent(true)}
            className="flex items-center gap-2 px-5 py-2.5 rounded-md bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold transition"
          >
            <Plus className="w-4 h-4" />
            <span>Add First Video</span>
          </button>
        </div>
      ) : (
        /* Netflix Style Card Grid */
        <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4">
          {displayItems.map((item: TorrentItem) => {
            const progressPct = Math.round(item.progress * 100);
            const isDownloading = item.status === 'Downloading';
            return (
              <div
                key={item.id}
                className="group relative rounded-lg bg-[#1f1f1f] border border-[#333333] hover:border-[#e50914] overflow-hidden flex flex-col justify-between transition"
              >
                {/* 16:9 Thumbnail Poster Area */}
                <div className="aspect-video w-full bg-[#141414] relative flex items-center justify-center overflow-hidden">
                  <Film className="w-12 h-12 text-[#333333] group-hover:text-[#525252] transition" />

                  {/* Hover Overlay Play Action */}
                  <div className="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 flex items-center justify-center gap-3 transition">
                    <button
                      onClick={() => (isDownloading ? pauseTorrent(item.id) : resumeTorrent(item.id))}
                      className="p-3 rounded-full bg-[#e50914] text-white hover:scale-105 transition shadow"
                      title={isDownloading ? 'Pause' : 'Play'}
                    >
                      {isDownloading ? <Pause className="w-5 h-5" /> : <Play className="w-5 h-5 fill-white" />}
                    </button>
                  </div>

                  {/* Status Badge */}
                  <span className="absolute top-2 left-2 text-[10px] font-mono px-2 py-0.5 rounded bg-black/80 border border-[#333333] text-white font-semibold">
                    {item.status}
                  </span>

                  {/* Progress bar at bottom of poster */}
                  <div className="absolute bottom-0 inset-x-0 h-1 bg-[#333333]">
                    <div
                      className="h-full bg-[#e50914] transition-all"
                      style={{ width: `${progressPct}%` }}
                    />
                  </div>
                </div>

                {/* Card Content & Metadata */}
                <div className="p-3 space-y-2">
                  <h4 className="text-xs font-semibold text-white truncate" title={item.name}>
                    {item.name}
                  </h4>
                  <div className="flex items-center justify-between text-[11px] font-mono text-[#a3a3a3]">
                    <span>{formatBytes(item.total_bytes)}</span>
                    <span>
                      {isDownloading ? formatSpeed(item.download_rate) : `${progressPct}%`}
                    </span>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};

export default SimpleMediaView;
