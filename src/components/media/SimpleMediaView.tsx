import React from 'react';
import { useTorrentStore } from '../../stores/torrentStore';
import { useUiStore } from '../../stores/uiStore';
import { formatBytes, formatSpeed, formatEta } from '../../lib/formatters';
import {
  Play,
  FolderOpen,
  ArrowDownCircle,
  ArrowUpCircle,
  Clock,
  Sparkles,
  FilePlus,
  CheckCircle2,
} from 'lucide-react';

export const SimpleMediaView: React.FC = () => {
  const { torrents } = useTorrentStore();
  const { setShowAddTorrent } = useUiStore();

  const activeDownloads = torrents.filter((t) => t.status === 'Downloading');
  const completed = torrents.filter((t) => t.status === 'Completed' || t.status === 'Seeding');
  const readyToWatch = torrents.filter((t) => t.is_media && t.progress > 0.1);

  return (
    <div className="flex-1 overflow-y-auto p-8 space-y-10">
      {/* Hero Welcome / Quick Add Shelf */}
      <div className="relative overflow-hidden rounded-2xl bg-gradient-to-r from-adc-crimson/30 via-black/80 to-black/90 border border-adc-crimson/30 p-6 flex items-center justify-between shadow-adc-glass backdrop-blur-md">
        <div className="space-y-1 z-10 max-w-xl">
          <div className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full bg-adc-crimson/30 border border-adc-crimsonBright/40 text-adc-ivory text-[11px] font-semibold mb-2">
            <Sparkles className="w-3.5 h-3.5 text-adc-gold" />
            <span>Simple Media Mode</span>
          </div>
          <h2 className="text-xl font-bold text-adc-ivory tracking-tight">
            Media Deliveries & Playback
          </h2>
          <p className="text-xs text-white/70 leading-relaxed">
            Drag and drop .torrent files anywhere onto this window, paste magnet links, or explore your downloads.
          </p>
        </div>
        <button
          onClick={() => setShowAddTorrent(true)}
          className="z-10 adc-btn-crimson flex items-center gap-2 px-5 py-2.5 rounded-xl text-xs font-semibold"
        >
          <FilePlus className="w-4 h-4" />
          <span>Add Media Torrent</span>
        </button>
      </div>

      {/* Shelf 1: Active Downloads */}
      <section className="space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <ArrowDownCircle className="w-4 h-4 text-pink-400" />
            <h3 className="text-sm font-bold text-white uppercase tracking-wider">Active Downloads</h3>
            <span className="text-xs font-mono text-slate-500">({activeDownloads.length})</span>
          </div>
        </div>

        {activeDownloads.length === 0 ? (
          <div className="border border-dashed border-slate-800 rounded-xl p-8 text-center text-slate-500 text-xs">
            No active downloads at this moment. Drag and drop a .torrent file to begin.
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-5">
            {activeDownloads.map((torrent) => (
              <div
                key={torrent.id}
                className="group relative overflow-hidden rounded-xl bg-black/60 border border-white/10 hover:border-adc-crimson/50 p-4 transition-all shadow-adc-glass backdrop-blur-md flex flex-col justify-between"
              >
                <div>
                  <div className="flex items-start justify-between gap-3 mb-2">
                    <h4 className="text-xs font-semibold text-adc-ivory truncate flex-1 group-hover:text-adc-crimsonGlow transition">
                      {torrent.name}
                    </h4>
                    <span className="text-[10px] uppercase font-mono px-2 py-0.5 rounded bg-adc-crimson/40 border border-adc-crimsonBright/50 text-adc-ivory font-semibold">
                      {Math.round(torrent.progress * 100)}%
                    </span>
                  </div>

                  <div className="flex items-center gap-3 text-[11px] text-white/70 font-mono mb-3">
                    <span className="flex items-center gap-1 text-adc-ice">
                      <ArrowDownCircle className="w-3.5 h-3.5" />
                      {formatSpeed(torrent.download_rate)}
                    </span>
                    <span className="flex items-center gap-1">
                      <Clock className="w-3.5 h-3.5 text-white/50" />
                      ETA: {formatEta(torrent.eta_seconds)}
                    </span>
                  </div>

                  {/* Progress Bar (ADC Crimson to Ice glow) */}
                  <div className="w-full h-2 rounded-full bg-white/10 overflow-hidden mb-3">
                    <div
                      className="h-full bg-gradient-to-r from-adc-crimsonBright via-adc-crimsonGlow to-adc-ice rounded-full transition-all"
                      style={{ width: `${Math.round(torrent.progress * 100)}%` }}
                    ></div>
                  </div>
                </div>

                <div className="flex items-center justify-between text-[11px] text-white/50 border-t border-white/10 pt-2 font-mono">
                  <span>
                    {formatBytes(torrent.downloaded_bytes)} of {formatBytes(torrent.total_bytes)}
                  </span>
                  <span>Peers: {torrent.peers}</span>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

      {/* Shelf 2: Ready to Watch */}
      <section className="space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Play className="w-4 h-4 text-adc-crimsonGlow" />
            <h3 className="text-sm font-bold text-adc-ivory uppercase tracking-wider">Ready to Watch</h3>
            <span className="text-xs font-mono text-white/50">({readyToWatch.length})</span>
          </div>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-5">
          {readyToWatch.map((torrent) => (
            <div
              key={`watch-${torrent.id}`}
              className="group relative rounded-xl bg-black/60 border border-white/10 hover:border-adc-crimson/50 p-4 transition-all shadow-adc-glass backdrop-blur-md flex flex-col justify-between"
            >
              <div>
                <div className="aspect-video w-full rounded-lg bg-black border border-white/10 flex items-center justify-center mb-3 relative overflow-hidden group-hover:border-adc-crimson/50 transition">
                  <div className="absolute inset-0 bg-gradient-to-t from-black via-transparent to-transparent opacity-80"></div>
                  <Play className="w-10 h-10 text-adc-crimsonBright group-hover:scale-110 transition drop-shadow" />
                  <span className="absolute bottom-2 left-2 text-[10px] font-mono uppercase px-2 py-0.5 rounded bg-black/80 text-white/80 border border-white/10">
                    {torrent.status}
                  </span>
                </div>
                <h4 className="text-xs font-semibold text-adc-ivory truncate mb-1">
                  {torrent.name}
                </h4>
                <p className="text-[11px] text-white/60 font-mono">
                  Size: {formatBytes(torrent.total_bytes)} · Ratio: {torrent.ratio.toFixed(2)}
                </p>
              </div>

              <div className="flex items-center gap-2 mt-4 pt-3 border-t border-white/10">
                <button className="flex-1 adc-btn-crimson flex items-center justify-center gap-1.5 py-1.5 rounded-lg text-xs font-semibold">
                  <Play className="w-3.5 h-3.5" />
                  <span>Play Media</span>
                </button>
                <button className="adc-btn-glass p-2 rounded-lg text-white/80 hover:text-white" title="Open Destination Folder">
                  <FolderOpen className="w-4 h-4" />
                </button>
              </div>
            </div>
          ))}
        </div>
      </section>

      {/* Shelf 3: Completed & Seeding */}
      <section className="space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <CheckCircle2 className="w-4 h-4 text-adc-emerald" />
            <h3 className="text-sm font-bold text-adc-ivory uppercase tracking-wider">Completed & Seeding</h3>
            <span className="text-xs font-mono text-white/50">({completed.length})</span>
          </div>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-5">
          {completed.map((torrent) => (
            <div
              key={`done-${torrent.id}`}
              className="rounded-xl bg-black/60 border border-white/10 p-4 transition-all shadow-adc-glass backdrop-blur-md flex items-center justify-between gap-4"
            >
              <div className="flex-1 truncate">
                <h4 className="text-xs font-semibold text-adc-ivory truncate mb-1">
                  {torrent.name}
                </h4>
                <div className="flex items-center gap-3 text-[11px] text-white/60 font-mono">
                  <span>{formatBytes(torrent.total_bytes)}</span>
                  <span className="text-adc-emerald flex items-center gap-1">
                    <ArrowUpCircle className="w-3 h-3" />
                    Ratio: {torrent.ratio.toFixed(2)}
                  </span>
                </div>
              </div>
              <button className="adc-btn-glass p-2 rounded-lg text-white/80 hover:text-white" title="Open in folder">
                <FolderOpen className="w-4 h-4" />
              </button>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
};
