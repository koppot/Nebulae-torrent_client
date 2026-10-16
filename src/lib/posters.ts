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

const CACHE_KEY = 'nebulae_adc_poster_map_v1';
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
  return name.toLowerCase().replace(/\s+/g, ' ').trim();
}

export function posterForTorrent(infoHash: string): string | null {
  const cache = loadCache();
  const stored = (cache.by_name as Record<string, string | undefined>)[infoHash];
  if (!stored) return null;
  return stored.startsWith('http') ? stored : convertFileSrc(stored);
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
    const direct = map.by_name[normalizeName(torrentName)];
    if (direct) {
      const absolute = direct.startsWith('http')
        ? direct
        : `https://asiandvdclub.org${direct}`;
      map.by_name[infoHash] = absolute; // per-torrent memo inside the same store
      try {
        localStorage.setItem(CACHE_KEY, JSON.stringify(map));
      } catch {
        /* ignore */
      }
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
          return convertFileSrc(local);
        }
      } catch {
        /* fall back to the raw URL; the img may still load */
      }
      return absolute;
    }
  }

  // 3. Local pipeline (cover scan → ffmpeg frame → TMDB when configured).
  try {
    const res = await invoke<{ thumbnail_path?: string | null }>('get_media_thumbnail', {
      mediaPath: contentPath,
    });
    if (res.thumbnail_path) return convertFileSrc(res.thumbnail_path);
  } catch {
    /* fall through to placeholder */
  }

  return null;
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
