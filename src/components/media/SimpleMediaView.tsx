import React, { useState, useEffect } from 'react';
import { useTorrentStore } from '../../stores/torrentStore';
import { useUiStore } from '../../stores/uiStore';
import { useTrackerStore } from '../../stores/trackerStore';
import { formatBytes, formatEta } from '../../lib/formatters';
import { errorMessage } from '../../utils/errors';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
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
  Search,
  X,
} from 'lucide-react';
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

/**
 * A playable disc, from either layout the owner owns:
 *  - a boxset of separate torrents (one torrent per disc), or
 *  - ONE torrent folder holding several ISO images (the Banner layout).
 * Both kinds render identical disc pills, so the two discs of a multi-ISO
 * torrent are visible and individually playable (owner: "two discs, no
 * visible folder").
 */
interface PlayableDisc {
  key: string; // stable identity (torrent id or play-target path)
  label: string; // pill number
  title: string; // tooltip
  path: string; // what the player receives
  torrent: TorrentItem; // progress/status source
}

export const SimpleMediaView: React.FC = () => {
  const { torrents, pauseTorrent, selectSourceFolder, setSelectedId } = useTorrentStore();
  const { setShowAddTorrent, setViewMode } = useUiStore();
  const { seedTimeRecords, fetchSeedTimeRecords } = useTrackerStore();

  const [discMetaMap, setDiscMetaMap] = useState<Record<string, DiscStructureInfo | null>>({});
  const [discSetMap, setDiscSetMap] = useState<Record<string, DiscStructureInfo[]>>({});
  const [thumbMap, setThumbMap] = useState<Record<string, string | null>>({});
  const [tmdbMap, setTmdbMap] = useState<Record<string, { poster?: string; overview?: string }>>({});
  const [selectedDiscs, setSelectedDiscs] = useState<Record<string, number>>({});
  const [groupBoxsets, setGroupBoxsets] = useState<boolean>(true);
  const [searchQuery, setSearchQuery] = useState<string>('');

  // Last-played disc per group: a resume should re-open the disc the owner
  // watched last, not the group's first item (owner directive). The stored
  // value is a torrent id (boxset) or a play-target path (multi-ISO set).
  const [lastPlayedDiscs, setLastPlayedDiscs] = useState<Record<string, string>>(() => {
    try {
      return JSON.parse(localStorage.getItem('nebulae_last_played_discs') || '{}');
    } catch {
      return {};
    }
  });
  const rememberLastDisc = (groupId: string, discKey: string) => {
    setLastPlayedDiscs((prev) => {
      const next = { ...prev, [groupId]: discKey };
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

  // ADC seed-time records power the hover line ("ADC: 2.48 · Seed time: 2d 14h").
  useEffect(() => {
    fetchSeedTimeRecords().catch(() => {});
  }, [fetchSeedTimeRecords]);

  // Inspect optical disc format for each media item, AND the full disc set
  // (multi-ISO torrents classify every disc by its own size — the badge, the
  // pills and the hover panel all read the set, not just the first image).
  useEffect(() => {
    torrents.forEach((t) => {
      if (!(t.id in discSetMap)) {
        invoke<DiscStructureInfo[]>('detect_disc_set', { filePath: t.save_path })
          .then((set) => {
            setDiscSetMap((prev) => ({ ...prev, [t.id]: set }));
            const primary = set[0];
            if (primary) {
              setDiscMetaMap((prev) => ({ ...prev, [t.id]: primary }));
            }
          })
          .catch((err) => {
            invoke('log_frontend', {
              message: `disc set failed for ${t.save_path}: ${errorMessage(err, 'unknown')}`,
            }).catch(() => {});
            setDiscSetMap((prev) => ({ ...prev, [t.id]: [] }));
          });
      } else if (!discMetaMap[t.id] && discSetMap[t.id]?.[0]) {
        setDiscMetaMap((prev) => ({ ...prev, [t.id]: discSetMap[t.id][0] }));
      }
    });
  }, [torrents, discSetMap, discMetaMap]);

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
          .catch((err) => {
            invoke('log_frontend', {
              message: `thumbnail failed for ${t.save_path}: ${errorMessage(err, 'unknown')}`,
            }).catch(() => {});
            setThumbMap((prev) => ({ ...prev, [t.id]: null }));
          });
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
          if (!res) {
            invoke('log_frontend', { message: `tmdb: no match for ${t.name}` }).catch(() => {});
          }
          setTmdbMap((prev) => ({
            ...prev,
            [t.id]: res
              ? { poster: res.poster_webp, overview: res.overview ?? undefined }
              : {},
          }));
        })
        .catch((err) => {
          invoke('log_frontend', {
            message: `tmdb fetch failed for ${t.name}: ${errorMessage(err, 'unknown')}`,
          }).catch(() => {});
          setTmdbMap((prev) => ({ ...prev, [t.id]: {} }));
        });
    });
  }, [activeDownloads, readyToWatch, discMetaMap, thumbMap, tmdbMap]);

  // Local file paths must go through Tauri's asset protocol — the webview
  // cannot load raw file:// URLs from its tauri:// origin (that is exactly
  // why posters never rendered). Remote URLs pass through untouched.
  const localSrc = (p: string) => (p.startsWith('http') ? p : convertFileSrc(p));

  const handlePlayDisc = async (
    torrent: TorrentItem,
    groupId?: string,
    targetPath?: string,
  ) => {
    const playPath = targetPath || torrent.save_path;
    invoke('log_frontend', {
      message: `disc button clicked: id=${torrent.id} path=${playPath}`,
    }).catch(() => {});
    try {
      await openInPlayer(playPath);
      if (groupId) rememberLastDisc(groupId, targetPath || torrent.id);
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

  // "Details" in the Canva mock: jump into the Manager with this release selected.
  const openDetails = (id: string) => {
    setSelectedId(id);
    setViewMode('manager');
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

  // Search filter (Canva top bar: "Search library...").
  const query = searchQuery.trim().toLowerCase();
  const visibleGroups = (groups: MediaGroup[]): MediaGroup[] =>
    query
      ? groups.filter(
          (g) =>
            g.clean_title.toLowerCase().includes(query) ||
            g.items.some((i) => i.name.toLowerCase().includes(query)),
        )
      : groups;

  // "2d 14h" style seed-time text for the ADC hover line.
  const fmtSeedTime = (s?: number) => {
    if (!s || s <= 0) return null;
    const d = Math.floor(s / 86400);
    const h = Math.floor((s % 86400) / 3600);
    const m = Math.floor((s % 3600) / 60);
    if (d > 0) return `${d}d ${h}h`;
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m`;
  };

  const layerLabel = (ls?: string) =>
    (({ Dvd5: 'DVD-5', Dvd9: 'DVD-9', Bd25: 'BD-25', Bd50: 'BD-50', UhdBd66: 'UHD-66', UhdBd100: 'UHD-100' }) as Record<string, string>)[ls || ''] || 'Disc';

  const renderGroupCard = (group: MediaGroup, variant: 'active' | 'watch' | 'seeding') => {
    // A remembered last-played disc wins as the card's face (and what
    // "Resume" re-opens) until the owner picks a different disc tab.
    const currentItem = group.items[group.active_index] || group.items[0];
    const discInfo = discMetaMap[currentItem.id];
    const progressPct = Math.round(group.overall_progress * 100);
    const itemProgressPct = Math.round(currentItem.progress * 100);
    const isDownloading = currentItem.status === 'Downloading';
    // Cover priority: structure-detected scan, then the WebP poster cache,
    // then TMDB (fetched lazily below), then any remote poster URL.
    const localCover = discInfo?.cover_image || thumbMap[currentItem.id];
    const coverImage = localCover || tmdbMap[group.id]?.poster || currentItem.poster_url;

    // Unified playable discs: separate torrents (boxset) OR the images of a
    // single multi-ISO torrent. >1 disc always renders visible pills.
    const set = discSetMap[currentItem.id] ?? [];
    const discs: PlayableDisc[] =
      group.items.length > 1
        ? group.items.map((item, idx) => ({
            key: item.id,
            label: String(discMetaMap[item.id]?.disc_number || idx + 1),
            title: item.name,
            path: item.save_path,
            torrent: item,
          }))
        : set.length > 1
          ? set.map((d, idx) => ({
              key: d.play_target,
              label: String(d.disc_number || idx + 1),
              title: d.play_target.split('/').pop() || d.play_target,
              path: d.play_target,
              torrent: currentItem,
            }))
          : [
              {
                key: currentItem.id,
                label: '1',
                title: currentItem.name,
                path: currentItem.save_path,
                torrent: currentItem,
              },
            ];
    const rememberedKey = lastPlayedDiscs[group.id];
    const rememberedIdx = rememberedKey
      ? discs.findIndex((d) => d.key === rememberedKey)
      : -1;
    const effIdx = Math.min(
      selectedDiscs[group.id] ?? (rememberedIdx >= 0 ? rememberedIdx : 0),
      discs.length - 1,
    );
    const effDisc = discs[effIdx] || discs[0];
    // Resume target: a stored path (multi-ISO set) or a torrent id (boxset).
    const resumePath = rememberedKey
      ? rememberedKey.startsWith('/')
        ? rememberedKey
        : (discs.find((d) => d.key === rememberedKey)?.path ?? effDisc.path)
      : null;

    // Aggregated disc badge from the FULL disc set (multi-ISO torrents count
    // every disc by its own size): uniform sets read "2× DVD-5", mixed sets
    // read "DVD-9 + DVD-5".
    const setSpecs = set.map((d) => d.layer_spec);
    const groupSpecs = Array.from(new Set(setSpecs));
    const discCount = set.length || group.items.length;
    const discBadge = groupSpecs.length
      ? groupSpecs.length === 1
        ? `${discCount > 1 ? `${discCount}× ` : ''}${layerLabel(groupSpecs[0])}`
        : groupSpecs.map(layerLabel).join(' + ')
      : null;

    // ADC hover line (Canva: "ADC: 2.48 - Seed time: 2d 14h").
    const seedSeconds = seedTimeRecords.find((r) => r.info_hash === currentItem.info_hash)
      ?.seeding_time_seconds;
    const seedTimeText = fmtSeedTime(seedSeconds);
    const adcLine =
      currentItem.ratio > 0 || seedTimeText
        ? `ADC: ${currentItem.ratio.toFixed(2)}${seedTimeText ? ` · Seed time: ${seedTimeText}` : ''}`
        : null;

    const folderName = currentItem.save_path.split('/').filter(Boolean).pop() || currentItem.save_path;

    return (
      <div
        key={group.id}
        className="group relative rounded-lg bg-[#1f1f1f] border border-[#262626] hover:border-[#3f3f3f] overflow-visible transition-all duration-200 hover:z-30 hover:shadow-2xl hover:shadow-black/60"
      >
        <div className="rounded-lg overflow-hidden bg-[#1f1f1f]">
          {/* Poster tile: full-bleed art, 2:3 like a DVD cover (Canva grid) */}
          <div className="aspect-[2/3] w-full bg-[#141414] relative flex items-center justify-center overflow-hidden">
            {coverImage ? (
              <img
                src={localSrc(coverImage)}
                alt={currentItem.name}
                className="w-full h-full object-cover"
                onError={(e) => {
                  // Calm fallback: hide broken thumbnail
                  (e.target as HTMLElement).style.display = 'none';
                }}
              />
            ) : (
              <div className="flex flex-col items-center justify-center text-[#333333] group-hover:text-[#e50914]/60 transition space-y-1">
                <Disc className="w-10 h-10" />
                <span className="text-[9px] font-mono uppercase tracking-widest text-[#737373]">
                  No Art Yet
                </span>
              </div>
            )}

            {/* Top badges: status left, disc spec right */}
            <div className="absolute top-1.5 left-1.5 flex items-center gap-1">
              <span className="text-[8px] font-mono px-1.5 py-0.5 rounded-full bg-black/80 border border-[#333333] text-white font-semibold">
                {currentItem.status}
              </span>
            </div>
            {discBadge && (
              <div className="absolute top-1.5 right-1.5">
                <span className="text-[8px] px-1.5 py-0.5 rounded-full bg-black/80 border border-[#333333] text-amber-300 font-mono font-semibold">
                  {discBadge}
                </span>
              </div>
            )}
          </div>

          {/* Below the tile (Canva): title, thin progress bar with % */}
          <div className="px-2 pt-1.5 pb-2">
            <h4 className="text-[11px] font-medium text-white truncate" title={group.clean_title}>
              {group.clean_title}
            </h4>
            <div className="flex items-center gap-1.5 mt-1">
              <div className="h-[3px] flex-1 rounded-full bg-[#333333] overflow-hidden">
                <div
                  className={`h-full transition-all duration-300 ${
                    currentItem.status === 'Seeding' ? 'bg-emerald-500' : 'bg-white'
                  }`}
                  style={{ width: `${progressPct}%` }}
                />
              </div>
              <span className="text-[9px] font-mono text-[#737373]">{progressPct}%</span>
            </div>
          </div>
        </div>

        {/* Hover panel (Canva): appears BELOW the poster while the card
            elevates — title, year·discs·spec·size, ADC seed line, the release
            folder, synopsis, disc pills, and the action row. */}
        <div className="absolute inset-x-0 top-full z-30 rounded-b-lg bg-[#1c1c1c] border border-t-0 border-[#262626] shadow-2xl p-3 space-y-2 opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto transition-opacity duration-150">
          <p className="text-[11px] font-semibold text-white line-clamp-2" title={group.clean_title}>
            {group.clean_title}
          </p>
          <div className="text-[9px] font-mono text-[#a3a3a3]">
            {group.year ? `${group.year} · ` : ''}
            {discCount} {discCount === 1 ? 'disc' : 'discs'}
            {discBadge ? ` · ${discBadge}` : ''} · {formatBytes(group.total_bytes)}
          </div>
          {adcLine && <div className="text-[9px] font-mono text-[#737373]">{adcLine}</div>}

          {/* Visible folder: the release's own folder, click to open. */}
          <button
            onClick={() => handleOpenFolder(currentItem.save_path)}
            className="w-full flex items-center gap-1.5 text-left text-[9px] font-mono text-[#a3a3a3] hover:text-white transition"
            title={`Open ${currentItem.save_path}`}
          >
            <FolderOpen className="w-3 h-3 flex-shrink-0" />
            <span className="truncate">{folderName}</span>
          </button>

          {tmdbMap[group.id]?.overview && (
            <p
              className="text-[10px] text-[#a3a3a3] leading-snug line-clamp-4"
              title={tmdbMap[group.id].overview}
            >
              {tmdbMap[group.id].overview}
            </p>
          )}

          {/* Multi-disc pills: visible for BOTH boxsets and multi-ISO sets */}
          {discs.length > 1 && (
            <div className="flex items-center gap-1 flex-wrap">
              {discs.map((d, idx) => (
                <button
                  key={d.key}
                  onClick={() => setSelectedDiscs((prev) => ({ ...prev, [group.id]: idx }))}
                  title={d.title}
                  className={`text-[9px] font-mono px-1.5 py-0.5 rounded-full transition ${
                    idx === effIdx
                      ? 'bg-[#e50914] text-white font-bold'
                      : 'bg-[#1f1f1f] border border-[#333333] text-[#a3a3a3] hover:text-white'
                  }`}
                >
                  {d.label}
                </button>
              ))}
            </div>
          )}

          <div className="flex items-center gap-1.5 pt-0.5">
            <button
              onClick={() => handlePlayDisc(effDisc.torrent, group.id, effDisc.path)}
              className="px-3 py-1.5 rounded-md bg-[#e50914] hover:bg-[#f40612] text-white font-semibold text-[11px] flex items-center gap-1 transition"
              title="Play Interactive Disc Menus (dvdnav/BD-J)"
            >
              <Play className="w-3 h-3 fill-white" />
              <span>Play</span>
            </button>

            {!isDownloading && resumePath && (
              // Shown ONLY when there is something to resume (owner rule):
              // re-open the last-played disc of this release.
              <button
                onClick={() => handlePlayDisc(effDisc.torrent, group.id, resumePath)}
                className="px-2.5 py-1.5 rounded-md border border-[#333333] text-white text-[11px] font-semibold flex items-center gap-1 hover:bg-[#262626] transition"
                title={`Resume the disc you played last: ${resumePath.split('/').pop()}`}
              >
                <Play className="w-3 h-3 fill-current" />
                <span>Resume</span>
              </button>
            )}

            {isDownloading && (
              <button
                onClick={() => pauseTorrent(currentItem.id)}
                className="p-1.5 rounded-md border border-[#333333] text-[#a3a3a3] hover:text-white hover:bg-[#262626] transition"
                title="Pause"
              >
                <Pause className="w-3 h-3" />
              </button>
            )}

            <button
              onClick={() => openDetails(currentItem.id)}
              className="px-2.5 py-1.5 rounded-md border border-[#333333] text-[#a3a3a3] text-[11px] font-medium hover:text-white hover:bg-[#262626] transition"
              title="Open this release in Nebulae Manager"
            >
              Details
            </button>

            <button
              onClick={() => handleOpenFolder(currentItem.save_path)}
              className="p-1.5 rounded-md border border-[#333333] text-[#a3a3a3] hover:text-white hover:bg-[#262626] transition"
              title="Open Enclosing Folder"
            >
              <FolderOpen className="w-3 h-3" />
            </button>
          </div>

          {variant === 'active' && currentItem.eta_seconds != null && (
            <div className="text-[9px] font-mono text-[#737373]">
              ETA {formatEta(currentItem.eta_seconds)} · disc {itemProgressPct}%
            </div>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-8 bg-[#141414] text-white select-none">
      {/* Top bar (Canva): Nebulae wordmark left; search + controls right.
          Dark palette per the owner's override of the Canva background. */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 pb-4 border-b border-[#262626]">
        <div className="flex items-center gap-2">
          <span className="w-2.5 h-2.5 rounded-full bg-[#e50914]" />
          <h2 className="text-lg font-bold tracking-tight">Nebulae</h2>
        </div>

        <div className="flex flex-wrap items-center gap-2.5">
          <div
            className="flex items-center gap-2 px-3 py-1.5 rounded-full bg-[#1f1f1f] border border-[#262626] w-56 focus-within:border-[#e50914]/50 transition"
            title="Search the library by release or folder name"
          >
            <Search className="w-3.5 h-3.5 text-[#737373]" />
            <input
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search library..."
              className="bg-transparent text-xs outline-none flex-1 text-white placeholder:text-[#737373]"
            />
            {searchQuery && (
              <button
                onClick={() => setSearchQuery('')}
                className="text-[#737373] hover:text-white transition"
                title="Clear search"
              >
                <X className="w-3 h-3" />
              </button>
            )}
          </div>

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
            className="p-2 rounded-lg border border-[#333333] bg-[#1f1f1f] hover:bg-[#262626] text-[#a3a3a3] hover:text-white transition"
            title="Clear Resume Cache — forget last-played discs (hides Resume until discs are played again)"
          >
            <Eraser className="w-3.5 h-3.5" />
          </button>

          <button
            onClick={() => setGroupBoxsets(!groupBoxsets)}
            className={`p-2 rounded-lg border transition ${
              groupBoxsets
                ? 'border-[#e50914]/40 bg-[#e50914]/10 text-white'
                : 'border-[#333333] bg-[#1f1f1f] text-[#a3a3a3] hover:text-white'
            }`}
            title="Cluster multi-disc releases into unified rows"
          >
            <Layers className="w-3.5 h-3.5" />
          </button>

          <button
            onClick={handlePlayExternalDisc}
            className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg border border-[#333333] bg-[#1f1f1f] hover:bg-[#262626] text-white text-xs font-semibold transition"
            title="Play a DVD, Blu-ray, or UHD disc from your optical drive with the bundled Nebulae player"
          >
            <Disc className="w-4 h-4 text-[#e50914]" />
            <span>Play External Disc</span>
          </button>
        </div>
      </div>

      {externalDiscError && (
        <div className="p-2.5 rounded-lg bg-[#2a1518] border border-[#e50914]/40 text-[11px] text-[#ffb3b3]">
          {externalDiscError}
        </div>
      )}

      {torrents.length === 0 ? (
        /* Empty State */
        <div className="flex flex-col items-center justify-center py-24 text-center space-y-4">
          <div className="p-5 rounded-2xl bg-[#1f1f1f] border border-[#262626]">
            <Disc className="w-12 h-12 text-[#e50914]" />
          </div>
          <div className="space-y-1">
            <h3 className="text-base font-semibold text-white">Your Optical Cinema is Empty</h3>
            <p className="text-xs text-[#a3a3a3] max-w-sm">
              Drop any DVD ISO, BDMV Blu-ray folder, or ADC torrent into this window to start
              streaming with interactive menus.
            </p>
          </div>
          <button
            onClick={() => setShowAddTorrent(true)}
            className="flex items-center gap-2 px-5 py-2.5 rounded-full bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold transition shadow-md"
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
                <ArrowDownCircle className="w-4 h-4 text-[#a3a3a3]" />
                <h3 className="text-[11px] font-semibold uppercase tracking-[0.18em] text-[#a3a3a3]">
                  Downloading
                </h3>
                <span className="text-[11px] font-mono text-white px-2 py-0.5 rounded-full bg-[#1f1f1f] border border-[#333333]">
                  {activeDownloads.length}
                </span>
              </div>
              <div className="grid gap-4 [grid-template-columns:repeat(auto-fill,minmax(150px,1fr))]">
                {visibleGroups(groupItems(activeDownloads)).map((g) => renderGroupCard(g, 'active'))}
              </div>
            </div>
          )}

          {/* Shelf 2: Ready to Watch */}
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <Play className="w-4 h-4 text-[#e50914] fill-current" />
              <h3 className="text-[11px] font-semibold uppercase tracking-[0.18em] text-[#a3a3a3]">
                Ready to Watch
              </h3>
              <span className="text-[11px] font-mono text-white px-2 py-0.5 rounded-full bg-[#1f1f1f] border border-[#333333]">
                {readyToWatch.length}
              </span>
            </div>
            {readyToWatch.length === 0 ? (
              <p className="text-xs text-[#737373] italic">
                No optical disc images or media downloaded yet. Completed releases will appear here.
              </p>
            ) : (
              <div className="grid gap-4 [grid-template-columns:repeat(auto-fill,minmax(150px,1fr))]">
                {visibleGroups(groupItems(readyToWatch)).map((g) => renderGroupCard(g, 'watch'))}
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
