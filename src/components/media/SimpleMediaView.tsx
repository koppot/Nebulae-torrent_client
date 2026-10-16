import React, { useState, useEffect } from 'react';
import { useTorrentStore } from '../../stores/torrentStore';
import { useUiStore } from '../../stores/uiStore';
import { useTrackerStore } from '../../stores/trackerStore';
import { formatBytes } from '../../lib/formatters';
import { errorMessage } from '../../utils/errors';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { openInPlayer, resumeInPlayer, savedPositionMs } from '../../lib/playerStore';
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
  runtime: number | null;
}
import {
  Play,
  Plus,
  FolderOpen,
  ArrowDownCircle,
  Disc,
  Eraser,
  Layers,
  FolderArchive,
  Search,
  X,
  Info,
  Copy,
} from 'lucide-react';

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

/**
 * The torrent's ACTUAL content path. `save_path` is the download ROOT (the
 * engine reports the download dir), so disc-set detection, thumbnails, folder
 * display and playback all resolved against ~/Downloads instead of the
 * release folder — the lone "DVD-9" badge and wrong-disc playback. The
 * release folder/file is save_path + name.
 */
export const contentPath = (t: TorrentItem): string => {
  const base = t.save_path.replace(/\/+$/, '');
  return base.endsWith(t.name) ? base : `${base}/${t.name}`;
};

/** Folder to reveal in Finder: the release folder, or the parent of a file. */
export const folderOf = (t: TorrentItem): string => {
  const p = contentPath(t);
  return /\.[a-z0-9]+$/i.test(p) ? p.split('/').slice(0, -1).join('/') : p;
};

export const SimpleMediaView: React.FC = () => {  const { torrents, selectSourceFolder, setSelectedId } = useTorrentStore();
  const { setShowAddTorrent, setViewMode } = useUiStore();
  const { seedTimeRecords, fetchSeedTimeRecords } = useTrackerStore();

  const [discMetaMap, setDiscMetaMap] = useState<Record<string, DiscStructureInfo | null>>({});
  const [discSetMap, setDiscSetMap] = useState<Record<string, DiscStructureInfo[]>>({});
  // Read-only: per-disc selection now happens in the player's disc switcher;
  // the group's active item follows the last-played disc.
  const [selectedDiscs] = useState<Record<string, number>>({});  const [thumbMap, setThumbMap] = useState<Record<string, string | null>>({});
  const [tmdbMap, setTmdbMap] = useState<
    Record<string, { poster?: string; overview?: string; runtime?: number }>
  >({});
  const [groupBoxsets, setGroupBoxsets] = useState<boolean>(true);
  const [searchQuery, setSearchQuery] = useState<string>('');

  // Hover = the lower half of the poster is replaced by grayscale info
  // (owner). The overlay lives INSIDE the poster box, so nothing can clip
  // it and no portal is needed.
  const [hoveredId, setHoveredId] = useState<string | null>(null);

  // Right-click menu on a poster (owner): Show in Finder, Play, Resume, plus
  // a few useful extras. Viewport-anchored, grayscale, closes on any click.
  const [ctxMenu, setCtxMenu] = useState<{
    x: number;
    y: number;
    groupId: string;
    itemId: string;
  } | null>(null);

  // Details popup (owner): ADC website info when available, otherwise the
  // TMDB/Trakt summary. Fetched when the popup opens.
  const [detailsFor, setDetailsFor] = useState<{ groupId: string; itemId: string } | null>(null);
  const [detailsData, setDetailsData] = useState<{
    adcUrl: string | null;
    adcText: string | null;
    overview: string | null;
    runtimeMin: number | null;
    source: 'adc' | 'tmdb' | 'none';
  } | null>(null);
  const [detailsLoading, setDetailsLoading] = useState(false);

  // Any scroll dismisses the context menu.
  useEffect(() => {
    if (!ctxMenu) return;
    const dismiss = () => setCtxMenu(null);
    window.addEventListener('scroll', dismiss, true);
    window.addEventListener('resize', dismiss);
    return () => {
      window.removeEventListener('scroll', dismiss, true);
      window.removeEventListener('resize', dismiss);
    };
  }, [ctxMenu]);

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
        invoke<DiscStructureInfo[]>('detect_disc_set', { filePath: contentPath(t) })
          .then((set) => {
            setDiscSetMap((prev) => ({ ...prev, [t.id]: set }));
            const primary = set[0];
            if (primary) {
              setDiscMetaMap((prev) => ({ ...prev, [t.id]: primary }));
            }
          })
          .catch((err) => {
            invoke('log_frontend', {
              message: `disc set failed for ${contentPath(t)}: ${errorMessage(err, 'unknown')}`,
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
        invoke<ThumbnailResult>('get_media_thumbnail', { mediaPath: contentPath(t) })
          .then((res) => {
            setThumbMap((prev) => ({ ...prev, [t.id]: res.thumbnail_path ?? null }));
          })
          .catch((err) => {
            invoke('log_frontend', {
              message: `thumbnail failed for ${contentPath(t)}: ${errorMessage(err, 'unknown')}`,
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
      // Reserve the slot IMMEDIATELY: without this, every re-run of the
      // effect (any map update) re-fires the fetch until the first response
      // lands — four identical TMDB searches per launch were observed.
      setTmdbMap((prev) => ({ ...prev, [t.id]: {} }));
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
              ? {
                  poster: res.poster_webp,
                  overview: res.overview ?? undefined,
                  runtime: res.runtime ?? undefined,
                }
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
    const playPath = targetPath || contentPath(torrent);
    invoke('log_frontend', {
      message: `disc button clicked: id=${torrent.id} path=${playPath}`,
    }).catch(() => {});
    try {
      const info = await openInPlayer(playPath);
      if (groupId) {
        // Remember the RESOLVED path (the actual ISO/VIDEO_TS) — the resume
        // position map and the disc pills key off exactly this path.
        rememberLastDisc(groupId, info.resolved_path || playPath);
      }
      setExternalDiscError(null);
    } catch (err: unknown) {
      // Surface the failure - a silently swallowed error looks like a dead button.
      const msg = errorMessage(err, 'Could not launch the disc.');
      invoke('log_frontend', { message: `disc open failed: ${msg}` }).catch(() => {});
      setExternalDiscError(msg);
    }
  };

  // Resume = skip the disc menu and continue the feature where the poller
  // saved it (owner: "Resume forwards me to the disc menu again").
  const handleResumeDisc = async (groupId: string, resumePath: string) => {
    const savedMs = savedPositionMs(resumePath);
    invoke('log_frontend', {
      message: `resume clicked: path=${resumePath} savedMs=${savedMs}`,
    }).catch(() => {});
    try {
      const info = await resumeInPlayer(resumePath, savedMs);
      rememberLastDisc(groupId, info.resolved_path || resumePath);
      setExternalDiscError(null);
    } catch (err: unknown) {
      const msg = errorMessage(err, 'Could not resume the disc.');
      invoke('log_frontend', { message: `resume failed: ${msg}` }).catch(() => {});
      setExternalDiscError(msg);
    }
  };

  const handleOpenFolder = (path: string) => {
    invoke('open_torrent_folder', { path }).catch(() => {});
  };

  // Reveal the release's own folder in Finder (not the download root).
  const handleRevealRelease = (t: TorrentItem) => handleOpenFolder(folderOf(t));

  // "Details" in the Canva mock: jump into the Manager with this release selected.
  const openDetails = (id: string) => {
    setSelectedId(id);
    setViewMode('manager');
  };

  // Details POPUP (owner): ADC website info when the torrent carries its
  // page URL, otherwise the TMDB/Trakt summary (which the engine caches).
  const openDetailsPopup = async (groupId: string, item: TorrentItem) => {
    setDetailsFor({ groupId, itemId: item.id });
    setDetailsLoading(true);
    setDetailsData(null);

    const adcUrl = /https?:\/\/(?:www\.)?asiandvdclub\.org\/[^\s"']+/i.exec(item.comment)?.[0] || null;
    let adcText: string | null = null;
    if (adcUrl) {
      try {
        adcText = await invoke<string>('fetch_adc_page', { url: adcUrl });
      } catch (err) {
        invoke('log_frontend', {
          message: `adc details unavailable: ${errorMessage(err, 'unknown')}`,
        }).catch(() => {});
      }
    }

    // TMDB/Trakt summary — DB-cached engine-side; the same call the poster
    // fallback uses, so this never refires a network search per open.
    let overview: string | null = null;
    let runtimeMin: number | null = null;
    try {
      const parsed = await invoke<ParsedMediaMetadata>('parse_media_title', { rawTitle: item.name });
      const res = await invoke<FetchTmdbResult | null>('fetch_tmdb_release', {
        title: parsed.clean_title,
        year: parsed.year ?? undefined,
      });
      if (res) {
        overview = res.overview ?? null;
        runtimeMin = res.runtime ?? null;
      }
    } catch {
      // Keep the ADC-only content; the popup says which source it used.
    }

    setDetailsData({
      adcUrl,
      adcText,
      overview,
      runtimeMin,
      source: adcText ? 'adc' : overview ? 'tmdb' : 'none',
    });
    setDetailsLoading(false);
  };

  const fmtRuntime = (min: number | null | undefined) => {
    if (!min || min <= 0) return null;
    const h = Math.floor(min / 60);
    const m = min % 60;
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
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
            path: contentPath(item),
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
                path: contentPath(currentItem),
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

    // Poster art: object-cover trims the picture into the 660:800 frame.
    const posterArt = coverImage ? (
      <img
        src={localSrc(coverImage)}
        alt={currentItem.name}
        className="w-full h-full object-cover"
        onError={(e) => (e.target as HTMLElement).style.display = 'none'}
      />
    ) : (
      <div className="flex flex-col items-center justify-center text-[#333333] space-y-1 h-full">
        <Disc className="w-10 h-10" />
        <span className="text-[9px] font-mono uppercase tracking-widest text-[#737373]">
          No Art Yet
        </span>
      </div>
    );

    // Hover meta line: exactly the owner's list — year, disc formats, size,
    // duration. Grayscale only; nothing duplicated.
    const runtimeText = fmtRuntime(tmdbMap[group.id]?.runtime ?? null);
    const hoverMeta = [group.year, discBadge, formatBytes(group.total_bytes), runtimeText]
      .filter(Boolean)
      .join(' · ');

    return (
      <div
        key={group.id}
        onMouseEnter={() => setHoveredId(group.id)}
        onMouseLeave={() => setHoveredId((prev) => (prev === group.id ? null : prev))}
        onContextMenu={(e) => {
          e.preventDefault();
          setCtxMenu({ x: e.clientX, y: e.clientY, groupId: group.id, itemId: currentItem.id });
        }}
        className="group relative rounded-lg cursor-pointer w-[176px] flex-shrink-0"
      >
        <div className="rounded-lg overflow-hidden">
          <div className="aspect-[660/800] w-full bg-[#141414] relative overflow-hidden">
            {posterArt}

            {/* Hover: the lower HALF of the poster is replaced with the info
                panel — grayscale only, two action buttons (owner spec). */}
            {hoveredId === group.id && (
              <div className="absolute inset-x-0 bottom-0 h-1/2 bg-black/90 border-t border-[#3a3a3a] p-3 flex flex-col justify-between">
                <div className="space-y-1 min-h-0">
                  <p
                    className="text-[13px] font-semibold text-white leading-tight line-clamp-2"
                    title={group.clean_title}
                  >
                    {group.clean_title}
                  </p>
                  <div className="text-[10px] font-mono text-[#a3a3a3] leading-snug">{hoverMeta}</div>
                </div>
                <div className="flex items-center gap-1.5">
                  {resumePath && !isDownloading ? (
                    <button
                      onClick={() => handleResumeDisc(group.id, resumePath)}
                      className="flex-1 px-2 py-1.5 rounded-md bg-[#e5e5e5] hover:bg-white text-black text-[11px] font-semibold flex items-center justify-center gap-1 transition"
                      title="Resume the last-played disc past its menu"
                    >
                      <Play className="w-3 h-3 fill-black" />
                      <span>Resume</span>
                    </button>
                  ) : (
                    <button
                      onClick={() => handlePlayDisc(effDisc.torrent, group.id, effDisc.path)}
                      className="flex-1 px-2 py-1.5 rounded-md bg-[#e5e5e5] hover:bg-white text-black text-[11px] font-semibold flex items-center justify-center gap-1 transition"
                      title="Play (interactive disc menus)"
                    >
                      <Play className="w-3 h-3 fill-black" />
                      <span>Play</span>
                    </button>
                  )}
                  <button
                    onClick={() => openDetailsPopup(group.id, currentItem)}
                    className="flex-1 px-2 py-1.5 rounded-md border border-[#555555] text-[#e5e5e5] text-[11px] font-medium hover:bg-[#262626] hover:text-white transition"
                    title="Summary from ADC (when available) or TMDB/Trakt"
                  >
                    Details
                  </button>
                </div>
              </div>
            )}
          </div>

          {/* Title on the midline BELOW the poster (owner). */}
          <div className="pt-2">
            <h4
              className="text-[12px] font-medium text-white text-center truncate leading-tight"
              title={group.clean_title}
            >
              {group.clean_title}
            </h4>
            {variant === 'active' && (
              <>
                <div className="flex justify-end mt-1">
                  <span className="text-[11px] font-mono text-[#a3a3a3]">{progressPct}%</span>
                </div>
                <div className="h-[4px] w-full rounded-full bg-[#333333] overflow-hidden mt-1">
                  <div
                    className={`h-full transition-all duration-300 ${
                      currentItem.status === 'Seeding' ? 'bg-white' : 'bg-white'
                    }`}
                    style={{ width: `${progressPct}%` }}
                  />
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    );
  };

  return (
    <div className="flex-1 overflow-y-auto p-5 space-y-16 bg-[#141414] text-white select-none">
      {/* Top bar (Canva): Nebulae wordmark left; search + controls right.
          Dark palette per the owner's override of the Canva background. */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 pb-5 border-b border-[#262626]">
        <div className="flex items-center gap-2.5">
          <span className="w-[22px] h-[22px] rounded-full bg-[#e50914]" />
          <h2 className="text-2xl font-bold tracking-tight">Nebulae</h2>
        </div>

        <div className="flex flex-wrap items-center gap-2.5">
          <div
            className="flex items-center gap-2 px-3 py-2 rounded-lg bg-[#1f1f1f] border border-[#262626] w-56 focus-within:border-[#e50914]/50 transition"
            title="Search the library by release or folder name"
          >
            <Search className="w-4 h-4 text-[#737373]" />
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
            <div className="space-y-7">
              <div className="flex items-center gap-2">
                <ArrowDownCircle className="w-4 h-4 text-[#a3a3a3]" />
                <h3 className="text-[14px] font-semibold uppercase tracking-[0.15em] text-[#a3a3a3]">
                  Downloading
                </h3>
                <span className="text-[11px] font-mono text-white px-2 rounded-md bg-[#1f1f1f] border border-[#333333] flex items-center h-[22px]">
                  {activeDownloads.length}
                </span>
              </div>
              <div className="flex gap-6 overflow-x-auto pb-4 scrollbar-thin">
                {visibleGroups(groupItems(activeDownloads)).map((g) => renderGroupCard(g, 'active'))}
              </div>
            </div>
          )}

          {/* Shelf 2: Ready to Watch */}
          <div className="space-y-7">
            <div className="flex items-center gap-2">
              <Play className="w-4 h-4 text-[#e50914] fill-current" />
              <h3 className="text-[14px] font-semibold uppercase tracking-[0.15em] text-[#a3a3a3]">
                Ready to Watch
              </h3>
              <span className="text-[11px] font-mono text-white px-2 rounded-md bg-[#1f1f1f] border border-[#333333] flex items-center h-[22px]">
                {readyToWatch.length}
              </span>
            </div>
            {readyToWatch.length === 0 ? (
              <p className="text-xs text-[#737373] italic">
                No optical disc images or media downloaded yet. Completed releases will appear here.
              </p>
            ) : (
              <div className="flex gap-6 overflow-x-auto pb-4 scrollbar-thin">
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

      {/* Right-click menu on a poster (grayscale, owner spec + extras). */}
      {ctxMenu && (
        <div
          className="fixed inset-0 z-[190]"
          onClick={() => setCtxMenu(null)}
          onContextMenu={(e) => {
            e.preventDefault();
            setCtxMenu(null);
          }}
        >
          <div
            className="absolute w-44 rounded-lg bg-[#1c1c1c] border border-[#333333] shadow-2xl shadow-black/80 py-1.5"
            style={{
              left: Math.min(ctxMenu.x, window.innerWidth - 190),
              top: Math.min(ctxMenu.y, window.innerHeight - 210),
            }}
          >
            {(() => {
              const group = groupItems([...activeDownloads, ...readyToWatch]).find(
                (g) => g.id === ctxMenu.groupId,
              );
              const item =
                group?.items.find((t) => t.id === ctxMenu.itemId) || group?.items[0];
              if (!group || !item) return null;
              const saved = lastPlayedDiscs[group.id];
              const resumeTarget = saved
                ? saved.startsWith('/')
                  ? saved
                  : (discSetMap[item.id] ?? []).find((d) => d.play_target === saved)?.play_target
                : null;
              const act =
                'w-full text-left px-3 py-1.5 text-[11px] text-[#d4d4d4] hover:bg-[#262626] hover:text-white flex items-center gap-2 transition';
              return (
                <>
                  <button className={act} onClick={() => { setCtxMenu(null); handleRevealRelease(item); }}>
                    <FolderOpen className="w-3.5 h-3.5" /> Show in Finder
                  </button>
                  <button
                    className={act}
                    onClick={() => {
                      setCtxMenu(null);
                      handlePlayDisc(item, group.id);
                    }}
                  >
                    <Play className="w-3.5 h-3.5" /> Play
                  </button>
                  {resumeTarget && (
                    <button
                      className={act}
                      onClick={() => {
                        setCtxMenu(null);
                        handleResumeDisc(group.id, resumeTarget);
                      }}
                    >
                      <Play className="w-3.5 h-3.5" /> Resume
                    </button>
                  )}
                  <button
                    className={act}
                    onClick={() => {
                      setCtxMenu(null);
                      openDetailsPopup(group.id, item);
                    }}
                  >
                    <Info className="w-3.5 h-3.5" /> Details
                  </button>
                  <button
                    className={act}
                    onClick={() => {
                      navigator.clipboard?.writeText(contentPath(item)).catch(() => {});
                      setCtxMenu(null);
                    }}
                  >
                    <Copy className="w-3.5 h-3.5" /> Copy Path
                  </button>
                  <button
                    className={act}
                    onClick={() => {
                      setCtxMenu(null);
                      openDetails(item.id);
                    }}
                  >
                    <FolderArchive className="w-3.5 h-3.5" /> Open in Manager
                  </button>
                </>
              );
            })()}
          </div>
        </div>
      )}

      {/* Details popup: ADC website info when available, otherwise the
          TMDB/Trakt summary (owner). Grayscale. */}
      {detailsFor && (
        <div
          className="fixed inset-0 z-[200] bg-black/70 backdrop-blur-xs flex items-center justify-center p-4"
          onClick={() => setDetailsFor(null)}
        >
          <div
            className="w-full max-w-md rounded-xl bg-[#161616] border border-[#333333] shadow-2xl p-5 space-y-3"
            onClick={(e) => e.stopPropagation()}
          >
            {(() => {
              const group = groupItems([...activeDownloads, ...readyToWatch]).find(
                (g) => g.id === detailsFor.groupId,
              );
              const item =
                group?.items.find((t) => t.id === detailsFor.itemId) || group?.items[0];
              if (!group || !item) return null;
              const set = discSetMap[item.id] ?? [];
              const specs = Array.from(new Set(set.map((d) => d.layer_spec)));
              const count = set.length || group.items.length;
              const badge = specs.length
                ? specs.length === 1
                  ? `${count > 1 ? `${count}× ` : ''}${layerLabel(specs[0])}`
                  : specs.map(layerLabel).join(' + ')
                : null;
              const seed = seedTimeRecords.find((r) => r.info_hash === item.info_hash);
              const meta = [
                group.year,
                badge,
                formatBytes(group.total_bytes),
                fmtRuntime(detailsData?.runtimeMin ?? null),
              ]
                .filter(Boolean)
                .join(' · ');
              return (
                <>
                  <div className="flex items-start justify-between gap-3">
                    <div className="space-y-1 min-w-0">
                      <h3 className="text-[16px] font-semibold text-white leading-tight">
                        {group.clean_title}
                      </h3>
                      <div className="text-[11px] font-mono text-[#a3a3a3]">{meta}</div>
                    </div>
                    <button
                      onClick={() => setDetailsFor(null)}
                      className="p-1 rounded hover:bg-[#262626] text-[#a3a3a3] hover:text-white transition"
                    >
                      <X className="w-4 h-4" />
                    </button>
                  </div>

                  {detailsLoading ? (
                    <p className="text-[12px] text-[#737373] italic">Fetching details…</p>
                  ) : detailsData ? (
                    <div className="space-y-3 max-h-[50vh] overflow-y-auto">
                      {detailsData.adcText && (
                        <div className="space-y-1.5">
                          <span className="text-[10px] font-semibold uppercase tracking-widest text-[#8c8c8c]">
                            AsianDVDClub
                          </span>
                          <p className="text-[12px] text-[#d4d4d4] leading-relaxed whitespace-pre-line">
                            {detailsData.adcText}
                          </p>
                          {detailsData.adcUrl && (
                            <a
                              href={detailsData.adcUrl}
                              target="_blank"
                              rel="noreferrer"
                              className="text-[11px] text-[#a3a3a3] underline underline-offset-2 hover:text-white"
                            >
                              Open the ADC release page
                            </a>
                          )}
                          {seed && (
                            <p className="text-[11px] font-mono text-[#8c8c8c]">
                              Seed time {fmtSeedTime(seed.seeding_time_seconds) || '0m'} of{' '}
                              {fmtSeedTime(seed.required_seeding_seconds) || '—'} required
                              {seed.is_shield_satisfied ? ' · shield satisfied' : ''}
                            </p>
                          )}
                        </div>
                      )}
                      {detailsData.overview && (
                        <div className="space-y-1.5">
                          <span className="text-[10px] font-semibold uppercase tracking-widest text-[#8c8c8c]">
                            {detailsData.source === 'tmdb' ? 'TMDB / Trakt' : 'TMDB'}
                          </span>
                          <p className="text-[12px] text-[#a3a3a3] leading-relaxed">
                            {detailsData.overview}
                          </p>
                        </div>
                      )}
                      {!detailsData.adcText && !detailsData.overview && (
                        <p className="text-[12px] text-[#737373] italic">
                          No summary available{detailsData.adcUrl ? ' — the ADC page requires a website login' : ''}.
                        </p>
                      )}
                    </div>
                  ) : null}
                </>
              );
            })()}
          </div>
        </div>
      )}
    </div>
  );
};

export default SimpleMediaView;
