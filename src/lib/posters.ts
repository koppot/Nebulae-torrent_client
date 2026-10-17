import { invoke } from '@tauri-apps/api/core';
import { convertFileSrc } from '@tauri-apps/api/core';

/**
 * Poster resolution chain for ADC torrents (approved by the owner):
 *   1. ADC poster — the artwork the tracker already displays on torrents.php
 *      (matched by torrent name; fetched once per session, cached in
 *      localStorage per info-hash so it survives restarts).
 *   2. Local pipeline — `get_media_thumbnail` (cover scan → downscale →
 *      cache → ffmpeg frame → TMDB when a key exists).
 *   3. null — caller renders the neutral placeholder tile.
 *
 * Shared by the Media view tiles and the torrent-list hover popup so both
 * always show the same artwork.
 */

const CACHE_KEY = 'nebulae_adc_poster_map_v2';
const CACHE_MAX_AGE_MS = 24 * 60 * 60 * 1000;

interface AdcPosterCache {
  fetched_at: number;
  // torrent name (as shown on torrents.php) -> poster URL
  by_name: Record<string, string>;
}

let adcMap: AdcPosterCache | null = null;
let adcFetch: Promise<AdcPosterCache | null> | null = null;

function loadCache(): AdcPosterCache {
  if (adcMap) return adcMap;
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as AdcPosterCache;
      if (Date.now() - parsed.fetched_at < CACHE_MAX_AGE_MS) {
        adcMap = parsed;
        return adcMap;
      }
    }
  } catch {
    /* corrupted cache — refetch */
  }
  adcMap = { fetched_at: 0, by_name: {} };
  return adcMap;
}

function normalizeName(name: string): string {
  return name
    .toLowerCase()
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, ' ')
    .trim();
}

// Release-tag noise that appears on torrent names but never on the ADC
// titles page (scene/group/region/medium tags).
const TOKEN_STOPLIST = new Set([
  'the', 'a', 'an', 'of', 'and', 'ova', 'oad', 'special', 'edition',
  'limited', 'complete', 'remastered', 'uncut', 'uncensored', 'dub', 'dubbed',
  'sub', 'subbed', 'dvd', 'bd', 'bluray', 'blu', 'ray', 'ntsc', 'pal',
  'funimation', 'sentai', 'discotek', 'boxset', 'box', 'set', 'collection',
  'series', 'season', 'part', 'volume', 'vol', 'disc', 'd1', 'd2', 'd3',
  'r1', 'r2', 'r2j', 'r3', 'jp', 'jpn', 'us', 'usa', 'uk', 'h264', 'h265',
  'x264', 'x265', 'hevc', 'aac', 'ac3', 'dts', 'dd2', 'dd51', '576p',
  '720p', '1080p', '2160p', 'dvdrip', 'webrip', 'webdl', 'mkv', 'iso',
  'dual', 'audio', 'eng', 'jpn', 'japanese', 'english', 'subs',
]);

function significantTokens(name: string): string[] {
  return normalizeName(name)
    .split(' ')
    .filter((t) => t.length >= 2 && !TOKEN_STOPLIST.has(t) && !/^\d+$/.test(t));
}

/**
 * Match a torrent release name to the ADC torrents.php title.
 * Exact normalized equality first, then a token-subset rule: the release
 * name carries extra tags the tracker title lacks ("Fuuka (2017)
 * Funimation" vs ADC "Fuuka (2017)"), so the ADC title's significant
 * tokens must all appear in the torrent name (or vice versa). Among
 * candidates the one with the best coverage wins; ambiguity (e.g. shared
 * franchise words) is broken by the smallest token difference.
 */
function matchAdcTitle(
  torrentName: string,
  byName: Record<string, string>,
): { title: string; url: string } | null {
  const exact = byName[normalizeName(torrentName)];
  if (exact) return { title: torrentName, url: exact };

  const torrentTokens = new Set(significantTokens(torrentName));
  if (torrentTokens.size === 0) return null;

  let best: { title: string; url: string; coverage: number; slack: number } | null = null;
  for (const [title, url] of Object.entries(byName)) {
    const adcTokens = significantTokens(title);
    if (adcTokens.length === 0) continue;
    const adcSet = new Set(adcTokens);
    const adcMissing = adcTokens.filter((t) => !torrentTokens.has(t)).length;
    const torrentMissing = [...torrentTokens].filter((t) => !adcSet.has(t)).length;
    // One-sided containment: the smaller token set must be fully covered
    // by the larger (a single stray token on the ADC side breaks it).
    const covered =
      adcMissing === 0 || (torrentMissing === 0 && adcTokens.length <= torrentTokens.size);
    if (!covered) continue;
    const smaller = Math.min(adcTokens.length, torrentTokens.size);
    if (smaller < 1) continue;
    const coverage = smaller / Math.max(adcTokens.length, torrentTokens.size);
    const slack = adcMissing + torrentMissing;
    if (!best || coverage > best.coverage || (coverage === best.coverage && slack < best.slack)) {
      best = { title, url, coverage, slack };
    }
  }
  return best ? { title: best.title, url: best.url } : null;
}

export function posterForTorrent(infoHash: string): string | null {
  const cache = loadCache();
  const stored = (cache.by_name as Record<string, string | undefined>)[infoHash];
  return stored ?? null; // raw path or remote URL; caller converts for display
}

async function fetchAdcMap(): Promise<AdcPosterCache | null> {
  if (adcFetch) return adcFetch;
  adcFetch = invoke<Record<string, string>>('fetch_adc_poster_map')
    .then((byName) => {
      // Normalize keys so whitespace/case differences never block a match.
      const normalized: Record<string, string> = {};
      for (const [k, v] of Object.entries(byName)) {
        normalized[normalizeName(k)] = v;
      }
      const cache: AdcPosterCache = { fetched_at: Date.now(), by_name: normalized };
      adcMap = cache;
      try {
        localStorage.setItem(CACHE_KEY, JSON.stringify(cache));
      } catch {
        /* storage full — session-only cache */
      }
      return cache;
    })
    .catch(() => null);
  return adcFetch;
}

/**
 * Resolve the best poster for a torrent. Resolves null when nothing is
 * available; callers render the placeholder.
 */
export async function resolvePoster(
  infoHash: string,
  torrentName: string,
  contentPath: string,
): Promise<string | null> {
  // 1. Per-torrent cache from a previous resolution of the same torrent.
  const cached = posterForTorrent(infoHash);
  if (cached) return cached;

  // 2. ADC poster: match the torrent's name against the tracker's list.
  const cache = loadCache();
  const stale = Date.now() - cache.fetched_at >= CACHE_MAX_AGE_MS;
  const map = stale || Object.keys(cache.by_name).length === 0 ? await fetchAdcMap() : cache;
  if (map) {
    const match = matchAdcTitle(torrentName, map.by_name);
    if (match) {
      const { title, url } = match;
      const absolute = url.startsWith('http')
        ? url
        : `https://asiandvdclub.org${url}`;
      map.by_name[infoHash] = absolute; // per-torrent memo inside the same store
      try {
        localStorage.setItem(CACHE_KEY, JSON.stringify(map));
      } catch {
        /* ignore */
      }
      invoke('log_frontend', {
        message: `poster: ADC match for "${torrentName}" -> "${title}"`,
      }).catch(() => {});
      // /poster/{id} is hotlink-protected: the webview's <img> would be
      // rejected, so pull the bytes through the authenticated backend and
      // serve the cached local file (also works offline).
      try {
        const local = await invoke<string>('download_adc_poster', { url: absolute });
        if (local) {
          map.by_name[infoHash] = local;
          try {
            localStorage.setItem(CACHE_KEY, JSON.stringify(map));
          } catch {
            /* ignore */
          }
          // RAW local path — the renderer converts via convertFileSrc exactly
          // once. Returning an asset:// URL here caused double conversion
          // (asset://localhost/%2Fasset%3A%2F%2F...) and a 403 from the
          // asset protocol scope (the reported blank posters).
          return local;
        }
      } catch {
        /* fall back to the raw URL; the img may still load */
      }
      return absolute;
    }
  }

  // 3. TMDB/Trakt artwork (DB-cached engine-side): preferred over local
  // scan images — a "screenshot" frame must never outrank real key art
  // (owner: Fuuka picked a screenshot although TMDB/Trakt had the poster).
  try {
    const parsed = await invoke<{ clean_title: string; year: number | null }>('parse_media_title', {
      rawTitle: torrentName,
    });
    const res = await invoke<{ poster_webp?: string | null } | null>('fetch_tmdb_release', {
      title: parsed.clean_title,
      year: parsed.year ?? undefined,
    });
    if (res?.poster_webp) return res.poster_webp;
  } catch {
    /* fall through to the local pipeline */
  }

  // 4. Local pipeline (cover scan → ffmpeg frame → TMDB when configured).
  if (map) {
    // Log the closest ADC titles so a real matching gap is diagnosable
    // from the session log alone.
    const torrentTokens = new Set(significantTokens(torrentName));
    const near = Object.keys(map.by_name)
      .map((title) => {
        const adcTokens = significantTokens(title);
        const overlap = adcTokens.filter((t) => torrentTokens.has(t)).length;
        return { title, score: overlap / Math.max(1, Math.max(adcTokens.length, torrentTokens.size)) };
      })
      .sort((a, b) => b.score - a.score)
      .slice(0, 3)
      .map((c) => `${c.title} (${Math.round(c.score * 100)}%)`);
    invoke('log_frontend', {
      message: `poster: no ADC match for "${torrentName}" — falling back to local pipeline (nearest: ${near.join('; ') || 'none'})`,
    }).catch(() => {});
  }
  try {
    const res = await invoke<{ thumbnail_path?: string | null }>('get_media_thumbnail', {
      mediaPath: contentPath,
    });
    if (res.thumbnail_path) return res.thumbnail_path;
  } catch {
    /* fall through to placeholder */
  }

  return null;
}

/** Convert a resolver result for display: remote URLs pass through, local
 * filesystem paths go through the asset protocol exactly once. */
export function toDisplaySrc(p: string): string {
  return p.startsWith('http') ? p : convertFileSrc(p);
}

export function clearPosterCaches(): void {
  adcMap = null;
  adcFetch = null;
  try {
    localStorage.removeItem(CACHE_KEY);
  } catch {
    /* ignore */
  }
}

export { normalizeName };
