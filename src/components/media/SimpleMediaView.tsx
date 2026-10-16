import React, { useState, useEffect } from 'react';
import { useTorrentStore } from '../../stores/torrentStore';
import { useUiStore } from '../../stores/uiStore';
import { formatBytes, formatSpeed, formatEta } from '../../lib/formatters';
import { errorMessage } from '../../utils/errors';
import {
  TorrentItem,
  DiscStructureInfo,
  OrganizeMediaReport,
  ThumbnailResult,
  ParsedMediaMetadata,
} from '../../types';

/** Shape of the Rust TmdbMatch (serde-serialized). */
interface FetchTmdbResult {
  poster_webp: string;
  overview: string | null;
  year: number | null;
}
import {
  Play,
  Pause,
  Plus,
  FolderOpen,
  ArrowDownCircle,
  Disc,
  Eraser,
  Layers,
  FolderArchive,
  X,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { openInPlayer } from '../../lib/playerStore';

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
  const { torrents, pauseTorrent, selectSourceFolder } = useTorrentStore();
  const { setShowAddTorrent } = useUiStore();

  const [discMetaMap, setDiscMetaMap] = useState<Record<string, DiscStructureInfo | null>>({});
  const [thumbMap, setThumbMap] = useState<Record<string, string | null>>({});
  const [tmdbMap, setTmdbMap] = useState<Record<string, { poster?: string; overview?: string }>>({});
  const [selectedDiscs, setSelectedDiscs] = useState<Record<string, number>>({});
  const [groupBoxsets, setGroupBoxsets] = useState<boolean>(true);

  // Last-played disc per boxset group: a resume should re-open the disc the
  // owner watched last, not the group's first item (owner directive).
  const [lastPlayedDiscs, setLastPlayedDiscs] = useState<Record<string, string>>(() => {
    try {
      return JSON.parse(localStorage.getItem('nebulae_last_played_discs') || '{}');
    } catch {
      return {};
    }
  });
  const rememberLastDisc = (groupId: string, torrentId: string) => {
    setLastPlayedDiscs((prev) => {
      const next = { ...prev, [groupId]: torrentId };
      try {
        localStorage.setItem('nebulae_last_played_discs', JSON.stringify(next));
      } catch {
        // Storage full/unavailable: in-memory memory still works this session.
      }
      return next;
    });
  };

  const [organizeTargetItem, setOrganizeTargetItem] = useState<TorrentItem | null>(null);
  const [organizeDestPath, setOrganizeDestPath] = useState<string>('');
  const [organizeReport, setOrganizeReport] = useState<OrganizeMediaReport | null>(null);
  const [isOrganizing, setIsOrganizing] = useState<boolean>(false);
  const [externalDiscError, setExternalDiscError] = useState<string | null>(null);

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

  // Poster thumbnails: the WebP pipeline (cover scan → downscale → cache)
  // runs per item; releases without any art fall back to the ffmpeg frame
  // extractor inside the same command. Only missing entries are fetched.
  useEffect(() => {
    torrents.forEach((t) => {
      if (!(t.id in thumbMap)) {
        invoke<ThumbnailResult>('get_media_thumbnail', { mediaPath: t.save_path })
          .then((res) => {
            setThumbMap((prev) => ({ ...prev, [t.id]: res.thumbnail_path ?? null }));
          })
          .catch(() => setThumbMap((prev) => ({ ...prev, [t.id]: null })));
      }
    });
  }, [torrents, thumbMap]);

  const activeDownloads = torrents.filter((t) => t.status === 'Downloading');
  const readyToWatch = torrents.filter(
    (t) =>
      (t.is_media || t.category === 'Movies') &&
      (t.progress > 0.05 || t.status === 'Seeding' || t.status === 'Completed')
  );

  // TMDB fallback art + synopsis: fetched once per release that has no
  // local cover at all (structure scan and WebP cache both empty). The
  // command parses the title via the engine's parser, then caches its match
  // in the DB, so this fires once per release ever.
  useEffect(() => {
    const shelves = [...activeDownloads, ...readyToWatch];
    shelves.forEach((t) => {
      if (t.id in tmdbMap) return;
      const hasLocalCover =
        discMetaMap[t.id]?.cover_image || (thumbMap[t.id] ?? null) !== null;
      if (hasLocalCover) return;
      invoke<ParsedMediaMetadata>('parse_media_title', { rawTitle: t.name })
        .then((parsed) =>
          invoke<FetchTmdbResult>('fetch_tmdb_release', {
            title: parsed.clean_title,
            year: parsed.year ?? undefined,
          }),
        )
        .then((res) => {
          setTmdbMap((prev) => ({
            ...prev,
            [t.id]: res
              ? { poster: res.poster_webp, overview: res.overview ?? undefined }
              : {},
          }));
        })
        .catch(() => setTmdbMap((prev) => ({ ...prev, [t.id]: {} })));
    });
  }, [activeDownloads, readyToWatch, discMetaMap, thumbMap, tmdbMap]);

  const handlePlayDisc = async (torrent: TorrentItem, preferMenu: boolean = true, groupId?: string) => {
    void preferMenu; // the embedded engine always starts discs at their menu
    invoke('log_frontend', {
      message: `disc button clicked: id=${torrent.id} path=${torrent.save_path}`,
    }).catch(() => {});
    try {
      await openInPlayer(torrent.save_path);
      if (groupId) rememberLastDisc(groupId, torrent.id);
      setExternalDiscError(null);
    } catch (err: unknown) {
      // Surface the failure - a silently swallowed error looks like a dead button.
      const msg = errorMessage(err, 'Could not launch the disc.');
      invoke('log_frontend', { message: `disc open failed: ${msg}` }).catch(() => {});
      setExternalDiscError(msg);
    }
  };

  const handleOpenFolder = (path: string) => {
    invoke('open_torrent_folder', { path }).catch(() => {});
  };

  // Play an external/internal optical disc: the user points Nebulae at the
  // mounted DVD/Blu-ray/UHD volume; the embedded VLC engine plays it with
  // full menu navigation inside Nebulae's own window.
  const handlePlayExternalDisc = async () => {
    setExternalDiscError(null);
    try {
      const folder = await selectSourceFolder();
      if (!folder) return;
      await openInPlayer(folder);
    } catch (err: unknown) {
      setExternalDiscError(errorMessage(err, 'Could not launch the disc.'));
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
    } catch (err: unknown) {
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

  const renderGroupCard = (group: MediaGroup, variant: 'active' | 'watch' | 'seeding') => {
    // A remembered last-played disc wins as the card's face (and what
    // "Resume" re-opens) until the owner picks a different disc tab.
    const rememberedId = lastPlayedDiscs[group.id];
    const rememberedIdx = rememberedId
      ? group.items.findIndex((i) => i.id === rememberedId)
      : -1;
    const effectiveIdx =
      selectedDiscs[group.id] ??
      (rememberedIdx >= 0 ? rememberedIdx : group.active_index);
    const currentItem = group.items[effectiveIdx] || group.items[group.active_index];
    const discInfo = discMetaMap[currentItem.id];
    const progressPct = Math.round(currentItem.progress * 100);
    const isDownloading = currentItem.status === 'Downloading';
    // Cover priority: structure-detected scan, then the WebP poster cache,
    // then TMDB (fetched lazily below), then any remote poster URL.
    const localCover = discInfo?.cover_image || thumbMap[currentItem.id];
    const coverImage = localCover || tmdbMap[group.id]?.poster || currentItem.poster_url;

    // Aggregated disc badge: a boxset whose discs share one layer spec reads
    // "2× DVD-9"; mixed specs read "DVD-9 + DVD-5" (owner: a lone DVD-9 badge
    // on a two-disc set is misleading).
    const layerLabel = (ls?: string) =>
      (({ Dvd5: 'DVD-5', Dvd9: 'DVD-9', Bd25: 'BD-25', Bd50: 'BD-50', UhdBd66: 'UHD-66', UhdBd100: 'UHD-100' }) as Record<string, string>)[ls || ''] || 'Disc';
    const groupSpecs = Array.from(
      new Set(group.items.map((i) => discMetaMap[i.id]?.layer_spec).filter(Boolean)),
    ) as string[];
    const discBadge = groupSpecs.length
      ? groupSpecs.length === 1
        ? `${group.is_boxset ? `${group.items.length}× ` : ''}${layerLabel(groupSpecs[0])}`
        : groupSpecs.map(layerLabel).join(' + ')
      : null;

    return (
      <div
        key={group.id}
        className="group relative flex-shrink-0 w-[180px] rounded-2xl bg-white border border-gray-200 hover:border-blue-300 overflow-hidden flex flex-col transition-all duration-300 shadow-sm hover:shadow-xl hover:-translate-y-1"
      >
        {/* Poster tile: full-bleed art, 2:3 like a DVD cover */}
        <div className="aspect-[2/3] w-full bg-gray-50 relative flex items-center justify-center overflow-hidden">
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
            <div className="flex flex-col items-center justify-center text-gray-300 group-hover:text-blue-300 transition space-y-1">
              <Disc className="w-12 h-12" />
              <span className="text-[9px] font-mono uppercase tracking-widest text-gray-400">
                No Art Yet
              </span>
            </div>
          )}

          {/* Top badges: status left, disc spec right — light pills */}
          <div className="absolute top-2 left-2 flex items-center gap-1">
            <span className="text-[9px] font-mono px-1.5 py-0.5 rounded-full bg-white/95 border border-gray-200 text-gray-600 font-semibold">
              {currentItem.status}
            </span>
          </div>
          {discBadge && (
            <div className="absolute top-2 right-2">
              <span className="text-[9px] px-1.5 py-0.5 rounded-full bg-white/95 border border-gray-200 text-blue-700 font-mono font-semibold">
                {discBadge}
              </span>
            </div>
          )}

          {/* Hover info panel: white, airy, everything extra lives here */}
          <div className="absolute inset-0 bg-white/97 opacity-0 group-hover:opacity-100 flex flex-col items-center justify-center gap-2 transition p-3">
            <p className="text-[11px] font-semibold text-[#2b2f36] text-center line-clamp-2">
              {group.clean_title}
            </p>
            <div className="text-[9px] font-mono text-gray-500 text-center">
              {group.year ? `${group.year} · ` : ''}
              {group.items.length} {group.items.length === 1 ? 'disc' : 'discs'}
              {discBadge ? ` · ${discBadge}` : ''} · {formatBytes(group.total_bytes)}
            </div>
            {tmdbMap[group.id]?.overview && (
              <p
                className="text-[10px] text-gray-500 leading-snug line-clamp-4 text-center"
                title={tmdbMap[group.id].overview}
              >
                {tmdbMap[group.id].overview}
              </p>
            )}

            <div className="flex items-center gap-1.5 mt-1">
              <button
                onClick={() => handlePlayDisc(currentItem, true, group.id)}
                className="px-3 py-1.5 rounded-full bg-blue-500 hover:bg-blue-600 text-white font-semibold text-[11px] flex items-center gap-1 transition shadow-sm"
                title="Play Interactive Disc Menus (dvdnav/BD-J)"
              >
                <Play className="w-3 h-3 fill-white" />
                <span>Play</span>
              </button>

              {isDownloading ? (
                <button
                  onClick={() => pauseTorrent(currentItem.id)}
                  className="p-1.5 rounded-full border border-gray-300 text-gray-600 hover:bg-gray-50 transition"
                  title="Pause"
                >
                  <Pause className="w-3 h-3" />
                </button>
              ) : lastPlayedDiscs[group.id] ? (
                // Shown ONLY when there is something to resume (owner rule):
                // re-open the last-played disc of this boxset.
                <button
                  onClick={() => handlePlayDisc(currentItem, true, group.id)}
                  className="px-2.5 py-1.5 rounded-full border border-gray-300 text-gray-700 text-[11px] font-semibold flex items-center gap-1 hover:bg-gray-50 transition"
                  title={`Resume from the last disc you played: ${currentItem.name}`}
                >
                  <Play className="w-3 h-3 fill-current" />
                  <span>Resume</span>
                </button>
              ) : null}

              <button
                onClick={() => handleOpenFolder(currentItem.save_path)}
                className="p-1.5 rounded-full border border-gray-300 text-gray-600 hover:bg-gray-50 transition"
                title="Open Enclosing Folder"
              >
                <FolderOpen className="w-3 h-3" />
              </button>
            </div>

            {/* Multi-disc selector: tiny pills inside the hover panel */}
            {group.is_boxset && (
              <div className="flex items-center gap-1 flex-wrap justify-center max-w-full">
                {group.items.map((item, idx) => {
                  const subDisc = discMetaMap[item.id];
                  const discNum = subDisc?.disc_number || idx + 1;
                  const isSelected = idx === effectiveIdx;
                  return (
                    <button
                      key={item.id}
                      onClick={() =>
                        setSelectedDiscs((prev) => ({ ...prev, [group.id]: idx }))
                      }
                      className={`text-[9px] font-mono px-1.5 py-0.5 rounded-full transition ${
                        isSelected
                          ? 'bg-blue-500 text-white font-bold'
                          : 'bg-gray-100 border border-gray-200 text-gray-500 hover:text-gray-800'
                      }`}
                    >
                      {discNum}
                    </button>
                  );
                })}
              </div>
            )}

            {variant === 'active' && currentItem.eta_seconds != null && (
              <div className="text-[9px] font-mono text-gray-400">
                ETA {formatEta(currentItem.eta_seconds)} · {progressPct}%
              </div>
            )}
          </div>

          {/* Progress bar at bottom of poster */}
          <div className="absolute bottom-0 inset-x-0 h-1 bg-gray-200">
            <div
              className={`h-full transition-all duration-300 ${
                currentItem.status === 'Seeding' ? 'bg-emerald-400' : 'bg-blue-500'
              }`}
              style={{ width: `${progressPct}%` }}
            />
          </div>
        </div>

        {/* Below the tile: title + year on the page background (Canva design) */}
        <div className="px-2.5 py-2">
          <h4 className="text-[12px] font-medium text-[#2b2f36] truncate" title={group.clean_title}>
            {group.clean_title}
          </h4>
          <div className="text-[10px] text-gray-400 font-mono">
            {group.year || '—'}
            {variant === 'watch' && progressPct < 100 ? ` · ${progressPct}% ready` : ''}
            {variant === 'active' ? ` · ${formatSpeed(currentItem.download_rate)}` : ''}
          </div>
        </div>
      </div>
    );
  };

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-8 bg-[#fafafa] text-[#2b2f36] select-none">
      {/* Header Bar (light, airy — Canva design) */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 pb-4 border-b border-gray-200">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-xl bg-blue-50">
            <Disc className="w-5 h-5 text-blue-600" />
          </div>
          <div>
            <h2 className="text-base font-semibold text-[#2b2f36] tracking-tight">
              Asian DVD Club Cinema
            </h2>
            <p className="text-xs text-gray-500">
              DVD, Blu-ray & 4K UHD optical library organizer
            </p>
          </div>
        </div>

        {/* Controls (Add/Manager/Free-space live in the title/status bars) */}
        <div className="flex flex-wrap items-center gap-2.5">
          <button
            onClick={() => {
              // Forget every last-played disc: the Resume buttons vanish
              // until new discs are played (owner-requested reset).
              setLastPlayedDiscs({});
              try {
                localStorage.removeItem('nebulae_last_played_discs');
              } catch {
                // Nothing to clear if storage is unavailable.
              }
            }}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-adc-cardBorderDark bg-adc-cardDark hover:bg-adc-cardDark/80 text-white text-xs font-medium transition"
            title="Forget last-played discs — hides the Resume buttons until discs are played again"
          >
            <Eraser className="w-3.5 h-3.5 text-slate-400" />
            <span>Clear Resume Cache</span>
          </button>

          <button
            onClick={() => setGroupBoxsets(!groupBoxsets)}
            className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg border text-xs font-medium transition ${
              groupBoxsets
                ? 'border-blue-300 bg-blue-50 text-blue-700'
                : 'border-gray-200 bg-white text-gray-500 hover:text-gray-800'
            }`}
            title="Cluster multi-disc releases into unified rows"
          >
            <Layers className="w-3.5 h-3.5" />
          </button>

          <button
            onClick={handlePlayExternalDisc}
            className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg border border-gray-200 bg-white hover:bg-gray-50 text-[#2b2f36] text-xs font-semibold transition"
            title="Play a DVD, Blu-ray, or UHD disc from your optical drive with the bundled Nebulae player"
          >
            <Disc className="w-4 h-4 text-blue-600" />
            <span>Play External Disc</span>
          </button>
        </div>
      </div>

      {externalDiscError && (
        <div className="p-2.5 rounded-lg bg-red-50 border border-red-200 text-[11px] text-red-700">
          {externalDiscError}
        </div>
      )}

      {torrents.length === 0 ? (
        /* Empty State */
        <div className="flex flex-col items-center justify-center py-24 text-center space-y-4">
          <div className="p-5 rounded-2xl bg-blue-50 border border-blue-100">
            <Disc className="w-12 h-12 text-blue-400" />
          </div>
          <div className="space-y-1">
            <h3 className="text-base font-semibold text-[#2b2f36]">Your Optical Cinema is Empty</h3>
            <p className="text-xs text-gray-500 max-w-sm">
              Drop any DVD ISO, BDMV Blu-ray folder, or ADC torrent into this window to start
              streaming with interactive menus.
            </p>
          </div>
          <button
            onClick={() => setShowAddTorrent(true)}
            className="flex items-center gap-2 px-5 py-2.5 rounded-full bg-blue-500 hover:bg-blue-600 text-white text-xs font-semibold transition shadow-md"
          >
            <Plus className="w-4 h-4" />
            <span>Add First Disc</span>
          </button>
        </div>
      ) : (
        <div className="space-y-10">
          {/* Shelf 1: Active Downloads */}
          {activeDownloads.length > 0 && (
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <ArrowDownCircle className="w-4 h-4 text-blue-500" />
                <h3 className="text-[11px] font-semibold uppercase tracking-wider text-gray-500">
                  Downloading
                </h3>
                <span className="text-[11px] font-mono text-blue-600 px-2 py-0.5 rounded-full bg-blue-50">
                  {activeDownloads.length}
                </span>
              </div>
              <div className="flex gap-5 overflow-x-auto pb-3 scrollbar-thin">
                {groupItems(activeDownloads).map((g) => renderGroupCard(g, 'active'))}
              </div>
            </div>
          )}

          {/* Shelf 2: Ready to Watch */}
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <Play className="w-4 h-4 text-blue-500 fill-current" />
              <h3 className="text-[11px] font-semibold uppercase tracking-wider text-gray-500">
                Ready to Watch
              </h3>
              <span className="text-[11px] font-mono text-blue-600 px-2 py-0.5 rounded-full bg-blue-50">
                {readyToWatch.length}
              </span>
            </div>
            {readyToWatch.length === 0 ? (
              <p className="text-xs text-gray-400 italic">
                No optical disc images or media downloaded yet. Completed releases will appear here.
              </p>
            ) : (
              <div className="flex gap-5 overflow-x-auto pb-3 scrollbar-thin">
                {groupItems(readyToWatch).map((g) => renderGroupCard(g, 'watch'))}
              </div>
            )}
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
