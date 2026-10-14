import React from 'react';
import { useTorrentStore } from '../../stores/torrentStore';
import { useUiStore } from '../../stores/uiStore';
import { formatBytes, formatSpeed, formatEta, formatRatio } from '../../lib/formatters';
import { TorrentItem } from '../../types';
import {
  Play,
  Pause,
  Plus,
  Film,
  FolderOpen,
  ArrowDownCircle,
  CheckCircle2,
  Eye,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';

export const SimpleMediaView: React.FC = () => {
  const { torrents, pauseTorrent, resumeTorrent } = useTorrentStore();
  const { setShowAddTorrent, setViewMode } = useUiStore();

  const activeDownloads = torrents.filter((t) => t.status === 'Downloading');
  const readyToWatch = torrents.filter(
    (t) => (t.is_media || t.category === 'Movies') && (t.progress > 0.05 || t.status === 'Seeding' || t.status === 'Completed')
  );
  const completedSeeding = torrents.filter((t) => t.status === 'Seeding' || t.status === 'Completed');

  const handlePlayMedia = async (torrent: TorrentItem) => {
    // If torrent has files, find the largest media file
    const mediaFile = torrent.files.find((f) => {
      const lower = f.name.toLowerCase();
      return (
        lower.endsWith('.mkv') ||
        lower.endsWith('.mp4') ||
        lower.endsWith('.avi') ||
        lower.endsWith('.iso') ||
        lower.endsWith('.mov') ||
        lower.endsWith('.ts')
      );
    });

    const targetPath = mediaFile
      ? `${torrent.save_path}/${mediaFile.name}`
      : torrent.save_path;

    try {
      await invoke('launch_media_player', { filePath: targetPath });
    } catch {
      // If launch fails, fallback to revealing in folder
      invoke('open_torrent_folder', { path: torrent.save_path }).catch(() => {});
    }
  };

  const handleOpenFolder = (path: string) => {
    invoke('open_torrent_folder', { path }).catch(() => {});
  };

  const renderMediaCard = (item: TorrentItem, variant: 'active' | 'watch' | 'seeding') => {
    const progressPct = Math.round(item.progress * 100);
    const isDownloading = item.status === 'Downloading';

    return (
      <div
        key={item.id}
        className="group relative flex-shrink-0 w-64 rounded-xl bg-adc-cardDark border border-adc-cardBorderDark hover:border-adc-accent/60 overflow-hidden flex flex-col justify-between transition-all duration-200 shadow-lg hover:shadow-adc-glow"
      >
        {/* 16:9 Thumbnail Poster Area */}
        <div className="aspect-video w-full bg-adc-dark relative flex items-center justify-center overflow-hidden">
          {item.poster_url ? (
            <img src={item.poster_url} alt={item.name} className="w-full h-full object-cover" />
          ) : (
            <Film className="w-12 h-12 text-adc-cardBorderDark group-hover:text-adc-accent/60 transition" />
          )}

          {/* Hover Overlay Play / Inspect Actions */}
          <div className="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 flex items-center justify-center gap-3 transition backdrop-blur-xs">
            <button
              onClick={() => handlePlayMedia(item)}
              className="p-3 rounded-full bg-adc-accent hover:bg-adc-accent/90 text-white hover:scale-105 transition shadow-lg"
              title="Play Video in Default Player"
            >
              <Play className="w-5 h-5 fill-white ml-0.5" />
            </button>
            <button
              onClick={() => (isDownloading ? pauseTorrent(item.id) : resumeTorrent(item.id))}
              className="p-2.5 rounded-full bg-adc-dark/90 hover:bg-adc-cardDark text-white border border-adc-cardBorderDark transition"
              title={isDownloading ? 'Pause' : 'Resume'}
            >
              {isDownloading ? <Pause className="w-4 h-4" /> : <Play className="w-4 h-4 fill-white" />}
            </button>
            <button
              onClick={() => handleOpenFolder(item.save_path)}
              className="p-2.5 rounded-full bg-adc-dark/90 hover:bg-adc-cardDark text-white border border-adc-cardBorderDark transition"
              title="Open Enclosing Folder"
            >
              <FolderOpen className="w-4 h-4" />
            </button>
          </div>

          {/* Status Badge */}
          <span className="absolute top-2 left-2 text-[10px] font-mono px-2 py-0.5 rounded bg-black/80 border border-adc-cardBorderDark text-white font-semibold">
            {item.status}
          </span>

          {/* Progress bar at bottom of poster */}
          <div className="absolute bottom-0 inset-x-0 h-1 bg-adc-cardBorderDark">
            <div
              className={`h-full transition-all duration-300 ${
                item.status === 'Seeding' ? 'bg-adc-emerald' : 'bg-adc-accent'
              }`}
              style={{ width: `${progressPct}%` }}
            />
          </div>
        </div>

        {/* Card Content & Metadata */}
        <div className="p-3.5 space-y-2">
          <h4 className="text-xs font-semibold text-white truncate" title={item.name}>
            {item.name}
          </h4>

          <div className="flex items-center justify-between text-[11px] font-mono text-slate-400">
            <span>{formatBytes(item.total_bytes)}</span>
            {variant === 'active' && (
              <span className="text-adc-cyan font-semibold">
                {formatSpeed(item.download_rate)}
              </span>
            )}
            {variant === 'watch' && (
              <span className="text-adc-gold font-medium">
                {progressPct >= 100 ? 'Ready' : `${progressPct}% ready`}
              </span>
            )}
            {variant === 'seeding' && (
              <span className="text-adc-emerald font-semibold">
                Ratio {formatRatio(item.ratio)}
              </span>
            )}
          </div>

          {variant === 'active' && item.eta_seconds != null && (
            <div className="text-[10px] text-slate-500 font-mono flex justify-between">
              <span>ETA: {formatEta(item.eta_seconds)}</span>
              <span>{progressPct}%</span>
            </div>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-8 bg-adc-dark text-white select-none">
      {/* Header Bar */}
      <div className="flex items-center justify-between pb-4 border-b border-adc-cardBorderDark">
        <div className="flex items-center gap-3">
          <Film className="w-6 h-6 text-adc-accent" />
          <div>
            <h2 className="text-base font-bold text-white tracking-tight">Media Library</h2>
            <p className="text-xs text-slate-400">Instant streaming & sequential downloads for Asian cinema</p>
          </div>
        </div>
        <div className="flex items-center gap-2.5">
          <button
            onClick={() => setViewMode('manager')}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-adc-cardBorderDark bg-adc-cardDark hover:bg-adc-cardDark/80 text-white text-xs font-medium transition"
          >
            <Eye className="w-3.5 h-3.5 text-adc-accent" />
            <span>Open Torrent Manager</span>
          </button>
          <button
            onClick={() => setShowAddTorrent(true)}
            className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg bg-adc-accent hover:bg-adc-accent/90 text-white text-xs font-semibold transition shadow-md"
          >
            <Plus className="w-4 h-4" />
            <span>Add Media</span>
          </button>
        </div>
      </div>

      {torrents.length === 0 ? (
        /* Empty State */
        <div className="flex flex-col items-center justify-center py-24 text-center space-y-4">
          <div className="p-5 rounded-2xl bg-adc-cardDark border border-adc-cardBorderDark text-slate-400">
            <Film className="w-12 h-12 text-adc-accent" />
          </div>
          <div className="space-y-1">
            <h3 className="text-base font-bold text-white">Your Media Library is Empty</h3>
            <p className="text-xs text-slate-400 max-w-sm">
              Drop any .torrent, .adc, or magnet link into this window to start streaming or seeding.
            </p>
          </div>
          <button
            onClick={() => setShowAddTorrent(true)}
            className="flex items-center gap-2 px-5 py-2.5 rounded-lg bg-adc-accent hover:bg-adc-accent/90 text-white text-xs font-semibold transition shadow-lg"
          >
            <Plus className="w-4 h-4" />
            <span>Add First Video</span>
          </button>
        </div>
      ) : (
        <div className="space-y-8">
          {/* Shelf 1: Active Downloads */}
          {activeDownloads.length > 0 && (
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <ArrowDownCircle className="w-4 h-4 text-adc-cyan" />
                <h3 className="text-sm font-bold text-white">Active Downloads</h3>
                <span className="text-[11px] font-mono text-slate-400 px-2 py-0.5 rounded-full bg-adc-cardDark border border-adc-cardBorderDark">
                  {activeDownloads.length}
                </span>
              </div>
              <div className="flex gap-4 overflow-x-auto pb-2 scrollbar-thin">
                {activeDownloads.map((t) => renderMediaCard(t, 'active'))}
              </div>
            </div>
          )}

          {/* Shelf 2: Ready to Watch */}
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <Play className="w-4 h-4 text-adc-gold fill-current" />
              <h3 className="text-sm font-bold text-white">Ready to Watch</h3>
              <span className="text-[11px] font-mono text-slate-400 px-2 py-0.5 rounded-full bg-adc-cardDark border border-adc-cardBorderDark">
                {readyToWatch.length}
              </span>
            </div>
            {readyToWatch.length === 0 ? (
              <p className="text-xs text-slate-500 italic">No media files downloaded yet. Completed videos will appear here.</p>
            ) : (
              <div className="flex gap-4 overflow-x-auto pb-2 scrollbar-thin">
                {readyToWatch.map((t) => renderMediaCard(t, 'watch'))}
              </div>
            )}
          </div>

          {/* Shelf 3: Completed & Seeding */}
          {completedSeeding.length > 0 && (
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <CheckCircle2 className="w-4 h-4 text-adc-emerald" />
                <h3 className="text-sm font-bold text-white">Completed & Seeding</h3>
                <span className="text-[11px] font-mono text-slate-400 px-2 py-0.5 rounded-full bg-adc-cardDark border border-adc-cardBorderDark">
                  {completedSeeding.length}
                </span>
              </div>
              <div className="flex gap-4 overflow-x-auto pb-2 scrollbar-thin">
                {completedSeeding.map((t) => renderMediaCard(t, 'seeding'))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
};

export default SimpleMediaView;
