import React, { useState, useEffect } from 'react';
import { useTorrentStore } from '../../stores/torrentStore';
import { useUiStore } from '../../stores/uiStore';
import { formatBytes, formatSpeed, formatEta, formatRatio } from '../../lib/formatters';
import {
  TorrentItem,
  DiscStructureInfo,
  VolumeSpaceInfo,
  CacheCleanupReport,
  OrganizeMediaReport,
} from '../../types';
import {
  Play,
  Pause,
  Plus,
  FolderOpen,
  ArrowDownCircle,
  CheckCircle2,
  Eye,
  Disc,
  HardDrive,
  Trash2,
  Layers,
  FolderArchive,
  X,
  Sparkles,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';

interface MediaGroup {
  id: string;
  clean_title: string;
  year?: number;
  is_boxset: boolean;
  total_bytes: number;
  overall_progress: number;
  items: TorrentItem[];
  active_index: number;
}

export const SimpleMediaView: React.FC = () => {
  const { torrents, pauseTorrent, resumeTorrent } = useTorrentStore();
  const { setShowAddTorrent, setViewMode } = useUiStore();

  const [discMetaMap, setDiscMetaMap] = useState<Record<string, DiscStructureInfo | null>>({});
  const [volumeSpace, setVolumeSpace] = useState<VolumeSpaceInfo | null>(null);
  const [selectedDiscs, setSelectedDiscs] = useState<Record<string, number>>({});
  const [groupBoxsets, setGroupBoxsets] = useState<boolean>(true);

  // Modals state
  const [showCleanerModal, setShowCleanerModal] = useState<boolean>(false);
  const [cleanerDays, setCleanerDays] = useState<number>(30);
  const [cleaningReport, setCleaningReport] = useState<CacheCleanupReport | null>(null);
  const [isCleaning, setIsCleaning] = useState<boolean>(false);

  const [organizeTargetItem, setOrganizeTargetItem] = useState<TorrentItem | null>(null);
  const [organizeDestPath, setOrganizeDestPath] = useState<string>('');
  const [organizeReport, setOrganizeReport] = useState<OrganizeMediaReport | null>(null);
  const [isOrganizing, setIsOrganizing] = useState<boolean>(false);

  // Query disk space info on mount
  useEffect(() => {
    invoke<VolumeSpaceInfo>('get_volume_disk_space')
      .then((space) => setVolumeSpace(space))
      .catch(() => {});
  }, []);

  // Inspect optical disc format for each media item
  useEffect(() => {
    torrents.forEach((t) => {
      if (!discMetaMap[t.id]) {
        invoke<DiscStructureInfo | null>('detect_disc_format', { filePath: t.save_path })
          .then((info) => {
            if (info) {
              setDiscMetaMap((prev) => ({ ...prev, [t.id]: info }));
            }
          })
          .catch(() => {});
      }
    });
  }, [torrents, discMetaMap]);

  const activeDownloads = torrents.filter((t) => t.status === 'Downloading');
  const readyToWatch = torrents.filter(
    (t) =>
      (t.is_media || t.category === 'Movies') &&
      (t.progress > 0.05 || t.status === 'Seeding' || t.status === 'Completed')
  );
  const completedSeeding = torrents.filter(
    (t) => t.status === 'Seeding' || t.status === 'Completed'
  );

  const handlePlayDisc = async (torrent: TorrentItem, preferMenu: boolean = true) => {
    try {
      await invoke('launch_disc_player', {
        filePath: torrent.save_path,
        preferMenu,
      });
    } catch {
      // Fallback: reveal in enclosing folder
      invoke('open_torrent_folder', { path: torrent.save_path }).catch(() => {});
    }
  };

  const handleOpenFolder = (path: string) => {
    invoke('open_torrent_folder', { path }).catch(() => {});
  };

  const handleCleanCache = async (dryRun: boolean = false) => {
    setIsCleaning(true);
    try {
      const rep = await invoke<CacheCleanupReport>('clean_cache_files', {
        olderThanDays: cleanerDays,
        dryRun,
      });
      setCleaningReport(rep);
      // Refresh disk space info
      invoke<VolumeSpaceInfo>('get_volume_disk_space')
        .then((s) => setVolumeSpace(s))
        .catch(() => {});
    } catch (e) {
      console.error(e);
    } finally {
      setIsCleaning(false);
    }
  };

  const handleExecuteOrganize = async () => {
    if (!organizeTargetItem || !organizeDestPath.trim()) return;
    setIsOrganizing(true);
    try {
      const rep = await invoke<OrganizeMediaReport>('organize_completed_media', {
        sourcePath: organizeTargetItem.save_path,
        destinationRoot: organizeDestPath.trim(),
      });
      setOrganizeReport(rep);
    } catch (err: any) {
      setOrganizeReport({
        source_path: organizeTargetItem.save_path,
        destination_path: organizeDestPath,
        files_moved: 0,
        bytes_moved: 0,
        success: false,
        message: String(err),
      });
    } finally {
      setIsOrganizing(false);
    }
  };

  // Group items into boxsets
  const groupItems = (items: TorrentItem[]): MediaGroup[] => {
    if (!groupBoxsets) {
      return items.map((item) => ({
        id: item.id,
        clean_title: item.media_meta?.clean_title || item.name,
        year: item.media_meta?.year,
        is_boxset: false,
        total_bytes: item.total_bytes,
        overall_progress: item.progress,
        items: [item],
        active_index: 0,
      }));
    }

    const clusters: Record<string, TorrentItem[]> = {};
    items.forEach((item) => {
      const key = (item.media_meta?.clean_title || item.name).toLowerCase().trim();
      if (!clusters[key]) clusters[key] = [];
      clusters[key].push(item);
    });

    return Object.entries(clusters).map(([key, group]) => {
      const isBox = group.length > 1;
      const first = group[0];
      const title = first.media_meta?.clean_title || first.name;
      const totalBytes = group.reduce((acc, cur) => acc + cur.total_bytes, 0);
      const avgProgress = group.reduce((acc, cur) => acc + cur.progress, 0) / group.length;
      const activeIdx = selectedDiscs[key] !== undefined ? selectedDiscs[key] : 0;

      return {
        id: key,
        clean_title: title,
        year: first.media_meta?.year,
        is_boxset: isBox,
        total_bytes: totalBytes,
        overall_progress: avgProgress,
        items: group,
        active_index: Math.min(activeIdx, group.length - 1),
      };
    });
  };

  const getDiscBadge = (disc?: DiscStructureInfo | null) => {
    if (!disc) return null;
    let label = 'OPTICAL DISC';
    let color = 'bg-adc-cardBorderDark text-slate-300';

    switch (disc.layer_spec) {
      case 'Dvd5':
        label = 'DVD-5';
        color = 'bg-amber-950/80 border-amber-600/40 text-amber-300';
        break;
      case 'Dvd9':
        label = 'DVD-9';
        color = 'bg-amber-900/80 border-amber-500/50 text-amber-200';
        break;
      case 'Bd25':
        label = 'BD-25';
        color = 'bg-cyan-950/80 border-cyan-500/40 text-cyan-300';
        break;
      case 'Bd50':
        label = 'BD-50';
        color = 'bg-blue-950/80 border-blue-500/50 text-blue-300';
        break;
      case 'UhdBd66':
        label = '4K UHD BD-66';
        color = 'bg-purple-950/80 border-purple-500/50 text-purple-300';
        break;
      case 'UhdBd100':
        label = '4K UHD BD-100';
        color = 'bg-pink-950/80 border-pink-500/50 text-pink-300';
        break;
      default:
        if (disc.disc_type === 'DvdVideo') label = 'DVD VIDEO';
        else if (disc.disc_type === 'BluRay') label = 'BLU-RAY';
        else if (disc.disc_type === 'UhdBluRay') label = '4K UHD';
        else if (disc.disc_type === 'IsoImage') label = 'DISC ISO';
    }

    return (
      <span
        className={`text-[9px] px-1.5 py-0.5 rounded border font-mono font-semibold tracking-wider ${color}`}
      >
        {label}
      </span>
    );
  };

  const renderGroupCard = (group: MediaGroup, variant: 'active' | 'watch' | 'seeding') => {
    const currentItem = group.items[group.active_index];
    const discInfo = discMetaMap[currentItem.id];
    const progressPct = Math.round(currentItem.progress * 100);
    const isDownloading = currentItem.status === 'Downloading';
    const coverImage = discInfo?.cover_image || currentItem.poster_url;

    return (
      <div
        key={group.id}
        className="group relative flex-shrink-0 w-72 rounded-xl bg-adc-cardDark border border-adc-cardBorderDark hover:border-adc-accent/60 overflow-hidden flex flex-col justify-between transition-all duration-200 shadow-lg hover:shadow-adc-glow"
      >
        {/* 16:9 Thumbnail Poster Area */}
        <div className="aspect-video w-full bg-adc-dark relative flex items-center justify-center overflow-hidden">
          {coverImage ? (
            <img
              src={coverImage.startsWith('http') ? coverImage : `file://${coverImage}`}
              alt={currentItem.name}
              className="w-full h-full object-cover"
              onError={(e) => {
                // Calm fallback: hide broken thumbnail
                (e.target as HTMLElement).style.display = 'none';
              }}
            />
          ) : (
            <div className="flex flex-col items-center justify-center text-adc-cardBorderDark group-hover:text-adc-accent/60 transition space-y-1">
              <Disc className="w-12 h-12" />
              <span className="text-[10px] font-mono uppercase tracking-widest text-slate-500">
                Asian DVD Club
              </span>
            </div>
          )}

          {/* Hover Overlay Play / Inspect Actions */}
          <div className="absolute inset-0 bg-black/75 opacity-0 group-hover:opacity-100 flex flex-col items-center justify-center gap-2.5 transition backdrop-blur-xs p-3">
            <div className="flex items-center gap-2">
              <button
                onClick={() => handlePlayDisc(currentItem, true)}
                className="px-3.5 py-2 rounded-lg bg-adc-accent hover:bg-adc-accent/90 text-white font-semibold text-xs flex items-center gap-1.5 hover:scale-105 transition shadow-lg"
                title="Play Interactive Disc Menus (dvdnav/BD-J)"
              >
                <Play className="w-4 h-4 fill-white" />
                <span>Play Disc</span>
              </button>

              <button
                onClick={() => (isDownloading ? pauseTorrent(currentItem.id) : resumeTorrent(currentItem.id))}
                className="p-2 rounded-lg bg-adc-dark/90 hover:bg-adc-cardDark text-white border border-adc-cardBorderDark transition"
                title={isDownloading ? 'Pause' : 'Resume'}
              >
                {isDownloading ? <Pause className="w-4 h-4" /> : <Play className="w-4 h-4 fill-white" />}
              </button>

              <button
                onClick={() => handleOpenFolder(currentItem.save_path)}
                className="p-2 rounded-lg bg-adc-dark/90 hover:bg-adc-cardDark text-white border border-adc-cardBorderDark transition"
                title="Open Enclosing Folder"
              >
                <FolderOpen className="w-4 h-4" />
              </button>
            </div>

            {variant === 'seeding' && (
              <button
                onClick={() => {
                  setOrganizeTargetItem(currentItem);
                  setOrganizeReport(null);
                }}
                className="flex items-center gap-1 text-[11px] text-slate-300 hover:text-adc-gold hover:underline"
              >
                <FolderArchive className="w-3.5 h-3.5" />
                <span>Organize into Library...</span>
              </button>
            )}
          </div>

          {/* Top Badges */}
          <div className="absolute top-2 left-2 flex items-center gap-1.5">
            <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-black/80 border border-adc-cardBorderDark text-white font-semibold">
              {currentItem.status}
            </span>
            {group.is_boxset && (
              <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-adc-accent/30 border border-adc-accent/50 text-pink-200 font-bold flex items-center gap-1">
                <Layers className="w-3 h-3" />
                <span>{group.items.length} DISCS</span>
              </span>
            )}
          </div>

          <div className="absolute top-2 right-2">
            {getDiscBadge(discInfo)}
          </div>

          {/* Progress bar at bottom of poster */}
          <div className="absolute bottom-0 inset-x-0 h-1 bg-adc-cardBorderDark">
            <div
              className={`h-full transition-all duration-300 ${
                currentItem.status === 'Seeding' ? 'bg-adc-emerald' : 'bg-adc-accent'
              }`}
              style={{ width: `${progressPct}%` }}
            />
          </div>
        </div>

        {/* Card Content & Metadata */}
        <div className="p-3.5 space-y-2.5">
          <div>
            <h4 className="text-xs font-semibold text-white truncate" title={group.clean_title}>
              {group.clean_title}
            </h4>
            <div className="flex flex-wrap items-center gap-1 mt-1">
              {group.year && (
                <span className="text-[9px] px-1.5 py-0.5 rounded bg-adc-cardBorderDark text-adc-gold font-mono font-medium">
                  {group.year}
                </span>
              )}
              {currentItem.media_meta?.resolution && (
                <span className="text-[9px] px-1.5 py-0.5 rounded bg-adc-cardBorderDark text-adc-cyan font-mono font-medium">
                  {currentItem.media_meta.resolution}
                </span>
              )}
              {discInfo?.has_menu && (
                <span className="text-[9px] px-1.5 py-0.5 rounded bg-purple-900/40 border border-purple-500/30 text-purple-300 font-medium">
                  Menu Navigation
                </span>
              )}
            </div>
          </div>

          {/* Multi-Disc Selector Tabs */}
          {group.is_boxset && (
            <div className="flex items-center gap-1 overflow-x-auto pb-1 scrollbar-none">
              {group.items.map((item, idx) => {
                const subDisc = discMetaMap[item.id];
                const discNum = subDisc?.disc_number || idx + 1;
                const isSelected = idx === group.active_index;

                return (
                  <button
                    key={item.id}
                    onClick={() =>
                      setSelectedDiscs((prev) => ({ ...prev, [group.id]: idx }))
                    }
                    className={`text-[10px] font-mono px-2 py-0.5 rounded transition ${
                      isSelected
                        ? 'bg-adc-accent text-white font-bold shadow'
                        : 'bg-adc-dark border border-adc-cardBorderDark text-slate-400 hover:text-white'
                    }`}
                  >
                    Disc {discNum}
                  </button>
                );
              })}
            </div>
          )}

          <div className="flex items-center justify-between text-[11px] font-mono text-slate-400 pt-1 border-t border-adc-cardBorderDark/50">
            <span>{formatBytes(currentItem.total_bytes)}</span>
            {variant === 'active' && (
              <span className="text-adc-cyan font-semibold">
                {formatSpeed(currentItem.download_rate)}
              </span>
            )}
            {variant === 'watch' && (
              <span className="text-adc-gold font-medium">
                {progressPct >= 100 ? 'Ready' : `${progressPct}% ready`}
              </span>
            )}
            {variant === 'seeding' && (
              <span className="text-adc-emerald font-semibold">
                Ratio {formatRatio(currentItem.ratio)}
              </span>
            )}
          </div>

          {variant === 'active' && currentItem.eta_seconds != null && (
            <div className="text-[10px] text-slate-500 font-mono flex justify-between">
              <span>ETA: {formatEta(currentItem.eta_seconds)}</span>
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
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 pb-4 border-b border-adc-cardBorderDark">
        <div className="flex items-center gap-3">
          <Disc className="w-7 h-7 text-adc-accent animate-pulse" />
          <div>
            <h2 className="text-base font-bold text-white tracking-tight">
              Asian DVD Club Cinema
            </h2>
            <p className="text-xs text-slate-400">
              Interactive DVD, Blu-ray & 4K UHD menu navigation & optical library organizer
            </p>
          </div>
        </div>

        {/* Volume Space Info & Controls */}
        <div className="flex flex-wrap items-center gap-2.5">
          {volumeSpace && (
            <div
              className="flex items-center gap-2 px-3 py-1.5 rounded-lg border border-adc-cardBorderDark bg-adc-cardDark text-xs font-mono"
              title={`Mount: ${volumeSpace.mount_point} (${volumeSpace.used_percent}% used)`}
            >
              <HardDrive className="w-3.5 h-3.5 text-adc-cyan" />
              <span className="text-slate-300 font-medium">
                {formatBytes(volumeSpace.available_bytes)} Free
              </span>
              <div className="w-12 h-1.5 bg-adc-dark rounded-full overflow-hidden border border-adc-cardBorderDark">
                <div
                  className={`h-full ${
                    volumeSpace.used_percent > 85 ? 'bg-red-500' : 'bg-adc-cyan'
                  }`}
                  style={{ width: `${volumeSpace.used_percent}%` }}
                />
              </div>
            </div>
          )}

          <button
            onClick={() => {
              setCleaningReport(null);
              setShowCleanerModal(true);
            }}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-adc-cardBorderDark bg-adc-cardDark hover:bg-adc-cardDark/80 text-white text-xs font-medium transition"
            title="Prune orphaned piece caches & old watched torrent markers"
          >
            <Trash2 className="w-3.5 h-3.5 text-slate-400" />
            <span>Clean Cache</span>
          </button>

          <button
            onClick={() => setGroupBoxsets(!groupBoxsets)}
            className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg border text-xs font-medium transition ${
              groupBoxsets
                ? 'border-adc-accent/40 bg-adc-accent/10 text-pink-200'
                : 'border-adc-cardBorderDark bg-adc-cardDark text-slate-400'
            }`}
            title="Cluster multi-disc releases into unified boxset carousels"
          >
            <Layers className="w-3.5 h-3.5" />
            <span>Boxsets</span>
          </button>

          <button
            onClick={() => setViewMode('manager')}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-adc-cardBorderDark bg-adc-cardDark hover:bg-adc-cardDark/80 text-white text-xs font-medium transition"
          >
            <Eye className="w-3.5 h-3.5 text-adc-accent" />
            <span>Manager</span>
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
            <Disc className="w-12 h-12 text-adc-accent" />
          </div>
          <div className="space-y-1">
            <h3 className="text-base font-bold text-white">Your Optical Cinema is Empty</h3>
            <p className="text-xs text-slate-400 max-w-sm">
              Drop any DVD ISO, BDMV Blu-ray folder, or ADC torrent into this window to start
              streaming with interactive menus.
            </p>
          </div>
          <button
            onClick={() => setShowAddTorrent(true)}
            className="flex items-center gap-2 px-5 py-2.5 rounded-lg bg-adc-accent hover:bg-adc-accent/90 text-white text-xs font-semibold transition shadow-lg"
          >
            <Plus className="w-4 h-4" />
            <span>Add First Disc</span>
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
                {groupItems(activeDownloads).map((g) => renderGroupCard(g, 'active'))}
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
              <p className="text-xs text-slate-500 italic">
                No optical disc images or media downloaded yet. Completed releases will appear here.
              </p>
            ) : (
              <div className="flex gap-4 overflow-x-auto pb-2 scrollbar-thin">
                {groupItems(readyToWatch).map((g) => renderGroupCard(g, 'watch'))}
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
                {groupItems(completedSeeding).map((g) => renderGroupCard(g, 'seeding'))}
              </div>
            </div>
          )}
        </div>
      )}

      {/* Modal: Clean Temporary Cache */}
      {showCleanerModal && (
        <div className="fixed inset-0 bg-black/70 backdrop-blur-xs flex items-center justify-center p-4 z-50">
          <div className="w-full max-w-md rounded-2xl bg-adc-cardDark border border-adc-cardBorderDark p-5 space-y-4 shadow-2xl">
            <div className="flex items-center justify-between pb-3 border-b border-adc-cardBorderDark">
              <div className="flex items-center gap-2">
                <Trash2 className="w-5 h-5 text-adc-accent" />
                <h3 className="text-sm font-bold text-white">Disk Space Cleaner</h3>
              </div>
              <button
                onClick={() => setShowCleanerModal(false)}
                className="p-1 rounded hover:bg-adc-cardBorderDark text-slate-400 hover:text-white"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            <p className="text-xs text-slate-400">
              Prune orphaned temporary pieces, stale video thumbnails, and completed .loaded
              markers without touching your downloaded media.
            </p>

            <div className="space-y-1">
              <label className="text-xs text-slate-300 font-medium">Remove files older than:</label>
              <div className="flex gap-2">
                {[7, 14, 30, 60].map((days) => (
                  <button
                    key={days}
                    onClick={() => setCleanerDays(days)}
                    className={`flex-1 py-1.5 text-xs font-mono rounded-lg border transition ${
                      cleanerDays === days
                        ? 'border-adc-accent bg-adc-accent/20 text-white font-bold'
                        : 'border-adc-cardBorderDark bg-adc-dark text-slate-400'
                    }`}
                  >
                    {days} days
                  </button>
                ))}
              </div>
            </div>

            {cleaningReport && (
              <div className="p-3 rounded-lg bg-adc-dark border border-adc-cardBorderDark space-y-1 text-xs font-mono">
                <div className="flex justify-between text-slate-300">
                  <span>Files scanned:</span>
                  <span>{cleaningReport.files_scanned}</span>
                </div>
                <div className="flex justify-between text-adc-emerald font-semibold">
                  <span>Files removed:</span>
                  <span>{cleaningReport.files_removed}</span>
                </div>
                <div className="flex justify-between text-adc-cyan font-semibold">
                  <span>Bytes reclaimed:</span>
                  <span>{formatBytes(cleaningReport.bytes_reclaimed)}</span>
                </div>
              </div>
            )}

            <div className="flex justify-end gap-2 pt-2">
              <button
                onClick={() => handleCleanCache(true)}
                disabled={isCleaning}
                className="px-3.5 py-2 rounded-lg border border-adc-cardBorderDark hover:bg-adc-cardBorderDark text-xs font-medium text-slate-300 transition"
              >
                Scan (Dry Run)
              </button>
              <button
                onClick={() => handleCleanCache(false)}
                disabled={isCleaning}
                className="px-4 py-2 rounded-lg bg-adc-accent hover:bg-adc-accent/90 text-white text-xs font-bold transition shadow-md flex items-center gap-1.5"
              >
                <Sparkles className="w-3.5 h-3.5" />
                <span>{isCleaning ? 'Cleaning...' : 'Clean Now'}</span>
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Modal: Organize Completed Media */}
      {organizeTargetItem && (
        <div className="fixed inset-0 bg-black/70 backdrop-blur-xs flex items-center justify-center p-4 z-50">
          <div className="w-full max-w-lg rounded-2xl bg-adc-cardDark border border-adc-cardBorderDark p-5 space-y-4 shadow-2xl">
            <div className="flex items-center justify-between pb-3 border-b border-adc-cardBorderDark">
              <div className="flex items-center gap-2">
                <FolderArchive className="w-5 h-5 text-adc-gold" />
                <h3 className="text-sm font-bold text-white">Organize into Media Library</h3>
              </div>
              <button
                onClick={() => setOrganizeTargetItem(null)}
                className="p-1 rounded hover:bg-adc-cardBorderDark text-slate-400 hover:text-white"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            <div className="text-xs space-y-1.5">
              <span className="text-slate-400">Target Release:</span>
              <div className="font-semibold text-white bg-adc-dark p-2 rounded-lg border border-adc-cardBorderDark truncate">
                {organizeTargetItem.name}
              </div>
            </div>

            <div className="space-y-1.5">
              <label className="text-xs text-slate-300 font-medium">
                Destination Library Root:
              </label>
              <input
                type="text"
                value={organizeDestPath}
                onChange={(e) => setOrganizeDestPath(e.target.value)}
                placeholder="/Volumes/Media/Asian Cinema"
                className="w-full px-3 py-2 rounded-lg bg-adc-dark border border-adc-cardBorderDark text-xs text-white focus:outline-none focus:border-adc-accent font-mono"
              />
              <p className="text-[11px] text-slate-500">
                Organizes disc into: <code>{'{Destination}/{Clean Title} ({Year})/'}</code>
              </p>
            </div>

            {organizeReport && (
              <div
                className={`p-3 rounded-lg border text-xs font-mono ${
                  organizeReport.success
                    ? 'bg-emerald-950/40 border-emerald-500/40 text-emerald-300'
                    : 'bg-red-950/40 border-red-500/40 text-red-300'
                }`}
              >
                <p className="font-semibold">{organizeReport.message}</p>
                {organizeReport.destination_path && (
                  <p className="text-[10px] text-slate-400 truncate mt-1">
                    Path: {organizeReport.destination_path}
                  </p>
                )}
              </div>
            )}

            <div className="flex justify-end gap-2 pt-2">
              <button
                onClick={() => setOrganizeTargetItem(null)}
                className="px-3.5 py-2 rounded-lg border border-adc-cardBorderDark hover:bg-adc-cardBorderDark text-xs font-medium text-slate-300 transition"
              >
                Close
              </button>
              <button
                onClick={handleExecuteOrganize}
                disabled={isOrganizing || !organizeDestPath.trim()}
                className="px-4 py-2 rounded-lg bg-adc-gold hover:bg-adc-gold/90 text-black text-xs font-bold transition shadow-md disabled:opacity-50"
              >
                {isOrganizing ? 'Moving Files...' : 'Organize'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default SimpleMediaView;
