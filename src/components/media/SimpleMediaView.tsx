import React, { useState, useEffect, useCallback } from 'react';
import { useTorrentStore } from '../../stores/torrentStore';
import { useUiStore } from '../../stores/uiStore';
import { useTrackerStore } from '../../stores/trackerStore';
import { formatBytes } from '../../lib/formatters';
import { errorMessage } from '../../utils/errors';
import { resolvePoster } from '../../lib/posters';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { openInPlayer, resumeInPlayer, savedPositionMs, savePositionMs, usePlayerStore } from '../../lib/playerStore';
import {
  TorrentItem,
  DiscStructureInfo,
  OrganizeMediaReport,
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
  Folder,
  FolderArchive,
  X,
  Info,
  Copy, Trash2 } from 'lucide-react';

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

const isMac = typeof navigator !== 'undefined' && navigator.userAgent.toLowerCase().includes('mac');

/**
 * The torrent's ACTUAL content path. `save_path` is the download ROOT (the
 * engine reports the download dir), so disc-set detection, thumbnails, folder
 * display and playback all resolved against ~/Downloads instead of the
 * release folder — the lone "DVD-9" badge and wrong-disc playback. The
 * release folder/file is save_path + name.
 */
export const contentPath = (t: TorrentItem): string => {
  const isWindows = t.save_path.includes('\\');
  const sep = isWindows ? '\\' : '/';
  const base = t.save_path.replace(/[\\/]+$/, '');
  return base.endsWith(t.name) ? base : `${base}${sep}${t.name}`;
};

/** Folder to reveal in Finder/Explorer: the release folder, or the parent of a file. */
export const folderOf = (t: TorrentItem): string => {
  const p = contentPath(t);
  const isWindows = p.includes('\\');
  const sep = isWindows ? '\\' : '/';
  return /\.[a-z0-9]+$/i.test(p) ? p.split(/[/\\]/).slice(0, -1).join(sep) : p;
};

export const SimpleMediaView: React.FC = () => {
  const { torrents, setSelectedId } = useTorrentStore();
  const { setShowAddTorrent, setViewMode, searchQuery } = useUiStore();
  const { fetchSeedTimeRecords } = useTrackerStore();
  const playerOpen = usePlayerStore().open;

  const [discMetaMap, setDiscMetaMap] = useState<Record<string, DiscStructureInfo | null>>({});
  const [discSetMap, setDiscSetMap] = useState<Record<string, DiscStructureInfo[]>>({});
  // Read-only: per-disc selection now happens in the player's disc switcher;
  // the group's active item follows the last-played disc.
  const [selectedDiscs] = useState<Record<string, number>>({});  const [thumbMap, setThumbMap] = useState<Record<string, string | null>>({});
  const [tmdbMap, setTmdbMap] = useState<
    Record<string, { poster?: string; overview?: string; runtime?: number }>
  >({});
  const [groupBoxsets] = useState<boolean>(true);
  

  // Hover = the lower half of the poster is replaced by grayscale info
  // (owner). The overlay lives INSIDE the poster box, so nothing can clip
  // it and no portal is needed.
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [isLaunchingDisc, setIsLaunchingDisc] = useState(false);

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

  // Media player language preferences (owner): ISO language codes for
  // audio and subtitles, applied automatically by the player overlay.
  const [showPlayerPrefs, setShowPlayerPrefs] = useState(false);
  const [prefAudioLangs, setPrefAudioLangs] = useState(
    () => localStorage.getItem('nebulae_pref_audio_langs') || '',
  );
  const [prefSubsLangs, setPrefSubsLangs] = useState(
    () => localStorage.getItem('nebulae_pref_subs_langs') || '',
  );
  const savePlayerPrefs = () => {
    try {
      localStorage.setItem('nebulae_pref_audio_langs', prefAudioLangs.trim());
      localStorage.setItem('nebulae_pref_subs_langs', prefSubsLangs.trim());
    } catch {
      // Storage unavailable — the fields stay in memory for this session.
    }
    setShowPlayerPrefs(false);
  };

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
  const [externalTitlePrompt, setExternalTitlePrompt] = useState<{ path: string; guess: string } | null>(null);
  const [externalTitleInput, setExternalTitleInput] = useState('');
  const [feedbackNotice, setFeedbackNotice] = useState<{
    type: 'info' | 'error';
    message: string;
  } | null>(null);

  useEffect(() => {
    if (!feedbackNotice || feedbackNotice.type === 'error') return;
    const timer = setTimeout(() => setFeedbackNotice(null), 4000);
    return () => clearTimeout(timer);
  }, [feedbackNotice]);

  const [externalMedia, setExternalMedia] = useState<{ path: string; title: string; year?: number | null; kind: string }[]>([]);
  const [extHovered, setExtHovered] = useState<string | null>(null);
  const [extCtx, setExtCtx] = useState<{ x: number; y: number; path: string; title: string } | null>(null);
  const refreshExternalMedia = useCallback(async () => {
    try { setExternalMedia(await invoke('list_external_media')); } catch { /* best-effort */ }
  }, []);
  useEffect(() => { void refreshExternalMedia(); }, [refreshExternalMedia]);
  // Title-bar controls (media mode): Add Movie + Player prefs live in the
  // top bar and reach this component through window events.
  useEffect(() => {
    const onAdd = (e: Event) => {
      const custom = e as CustomEvent<{ kind?: 'folder' | 'file' }>;
      const kind = custom.detail?.kind;
      void handlePlayExternalDisc(kind);
    };
    const onPrefs = () => setShowPlayerPrefs(true);
    window.addEventListener('nebulae:add-movie', onAdd);
    window.addEventListener('nebulae:player-prefs', onPrefs);
    return () => {
      window.removeEventListener('nebulae:add-movie', onAdd);
      window.removeEventListener('nebulae:player-prefs', onPrefs);
    };
  }, []);
  // Library banners for external titles: same TMDB pipeline as downloads.
  const [externalPosterMap, setExternalPosterMap] = useState<Record<string, string | undefined>>({});
  // TMDB metadata per external path: overview / runtime / canonical year —
  // the SAME source the library Details popup uses.
  const [externalMetaMap, setExternalMetaMap] = useState<
    Record<string, { overview: string | null; runtime: number | null; year: number | null }>
  >({});
  useEffect(() => {
    if (playerOpen) return;
    externalMedia.forEach((m) => {
      if (m.path in externalPosterMap) return;
      setExternalPosterMap((prev) => ({ ...prev, [m.path]: undefined }));
      invoke<ParsedMediaMetadata>('parse_media_title', { rawTitle: m.title })
        .then((parsed) =>
          invoke<FetchTmdbResult>('fetch_tmdb_release', {
            title: parsed.clean_title || m.title,
            year: parsed.year ?? m.year ?? undefined,
          }),
        )
        .then((res) => {
          setExternalPosterMap((prev) => ({ ...prev, [m.path]: res?.poster_webp ?? undefined }));
          if (res) {
            setExternalMetaMap((prev) => ({
              ...prev,
              [m.path]: {
                overview: res.overview ?? null,
                runtime: res.runtime ?? null,
                year: res.year ?? m.year ?? null,
              },
            }));
          }
        })
        .catch(() => {
          setExternalPosterMap((prev) => ({ ...prev, [m.path]: undefined }));
        });
    });
  }, [externalMedia, externalPosterMap, playerOpen]);

  // Inspect optical disc format for external media items
  useEffect(() => {
    if (playerOpen) return;
    externalMedia.forEach((m) => {
      if (!(m.path in discSetMap)) {
        invoke<DiscStructureInfo[]>('detect_disc_set', { filePath: m.path })
          .then((set) => {
            setDiscSetMap((prev) => ({ ...prev, [m.path]: set }));
            const primary = set[0];
            if (primary) {
              setDiscMetaMap((prev) => ({ ...prev, [m.path]: primary }));
            }
          })
          .catch(() => {
            setDiscSetMap((prev) => ({ ...prev, [m.path]: [] }));
          });
      }
    });
  }, [externalMedia, discSetMap, playerOpen]);

  // Details popup for an external title (owner: same options/details as
  // the library).
  const [extDetails, setExtDetails] = useState<
    { path: string; title: string; year: number | null; kind: string } | null
  >(null);
  const extYear = extDetails
    ? externalMetaMap[extDetails.path]?.year ?? extDetails.year
    : null;
  const extMeta = extDetails ? externalMetaMap[extDetails.path] : undefined;
  // Disc structure for the external popup: same source the library uses.
  const [extDiscSet, setExtDiscSet] = useState<DiscStructureInfo[]>([]);
  useEffect(() => {
    if (!extDetails) return;
    setExtDiscSet([]);
    const probe = (p: string) =>
      invoke<DiscStructureInfo[]>('detect_disc_set', { filePath: p });
    probe(extDetails.path)
      .then(async (set) => {
        // Only probe parent if NO disc structure found in path AND parent is not a library root
        if (set.length === 0) {
          const libraryNames = new Set([
            'blu-ray', 'bluray', 'dvd', 'movies', 'films', 'media', 'cinema',
            'downloads', 'torrents', 'uhd', '4k', 'volumes'
          ]);
          const parent = extDetails.path.replace(/[/\\]+$/, '').split(/[/\\]/).slice(0, -1).join('/');
          const parentName = parent.split(/[/\\]/).pop()?.toLowerCase() || '';
          if (parent && parent !== extDetails.path && !libraryNames.has(parentName)) {
            try {
              const parentSet = await probe(parent);
              if (parentSet.length > 0) {
                setExtDiscSet(parentSet);
                return;
              }
            } catch { /* keep the single-disc result */ }
          }
        }
        setExtDiscSet(set);
      })
      .catch(() => {});
  }, [extDetails]);

  // ADC seed-time records power the hover line ("ADC: 2.48 · Seed time: 2d 14h").
  useEffect(() => {
    fetchSeedTimeRecords().catch(() => {});
  }, [fetchSeedTimeRecords]);

  // Inspect optical disc format for each media item, AND the full disc set
  // (multi-ISO torrents classify every disc by its own size — the badge, the
  // pills and the hover panel all read the set, not just the first image).
  useEffect(() => {
    if (playerOpen) return;
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
  }, [torrents, discSetMap, discMetaMap, playerOpen]);

  // Poster resolution (approved chain): 1) ADC poster (the artwork the
  // tracker already shows on torrents.php, matched by name and cached
  // locally per info-hash), 2) the WebP pipeline (cover scan → downscale →
  // cache → ffmpeg frame → TMDB when a key exists). Only missing entries
  // are fetched.
  useEffect(() => {
    torrents.forEach((t) => {
      if (t.id in thumbMap) return;
      resolvePoster(t.info_hash, t.name, contentPath(t))
        .then((src) => setThumbMap((prev) => ({ ...prev, [t.id]: src })))
        .catch(() => setThumbMap((prev) => ({ ...prev, [t.id]: null })));
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
      // Always fetch (DB-cached): the details modal needs the summary and
      // runtime even when a local cover exists (owner: summary missing for
      // locally-covered releases).
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

  // Feature label exactly as the card shows it: clean title + optional year.
  const featureLabel = (gid?: string) => {
    const g = groupItems([...activeDownloads, ...readyToWatch]).find((x) => x.id === gid);
    if (!g) return undefined;
    return g.year ? `${g.clean_title} (${g.year})` : g.clean_title;
  };

  const handlePlayDisc = async (
    torrent: TorrentItem,
    groupId?: string,
    targetPath?: string,
    displayTitle?: string,
    year?: number | string | null,
  ) => {
    if (isLaunchingDisc) return;
    setIsLaunchingDisc(true);
    const playPath = targetPath || contentPath(torrent);
    invoke('log_frontend', {
      message: `disc button clicked: id=${torrent.id} path=${playPath}`,
    }).catch(() => {});
    const allGroups = groupItems([...activeDownloads, ...readyToWatch]);
    const g = groupId ? allGroups.find((x) => x.id === groupId) : undefined;
    const resolvedYear = year ?? g?.year;
    try {
      const info = await openInPlayer(playPath, { displayTitle, year: resolvedYear });
      if (groupId) {
        // Remember the RESOLVED path (the actual ISO/VIDEO_TS) — the resume
        // position map and the disc pills key off exactly this path.
        rememberLastDisc(groupId, info.resolved_path || playPath);
      }
      setFeedbackNotice(null);
      setExternalDiscError(null);
    } catch (err: unknown) {
      // Surface the failure - a silently swallowed error looks like a dead button.
      const msg = errorMessage(err, 'Could not launch the disc.');
      invoke('log_frontend', { message: `disc open failed: ${msg}` }).catch(() => {});
      setFeedbackNotice({ type: 'error', message: msg });
      setExternalDiscError(msg);
    } finally {
      setIsLaunchingDisc(false);
    }
  };

  // Resume = skip the disc menu and continue the feature where the poller
  // saved it (owner: "Resume forwards me to the disc menu again").
  const handleResumeDisc = async (
    groupId: string,
    resumePath: string,
    displayTitle?: string,
    year?: number | string | null,
  ) => {
    const allGroups = groupItems([...activeDownloads, ...readyToWatch]);
    const g = allGroups.find((x) => x.id === groupId);
    const resolvedYear = year ?? g?.year;
    const savedMs = savedPositionMs(resumePath);
    invoke('log_frontend', {
      message: `resume clicked: path=${resumePath} savedMs=${savedMs}`,
    }).catch(() => {});
    try {
      const info = await resumeInPlayer(resumePath, savedMs, displayTitle, resolvedYear);
      rememberLastDisc(groupId, info.resolved_path || resumePath);
      setFeedbackNotice(null);
      setExternalDiscError(null);
    } catch (err: unknown) {
      const msg = errorMessage(err, 'Could not resume the disc.');
      invoke('log_frontend', { message: `resume failed: ${msg}` }).catch(() => {});
      setFeedbackNotice({ type: 'error', message: msg });
      setExternalDiscError(msg);
    }
  };

  const handleOpenFolder = async (path: string) => {
    try {
      await invoke('open_torrent_folder', { path });
      const name = path.split(/[/\\]/).filter(Boolean).pop() || path;
      const target = isMac ? 'Finder' : 'Explorer';
      setFeedbackNotice({ type: 'info', message: `Revealed in ${target}: ${name}` });
    } catch (err: unknown) {
      const target = isMac ? 'Finder' : 'Explorer';
      const msg = errorMessage(err, `Could not reveal folder in ${target}: ${path}`);
      setFeedbackNotice({ type: 'error', message: msg });
    }
  };

  const handleCopyPath = async (item: TorrentItem) => {
    const p = contentPath(item);
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(p);
      } else {
        const el = document.createElement('textarea');
        el.value = p;
        document.body.appendChild(el);
        el.select();
        document.execCommand('copy');
        document.body.removeChild(el);
      }
      setFeedbackNotice({ type: 'info', message: `Copied path to clipboard: ${p}` });
    } catch (err: unknown) {
      setFeedbackNotice({
        type: 'error',
        message: `Failed to copy path: ${errorMessage(err, 'clipboard unavailable')}`,
      });
    }
  };

  // Reveal the release's own folder in Finder (not the download root).
  const handleRevealRelease = (t: TorrentItem) => void handleOpenFolder(folderOf(t));

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
  // Import asks for the title when the folder/ISO name doesn't already
  // carry a "Name (Year)" pattern - metadata needs it (owner).
  const titleFromName = (p: string): string | null => {
    // Split on both / and \ to handle macOS/Linux and Windows paths.
    const name = p.split(/[/\\]/).filter(Boolean).pop() || p;
    // Check if name has (Year) or [Year] (e.g. "A Few Moments of Cheers (2024) UHD")
    const m = name.match(/^(.+?)\s*[\(\[](\d{4})[\)\]]/);
    if (m) {
      return `${m[1].trim()} (${m[2]})`;
    }
    // Also check parent folder if the selected item is a subfolder like BDMV or DISC_1
    const segments = p.split(/[/\\]/).filter(Boolean);
    if (segments.length >= 2) {
      const parentName = segments[segments.length - 2];
      const pm = parentName.match(/^(.+?)\s*[\(\[](\d{4})[\)\]]/);
      if (pm) {
        return `${pm[1].trim()} (${pm[2]})`;
      }
    }
    return null;
  };
  const importExternal = async (p: string) => {
    const known = titleFromName(p);
    if (known) {
      try {
        await invoke('add_external_media', { path: p, title: known });
        await refreshExternalMedia();
      } catch (err: unknown) {
        setExternalDiscError(errorMessage(err, 'Could not add external media.'));
      }
    } else {
      const name = p.split(/[/\\]/).filter(Boolean).pop() || '';
      const guess = name.replace(/[._]/g, ' ').replace(/\.[^.]+$/, '').trim();
      setExternalTitlePrompt({ path: p, guess });
    }
  };
  // The dialog accepts the release ROOT folder (holding all disc folders,
  // like a torrent download) OR an ISO disc image.
  const handlePlayExternalDisc = async (kind?: 'folder' | 'file') => {
    setExternalDiscError(null);
    try {
      const src = await invoke<string | null>('select_disc_source', { kind: kind || null });
      if (!src) return;
      await importExternal(src);
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
    const layerLabel = (ls?: string) =>
    (({ Dvd5: 'DVD-5', Dvd9: 'DVD-9', Bd25: 'BD-25', Bd50: 'BD-50', UhdBd66: 'UHD-66', UhdBd100: 'UHD-100' }) as Record<string, string>)[ls || ''] || 'Disc';

  const renderGroupCard = (group: MediaGroup, variant: 'active' | 'watch' | 'seeding') => {
    // A remembered last-played disc wins as the card's face (and what
    // "Resume" re-opens) until the owner picks a different disc tab.
    const currentItem = group.items[group.active_index] || group.items[0];
    const discInfo = discMetaMap[currentItem.id];
    const progressPct = Math.round(group.overall_progress * 100);
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
              title: d.play_target.split(/[/\\]/).pop() || d.play_target,
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
    // An absolute path starts with / (Unix) or a drive letter (Windows: C:\).
    const isAbsPath = (k: string) => k.startsWith('/') || /^[A-Za-z]:[/\\]/.test(k);
    const resumePath = rememberedKey
      ? isAbsPath(rememberedKey)
        ? rememberedKey
        : (discs.find((d) => d.key === rememberedKey)?.path ?? effDisc.path)
      : null;

    // Aggregated disc badge from the FULL disc set (multi-ISO torrents count
    // every disc by its own size), owner's mock format: "DVD-9 ×2", mixed
    // sets read "DVD-9 + DVD-5".

    // Poster art: object-cover trims the picture into the 660:800 frame.
    const posterArt = coverImage ? (
      <img
        src={localSrc(coverImage)}
        alt={currentItem.name}
        className="w-full h-full object-cover rounded-lg"
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

    // Hover overlay (owner): ONLY the Play and Details buttons, floating
    // over the poster with NO background panel and no data — all data lives
    // in the Details popup now.

    return (
      <div
        key={group.id}
        onMouseEnter={() => setHoveredId(group.id)}
        onMouseLeave={() => setHoveredId((prev) => (prev === group.id ? null : prev))}
        onContextMenu={(e) => {
          e.preventDefault();
          setCtxMenu({ x: e.clientX, y: e.clientY, groupId: group.id, itemId: currentItem.id });
        }}
        className="group relative rounded-lg cursor-pointer w-[150px] flex-shrink-0"
      >
        <div className="rounded-lg overflow-hidden">
          <div className="aspect-[660/800] w-full rounded-lg bg-[#141414] relative overflow-hidden">
            {posterArt}

            {/* Hover: only Play + Details, floating on the poster side by
                side — no panel background, no data (owner). Play ALWAYS
                starts at the disc's interactive menu (owner: "when user is
                clicking play, the media player window should start with
                menu first"); resuming past the menu stays an explicit
                action (the right-click menu's Resume). */}
            {hoveredId === group.id && (
              <div className="absolute inset-x-0 bottom-0 p-1.5 flex items-stretch gap-1">
                {(() => {
                  const onPrimary = () => {
                    if (resumePath) savePositionMs(resumePath, 0);
                    handlePlayDisc(effDisc.torrent, group.id, effDisc.path, featureLabel(group.id), group.year);
                  };
                  return (
                    <button
                      onClick={onPrimary}
                      className="flex-1 px-1.5 py-1.5 rounded-md bg-[#e5e5e5] hover:bg-white text-black text-[10px] font-semibold flex items-center justify-center gap-1 transition"
                      title="Play (interactive disc menus)"
                    >
                      <Play className="w-3 h-3 fill-black" />
                      <span>Play</span>
                    </button>
                  );
                })()}
                <button
                  onClick={() => openDetailsPopup(group.id, currentItem)}
                  className="flex-1 px-1.5 py-1.5 rounded-md bg-[#2a2a2a] hover:bg-[#333333] border border-[#3a3a3a] text-[#e5e5e5] text-[10px] font-medium flex items-center justify-center transition"
                  title="Name, year, duration, disc sizes and summary"
                >
                  Details
                </button>
              </div>
            )}
          </div>

          {/* Title on the midline BELOW the poster (owner). */}
          <div className="pt-2">
            <h4
              className="text-[12px] font-medium text-white text-center leading-tight line-clamp-2 break-words"
              title={group.clean_title}
            >
              {group.clean_title}
              {group.year ? <span className="text-[#a3a3a3]"> ({group.year})</span> : null}
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
      {feedbackNotice && (
        <div
          className={`sticky top-2 z-[195] mx-auto max-w-xl p-3 rounded-lg flex items-center justify-between gap-3 shadow-2xl backdrop-blur-md transition ${
            feedbackNotice.type === 'error'
              ? 'bg-[#2a1518]/95 border border-[#e50914] text-[#ffb3b3]'
              : 'bg-[#152a18]/95 border border-[#22c55e] text-[#bbf7d0]'
          }`}
        >
          <div className="flex items-center gap-2 text-xs font-medium">
            <span>{feedbackNotice.type === 'error' ? '⚠️' : '✓'}</span>
            <span>{feedbackNotice.message}</span>
          </div>
          <button
            onClick={() => setFeedbackNotice(null)}
            className="text-current opacity-70 hover:opacity-100 text-sm font-bold ml-2"
          >
            ✕
          </button>
        </div>
      )}

      {externalDiscError && (
        <div className="p-2.5 rounded-lg bg-[#2a1518] border border-[#e50914]/40 text-[11px] text-[#ffb3b3]">
          {externalDiscError}
        </div>
      )}

      {/* Import title prompt (owner): asked when the imported folder/ISO
          name carries no "Name (Year)" pattern, so metadata resolves. */}
      {externalTitlePrompt && (
        <div className="fixed inset-0 z-[200] flex items-center justify-center bg-black/70">
          <div className="w-[380px] p-5 rounded-xl bg-[#141414] border border-[#333333] shadow-2xl space-y-3">
            <h3 className="text-sm font-semibold text-white">Name this title</h3>
            <p className="text-[11px] text-[#a3a3a3]">
              The folder name does not contain a movie name and year. Enter it as
              <span className="text-white"> Name (Year)</span> so metadata is imported correctly.
            </p>
            <input
              autoFocus
              value={externalTitleInput || externalTitlePrompt.guess}
              onChange={(e) => setExternalTitleInput(e.target.value)}
              placeholder="e.g. Fuuka (2017)"
              className="w-full px-3 py-2 rounded-lg bg-[#1f1f1f] border border-[#333333] text-xs text-white focus:outline-none focus:border-[#e50914]/60"
              onKeyDown={async (e) => {
                if (e.key !== 'Enter') return;
                const title = (externalTitleInput || externalTitlePrompt.guess).trim();
                const { path } = externalTitlePrompt;
                setExternalTitlePrompt(null);
                setExternalTitleInput('');
                try {
                  await invoke('add_external_media', { path, title });
                  await refreshExternalMedia();
                } catch (err) {
                  setExternalDiscError(errorMessage(err, 'Could not add external media.'));
                }
              }}
            />
            <div className="flex justify-end gap-2">
              <button
                onClick={() => { setExternalTitlePrompt(null); setExternalTitleInput(''); }}
                className="px-3 py-1.5 rounded-lg text-xs text-[#a3a3a3] hover:text-white transition"
              >
                Cancel
              </button>
              <button
                onClick={async () => {
                  const title = (externalTitleInput || externalTitlePrompt.guess).trim();
                  const { path } = externalTitlePrompt;
                  setExternalTitlePrompt(null);
                  setExternalTitleInput('');
                  try {
                    await invoke('add_external_media', { path, title });
                    await refreshExternalMedia();
                  } catch (err) {
                    setExternalDiscError(errorMessage(err, 'Could not add external media.'));
                  }
                }}
                className="px-3 py-1.5 rounded-lg bg-[#e50914] hover:bg-[#e50914]/90 text-xs font-semibold text-white transition"
              >
                Add
              </button>
            </div>
          </div>
        </div>
      )}

      {/* External media collection (owner): below the downloaded section;
          entries persist while their source path exists. */}
      {externalMedia.length > 0 && (
        <div className="mt-8">
          <h3 className="text-[14px] font-semibold uppercase tracking-[0.15em] text-[#a3a3a3] mb-3">
            External
          </h3>
          {/* Same card geometry and options as the Library shelf (owner):
              150px posters, hover Play, right-click menu. */}
          <div className="flex flex-wrap gap-4">
            {externalMedia.map((m) => {
              const discInfo = discMetaMap[m.path];
              const extCover = discInfo?.cover_image || externalPosterMap[m.path];
              return (
                <div
                  key={m.path}
                  className="group relative rounded-lg cursor-pointer w-[150px] flex-shrink-0"
                  onMouseEnter={() => setExtHovered(m.path)}
                  onMouseLeave={() => setExtHovered((prev) => (prev === m.path ? null : prev))}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    setExtCtx({ x: e.clientX, y: e.clientY, path: m.path, title: m.title });
                  }}
                >
                  <div className="rounded-lg overflow-hidden">
                    <div className="aspect-[660/800] w-full rounded-lg bg-[#141414] relative overflow-hidden border border-[#262626] group-hover:border-[#e50914]/50 transition">
                      {extCover ? (
                        <img
                          src={localSrc(extCover as string)}
                          alt={m.title}
                          className="w-full h-full object-cover"
                          draggable={false}
                        />
                      ) : (
                        <div className="w-full h-full flex items-center justify-center bg-[#0d0d0d]">
                          <Disc className="w-8 h-8 text-[#333333] group-hover:text-[#e50914]/60 transition" />
                        </div>
                      )}
                      {extHovered === m.path && (
                        <div className="absolute inset-x-0 bottom-0 p-1.5 flex items-stretch gap-1">
                          <button
                            onClick={async () => {
                              try {
                                await openInPlayer(m.path, {
                                  displayTitle: m.title,
                                  year: m.year ?? externalMetaMap[m.path]?.year,
                                });
                                void refreshExternalMedia();
                              } catch (err) {
                                setExternalDiscError(errorMessage(err, 'Could not launch the disc.'));
                              }
                            }}
                            className="flex-1 px-1.5 py-1.5 rounded-md bg-[#e5e5e5] hover:bg-white text-black text-[10px] font-semibold flex items-center justify-center gap-1 transition"
                            title="Play (interactive disc menus)"
                          >
                            <Play className="w-3 h-3 fill-black" />
                            <span>Play</span>
                          </button>
                          <button
                            onClick={() =>
                              setExtDetails({ path: m.path, title: m.title, year: m.year ?? null, kind: m.kind })
                            }
                            className="flex-1 px-1.5 py-1.5 rounded-md bg-[#2a2a2a] hover:bg-[#333333] border border-[#3a3a3a] text-[#e5e5e5] text-[10px] font-medium flex items-center justify-center transition"
                            title="Detailed movie information"
                          >
                            Details
                          </button>
                        </div>
                      )}
                    </div>
                  </div>
                  <div className="pt-2">
                    <h4
                      className="text-[12px] font-medium text-white text-center leading-tight line-clamp-2 break-words"
                      title={m.title}
                    >
                      {m.title}
                      {(externalMetaMap[m.path]?.year ?? m.year) ? (
                        <span className="text-[#a3a3a3]"> ({externalMetaMap[m.path]?.year ?? m.year})</span>
                      ) : null}
                    </h4>

                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {torrents.length === 0 && externalMedia.length === 0 ? (
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
          <div className="flex items-center gap-3">
            <button
              onClick={() => void handlePlayExternalDisc('folder')}
              className="flex items-center gap-2 px-5 py-2.5 rounded-full bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold transition shadow-md"
            >
              <Folder className="w-4 h-4" />
              <span>Add Movie Folder</span>
            </button>
            <button
              onClick={() => void handlePlayExternalDisc('file')}
              className="flex items-center gap-2 px-5 py-2.5 rounded-full bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-semibold transition shadow-md"
            >
              <Disc className="w-4 h-4" />
              <span>Add Disc Image (.iso)</span>
            </button>
            <button
              onClick={() => setShowAddTorrent(true)}
              className="flex items-center gap-2 px-5 py-2.5 rounded-full bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-semibold transition shadow-md"
            >
              <Plus className="w-4 h-4" />
              <span>Add Torrent</span>
            </button>
          </div>
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
              </div>
              <div className="flex gap-6 overflow-x-auto pb-4 scrollbar-thin">
                {visibleGroups(groupItems(activeDownloads)).map((g) => renderGroupCard(g, 'active'))}
              </div>
            </div>
          )}

          {/* Shelf 2: Library (owner: no icon, no count). */}
          <div className="space-y-7">
            <div className="flex items-center gap-2">
              <h3 className="text-[14px] font-semibold uppercase tracking-[0.15em] text-[#a3a3a3]">
                Downloaded
              </h3>
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

      {/* EXTERNAL details popup (owner): detailed movie information from
          TMDB - the same source the library Details uses. */}
      {extDetails && (
        <div
          className="fixed inset-0 z-[200] flex items-center justify-center bg-black/70 backdrop-blur-xs p-4"
          onClick={() => setExtDetails(null)}
        >
          <div
            className="w-full max-w-[480px] rounded-2xl bg-[#141414] border border-[#262626] shadow-2xl shadow-black/80 p-6 space-y-4 text-white"
            onClick={(e) => e.stopPropagation()}
          >
            {(() => {
              const cover = discMetaMap[extDetails.path]?.cover_image || externalPosterMap[extDetails.path];
              const discInfo = discMetaMap[extDetails.path];
              const totalBytes = extDiscSet.reduce((a, d) => a + (d.total_bytes || 0), 0) || (discInfo?.total_bytes || 0);
              const discCount = extDiscSet.length > 0 ? extDiscSet.length : (discInfo ? 1 : 0);
              const specs = extDiscSet.length > 0
                ? Array.from(new Set(extDiscSet.map((d) => d.layer_spec).filter(Boolean)))
                : (discInfo?.layer_spec ? [discInfo.layer_spec] : []);
              const formatBadge = specs.length
                ? specs.length === 1
                  ? `${discCount > 1 ? `${discCount}x ` : ''}${layerLabel(specs[0])}`
                  : specs.map(layerLabel).join(' + ')
                : (discInfo?.layer_spec ? layerLabel(discInfo.layer_spec) : (extDetails.kind || 'Disc'));
              const formatStr = specs.length
                ? specs.length === 1
                  ? `${discCount > 1 ? `${discCount}× ` : ''}${layerLabel(specs[0])}`
                  : specs.map(layerLabel).join(' + ')
                : (discInfo?.layer_spec ? layerLabel(discInfo.layer_spec) : (extDetails.kind || 'Disc'));
              const durationStr = extMeta?.runtime ? (fmtRuntime(extMeta.runtime) ?? `${extMeta.runtime}m`) : '—';
              const totalSizeStr = totalBytes > 0 ? formatBytes(totalBytes) : '—';

              const discRows = extDiscSet.length > 0
                ? extDiscSet.map((d, i) => {
                    const name = d.title || (d.root_path ? (d.root_path.split(/[/\\]/).filter(Boolean).pop() || `Disc ${d.disc_number ?? i + 1}`) : `Disc ${d.disc_number ?? i + 1}`);
                    return {
                      key: d.root_path || i,
                      title: name,
                      spec: layerLabel(d.layer_spec),
                      size: formatBytes(d.total_bytes),
                    };
                  })
                : discInfo
                  ? [{
                      key: extDetails.path,
                      title: extDetails.path.split(/[/\\]/).filter(Boolean).pop() || extDetails.title,
                      spec: layerLabel(discInfo.layer_spec),
                      size: formatBytes(discInfo.total_bytes || totalBytes),
                    }]
                  : [];

              return (
                <>
                  <div className="flex gap-4 items-start">
                    <div className="w-[110px] shrink-0">
                      <div className="aspect-[2/3] w-full rounded-xl overflow-hidden bg-[#0d0d0d] border border-[#262626]">
                        {cover ? (
                          <img
                            src={localSrc(cover as string)}
                            alt={extDetails.title}
                            className="w-full h-full object-cover"
                            draggable={false}
                          />
                        ) : (
                          <div className="w-full h-full flex items-center justify-center">
                            <Disc className="w-8 h-8 text-[#333333]" />
                          </div>
                        )}
                      </div>
                    </div>
                    <div className="flex-1 min-w-0 space-y-2">
                      <div className="flex items-start justify-between gap-2">
                        <h3 className="text-base font-bold text-white leading-tight">
                          {extDetails.title}
                          {extYear ? <span className="text-[#a3a3a3] font-normal"> ({extYear})</span> : null}
                        </h3>
                        <button
                          type="button"
                          onClick={() => setExtDetails(null)}
                          className="p-1 -mr-1 -mt-1 text-[#737373] hover:text-white transition"
                          title="Close"
                        >
                          <X className="w-4 h-4" />
                        </button>
                      </div>
                      <div className="flex flex-wrap gap-1.5 font-mono">
                        {formatBadge && (
                          <span className="px-2 py-0.5 rounded bg-[#262626] border border-[#333333] text-[11px] text-[#d4d4d4]">
                            {formatBadge}
                          </span>
                        )}
                        {extMeta?.runtime ? (
                          <span className="px-2 py-0.5 rounded bg-[#262626] border border-[#333333] text-[11px] text-[#d4d4d4]">
                            {extMeta.runtime} min
                          </span>
                        ) : null}
                      </div>
                      {extMeta?.overview ? (
                        <p className="text-xs text-[#a3a3a3] leading-relaxed line-clamp-4">
                          {extMeta.overview}
                        </p>
                      ) : (
                        <p className="text-xs text-[#737373] italic">
                          No TMDB summary available for this title.
                        </p>
                      )}
                    </div>
                  </div>

                  {/* Facts 2x2 Grid */}
                  <div className="rounded-xl bg-[#0f0f0f] border border-[#262626] p-4">
                    <div className="grid grid-cols-2 gap-y-3 gap-x-4 font-mono">
                      <div>
                        <span className="block text-[10px] tracking-wider text-[#737373] uppercase mb-1">DURATION</span>
                        <span className="block text-sm text-white">{durationStr}</span>
                      </div>
                      <div>
                        <span className="block text-[10px] tracking-wider text-[#737373] uppercase mb-1">TOTAL SIZE</span>
                        <span className="block text-sm text-white">{totalSizeStr}</span>
                      </div>
                      <div>
                        <span className="block text-[10px] tracking-wider text-[#737373] uppercase mb-1">DISCS</span>
                        <span className="block text-sm text-white">{discCount}</span>
                      </div>
                      <div>
                        <span className="block text-[10px] tracking-wider text-[#737373] uppercase mb-1">FORMAT</span>
                        <span className="block text-sm text-white">{formatStr}</span>
                      </div>
                    </div>
                  </div>

                  {/* Discs list */}
                  {discRows.length > 0 && (
                    <div className="space-y-2">
                      <span className="block text-[10px] font-mono tracking-wider text-[#737373] uppercase">DISCS</span>
                      <div className="space-y-1.5 max-h-36 overflow-y-auto pr-0.5">
                        {discRows.map((d, i) => (
                          <div
                            key={d.key || i}
                            className="flex items-center justify-between px-3 py-2 rounded-lg bg-[#0f0f0f] border border-[#262626] font-mono text-xs"
                          >
                            <span className="text-white font-medium truncate pr-2">
                              {d.title}
                            </span>
                            <span className="text-[#8c8c8c] shrink-0">
                              {d.spec} · {d.size}
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Action buttons */}
                  <div className="grid grid-cols-2 gap-3 pt-1">
                    <button
                      type="button"
                      onClick={async () => {
                        setExtDetails(null);
                        try {
                          await openInPlayer(extDetails.path, { displayTitle: extDetails.title, year: extYear });
                          void refreshExternalMedia();
                        } catch (err) {
                          setExternalDiscError(errorMessage(err, 'Could not launch the disc.'));
                        }
                      }}
                      className="w-full py-2.5 rounded-lg bg-[#e5e5e5] hover:bg-white text-black text-xs font-semibold transition text-center"
                    >
                      Play
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        setExtDetails(null);
                        invoke('open_torrent_folder', { path: extDetails.path }).catch(() => {});
                      }}
                      className="w-full py-2.5 rounded-lg bg-[#1a1a1a] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium transition text-center"
                    >
                      {isMac ? 'Show in Finder' : 'Show in Explorer'}
                    </button>
                  </div>
                </>
              );
            })()}
          </div>
        </div>
      )}

      {/* Right-click menu on an EXTERNAL poster: same options as the
          library where they apply (Show in Finder / Explorer, Play) + Remove. */}
      {extCtx && (
        <div
          className="fixed inset-0 z-[190]"
          onClick={() => setExtCtx(null)}
          onContextMenu={(e) => {
            e.preventDefault();
            setExtCtx(null);
          }}
        >
          <div
            className="absolute w-44 rounded-lg bg-[#1c1c1c] border border-[#333333] shadow-2xl shadow-black/80 py-1.5"
            onClick={(e) => e.stopPropagation()}
            style={{
              left: Math.min(extCtx.x, window.innerWidth - 190),
              top: Math.min(extCtx.y, window.innerHeight - 210),
            }}
          >
            {(() => {
              const act =
                'w-full text-left px-3 py-1.5 text-[11px] text-[#d4d4d4] hover:bg-[#262626] hover:text-white flex items-center gap-2 transition';
              return (
                <>
                  <button
                    className={act}
                    onClick={() => {
                      setExtCtx(null);
                      void handleOpenFolder(extCtx.path);
                    }}
                  >
                    <FolderOpen className="w-3.5 h-3.5" /> {isMac ? 'Show in Finder' : 'Show in Explorer'}
                  </button>
                  <button
                    className={act}
                    onClick={async () => {
                      setExtCtx(null);
                      try {
                        await openInPlayer(extCtx.path, {
                          displayTitle: extCtx.title,
                          year: externalMetaMap[extCtx.path]?.year,
                        });
                        void refreshExternalMedia();
                        setFeedbackNotice(null);
                      } catch (err) {
                        setFeedbackNotice({
                          type: 'error',
                          message: errorMessage(err, 'Could not launch the disc.'),
                        });
                      }
                    }}
                  >
                    <Play className="w-3.5 h-3.5" /> Play
                  </button>
                  <button
                    className={act}
                    onClick={async () => {
                      setExtCtx(null);
                      try {
                        await invoke('remove_external_media', { path: extCtx.path });
                        void refreshExternalMedia();
                        setFeedbackNotice({
                          type: 'info',
                          message: `Removed from library: ${extCtx.title}`,
                        });
                      } catch (err) {
                        setFeedbackNotice({
                          type: 'error',
                          message: errorMessage(err, 'Could not remove external media.'),
                        });
                      }
                    }}
                  >
                    <Trash2 className="w-3.5 h-3.5" /> Remove from Library
                  </button>
                </>
              );
            })()}
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
            onClick={(e) => e.stopPropagation()}
            style={{
              left: Math.min(ctxMenu.x, window.innerWidth - 190),
              top: Math.min(ctxMenu.y, window.innerHeight - 210),
            }}
          >
            {(() => {
              const item =
                torrents.find((t) => t.id === ctxMenu.itemId) ||
                groupItems(torrents).flatMap((g) => g.items).find((t) => t.id === ctxMenu.itemId);
              const group =
                groupItems(torrents).find((g) => g.id === ctxMenu.groupId) ||
                (item
                  ? {
                      id: item.id,
                      clean_title: item.media_meta?.clean_title || item.name,
                      year: item.media_meta?.year,
                      is_boxset: false,
                      total_bytes: item.total_bytes,
                      overall_progress: item.progress,
                      items: [item],
                      active_index: 0,
                    }
                  : null);
              if (!group || !item) return null;
              const saved = lastPlayedDiscs[group.id];
              const isAbsPath = (k: string) => k.startsWith('/') || /^[A-Za-z]:[/\\]/.test(k);
              const resumeTarget = saved
                ? isAbsPath(saved)
                  ? saved
                  : (discSetMap[item.id] ?? []).find((d) => d.play_target === saved)?.play_target
                : null;
              const act =
                'w-full text-left px-3 py-1.5 text-[11px] text-[#d4d4d4] hover:bg-[#262626] hover:text-white flex items-center gap-2 transition';
              return (
                <>
                  <button
                    className={act}
                    onClick={() => {
                      setCtxMenu(null);
                      handleRevealRelease(item);
                    }}
                  >
                    <FolderOpen className="w-3.5 h-3.5" /> {isMac ? 'Show in Finder' : 'Show in Explorer'}
                  </button>
                  <button
                    className={act}
                    onClick={() => {
                      setCtxMenu(null);
                      handlePlayDisc(item, group.id, undefined, featureLabel(group.id), group.year);
                    }}
                  >
                    <Play className="w-3.5 h-3.5" /> Play
                  </button>
                  {resumeTarget && (
                    <button
                      className={act}
                      onClick={() => {
                        setCtxMenu(null);
                        handleResumeDisc(group.id, resumeTarget, featureLabel(group.id), group.year);
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
                      setCtxMenu(null);
                      void handleCopyPath(item);
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

      {/* Details popup: reference layout matching external popup */}
      {detailsFor && (
        <div
          className="fixed inset-0 z-[200] bg-black/70 backdrop-blur-xs flex items-center justify-center p-4"
          onClick={() => setDetailsFor(null)}
        >
          <div
            className="w-full max-w-[480px] rounded-2xl bg-[#141414] border border-[#262626] shadow-2xl shadow-black/80 p-6 space-y-4 text-white"
            onClick={(e) => e.stopPropagation()}
          >
            {(() => {
              const item =
                torrents.find((t) => t.id === detailsFor.itemId) ||
                groupItems(torrents).flatMap((g) => g.items).find((t) => t.id === detailsFor.itemId);
              const group =
                groupItems(torrents).find((g) => g.id === detailsFor.groupId) ||
                (item
                  ? {
                      id: item.id,
                      clean_title: item.media_meta?.clean_title || item.name,
                      year: item.media_meta?.year,
                      is_boxset: false,
                      total_bytes: item.total_bytes,
                      overall_progress: item.progress,
                      items: [item],
                      active_index: 0,
                    }
                  : null);
              if (!group || !item) {
                return (
                  <div className="space-y-4">
                    <div className="flex items-center justify-between">
                      <h3 className="text-sm font-semibold text-white">Details Unavailable</h3>
                      <button
                        onClick={() => setDetailsFor(null)}
                        className="p-1 rounded hover:bg-[#262626] text-[#a3a3a3] hover:text-white"
                      >
                        <X className="w-4 h-4" />
                      </button>
                    </div>
                    <p className="text-xs text-[#a3a3a3]">
                      Information for this release could not be loaded because the torrent is not present in the active library.
                    </p>
                  </div>
                );
              }
              const set = discSetMap[item.id] ?? [];
              const discInfo = discMetaMap[item.id];
              const specs = set.length > 0
                ? Array.from(new Set(set.map((d) => d.layer_spec).filter(Boolean)))
                : (discInfo?.layer_spec ? [discInfo.layer_spec] : []);
              const count = set.length || group.items.length || 1;
              const formatBadge = specs.length
                ? specs.length === 1
                  ? `${count > 1 ? `${count}x ` : ''}${layerLabel(specs[0])}`
                  : specs.map(layerLabel).join(' + ')
                : (discInfo?.layer_spec ? layerLabel(discInfo.layer_spec) : 'Disc');
              const formatStr = specs.length
                ? specs.length === 1
                  ? `${count > 1 ? `${count}× ` : ''}${layerLabel(specs[0])}`
                  : specs.map(layerLabel).join(' + ')
                : (discInfo?.layer_spec ? layerLabel(discInfo.layer_spec) : 'Disc');
              const durationStr = detailsData?.runtimeMin ? (fmtRuntime(detailsData.runtimeMin) ?? `${detailsData.runtimeMin}m`) : '—';
              const totalSizeStr = formatBytes(group.total_bytes);

              const cover =
                (discSetMap[item.id] ?? [])[0]?.cover_image ||
                thumbMap[item.id] ||
                tmdbMap[group.id]?.poster ||
                item.poster_url;

              const discRows = set.length > 0
                ? set.map((d, i) => {
                    const name = d.title || (d.root_path ? (d.root_path.split(/[/\\]/).filter(Boolean).pop() || `Disc ${d.disc_number ?? i + 1}`) : `Disc ${d.disc_number ?? i + 1}`);
                    return {
                      key: d.root_path || i,
                      title: name,
                      spec: layerLabel(d.layer_spec),
                      size: formatBytes(d.total_bytes),
                    };
                  })
                : discInfo
                  ? [{
                      key: item.id,
                      title: item.name,
                      spec: layerLabel(discInfo.layer_spec),
                      size: formatBytes(item.total_bytes),
                    }]
                  : [];

              return (
                <>
                  <div className="flex gap-4 items-start">
                    <div className="w-[110px] shrink-0">
                      <div className="aspect-[2/3] w-full rounded-xl overflow-hidden bg-[#0d0d0d] border border-[#262626]">
                        {cover ? (
                          <img
                            src={localSrc(cover)}
                            alt={group.clean_title}
                            className="w-full h-full object-cover"
                            draggable={false}
                          />
                        ) : (
                          <div className="w-full h-full flex items-center justify-center">
                            <Disc className="w-8 h-8 text-[#333333]" />
                          </div>
                        )}
                      </div>
                    </div>
                    <div className="flex-1 min-w-0 space-y-2">
                      <div className="flex items-start justify-between gap-2">
                        <h3 className="text-base font-bold text-white leading-tight">
                          {group.clean_title}
                          {group.year ? (
                            <span className="text-[#a3a3a3] font-normal"> ({group.year})</span>
                          ) : null}
                        </h3>
                        <button
                          type="button"
                          onClick={() => setDetailsFor(null)}
                          className="p-1 -mr-1 -mt-1 text-[#737373] hover:text-white transition"
                          title="Close"
                        >
                          <X className="w-4 h-4" />
                        </button>
                      </div>
                      <div className="flex flex-wrap gap-1.5 font-mono">
                        {formatBadge && (
                          <span className="px-2 py-0.5 rounded bg-[#262626] border border-[#333333] text-[11px] text-[#d4d4d4]">
                            {formatBadge}
                          </span>
                        )}
                        {detailsData?.runtimeMin ? (
                          <span className="px-2 py-0.5 rounded bg-[#262626] border border-[#333333] text-[11px] text-[#d4d4d4]">
                            {detailsData.runtimeMin} min
                          </span>
                        ) : null}
                      </div>
                      {detailsLoading ? (
                        <p className="text-xs text-[#737373] italic">Fetching details…</p>
                      ) : detailsData?.overview ? (
                        <p className="text-xs text-[#a3a3a3] leading-relaxed line-clamp-4">
                          {detailsData.overview}
                        </p>
                      ) : (
                        <p className="text-xs text-[#737373] italic">
                          No summary available for this title.
                        </p>
                      )}
                    </div>
                  </div>

                  {/* Facts 2x2 Grid */}
                  <div className="rounded-xl bg-[#0f0f0f] border border-[#262626] p-4">
                    <div className="grid grid-cols-2 gap-y-3 gap-x-4 font-mono">
                      <div>
                        <span className="block text-[10px] tracking-wider text-[#737373] uppercase mb-1">DURATION</span>
                        <span className="block text-sm text-white">{durationStr}</span>
                      </div>
                      <div>
                        <span className="block text-[10px] tracking-wider text-[#737373] uppercase mb-1">TOTAL SIZE</span>
                        <span className="block text-sm text-white">{totalSizeStr}</span>
                      </div>
                      <div>
                        <span className="block text-[10px] tracking-wider text-[#737373] uppercase mb-1">DISCS</span>
                        <span className="block text-sm text-white">{count}</span>
                      </div>
                      <div>
                        <span className="block text-[10px] tracking-wider text-[#737373] uppercase mb-1">FORMAT</span>
                        <span className="block text-sm text-white">{formatStr}</span>
                      </div>
                    </div>
                  </div>

                  {/* Discs list */}
                  {discRows.length > 0 && (
                    <div className="space-y-2">
                      <span className="block text-[10px] font-mono tracking-wider text-[#737373] uppercase">DISCS</span>
                      <div className="space-y-1.5 max-h-36 overflow-y-auto pr-0.5">
                        {discRows.map((d, i) => (
                          <div
                            key={d.key || i}
                            className="flex items-center justify-between px-3 py-2 rounded-lg bg-[#0f0f0f] border border-[#262626] font-mono text-xs"
                          >
                            <span className="text-white font-medium truncate pr-2">
                              {d.title}
                            </span>
                            <span className="text-[#8c8c8c] shrink-0">
                              {d.spec} · {d.size}
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Action buttons */}
                  <div className="grid grid-cols-2 gap-3 pt-1">
                    <button
                      type="button"
                      onClick={() => {
                        setDetailsFor(null);
                        handlePlayDisc(item, group.id, undefined, featureLabel(group.id), group.year);
                      }}
                      className="w-full py-2.5 rounded-lg bg-[#e5e5e5] hover:bg-white text-black text-xs font-semibold transition text-center"
                    >
                      Play
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        setDetailsFor(null);
                        handleOpenFolder(folderOf(item));
                      }}
                      className="w-full py-2.5 rounded-lg bg-[#1a1a1a] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium transition text-center"
                    >
                      {isMac ? 'Show in Finder' : 'Show in Explorer'}
                    </button>
                  </div>
                </>
              );
            })()}
          </div>
        </div>
      )}
      {/* Media player preferences (owner): preferred audio/subtitle
          languages as ISO codes (e.g. "en,ja"), applied automatically when
          playback starts. */}
      {showPlayerPrefs && (
        <div
          className="fixed inset-0 z-[200] bg-black/70 backdrop-blur-xs flex items-center justify-center p-4"
          onClick={() => setShowPlayerPrefs(false)}
        >
          <div
            className="w-full max-w-md rounded-xl bg-[#161616] border border-[#333333] shadow-2xl p-5 space-y-4"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-start justify-between gap-3">
              <div>
                <h3 className="text-[15px] font-semibold text-white">Media player preferences</h3>
                <p className="text-[11px] text-[#8c8c8c] mt-0.5">
                  Preferred languages, tried in order every time playback starts. Use ISO 639
                  codes, comma-separated.
                </p>
              </div>
              <button
                onClick={() => setShowPlayerPrefs(false)}
                className="p-1 rounded hover:bg-[#262626] text-[#a3a3a3] hover:text-white transition"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
            {(() => {
              const examples: [string, string][] = [
                ['en', 'English'],
                ['ja', 'Japanese'],
                ['fr', 'French'],
                ['de', 'German'],
                ['es', 'Spanish'],
                ['ko', 'Korean'],
              ];
              return (
                <p className="text-[10px] font-mono text-[#737373]">
                  {examples.map(([c, n]) => `${c} = ${n}`).join(' · ')}
                </p>
              );
            })()}
            <div className="space-y-3">
              <label className="block space-y-1">
                <span className="text-[10px] font-semibold uppercase tracking-widest text-[#8c8c8c]">
                  Audio languages
                </span>
                <input
                  value={prefAudioLangs}
                  onChange={(e) => setPrefAudioLangs(e.target.value)}
                  placeholder="e.g. en,ja"
                  className="w-full px-3 py-2 rounded-lg bg-[#1f1f1f] border border-[#262626] text-xs text-white outline-none focus:border-[#e50914]/50"
                />
              </label>
              <label className="block space-y-1">
                <span className="text-[10px] font-semibold uppercase tracking-widest text-[#8c8c8c]">
                  Subtitle languages
                </span>
                <input
                  value={prefSubsLangs}
                  onChange={(e) => setPrefSubsLangs(e.target.value)}
                  placeholder="e.g. en (leave empty for none)"
                  className="w-full px-3 py-2 rounded-lg bg-[#1f1f1f] border border-[#262626] text-xs text-white outline-none focus:border-[#e50914]/50"
                />
              </label>
            </div>
            <button
              onClick={savePlayerPrefs}
              className="w-full py-2 rounded-lg bg-[#e50914] hover:bg-[#c40812] text-white text-xs font-semibold transition"
            >
              Save preferences
            </button>
          </div>
        </div>
      )}
    </div>
  );
};

export default SimpleMediaView;
