/**
 * Client-side cache for adc_swarm_members results.
 *
 * TTL: 65 seconds (just over the 60-second server-side rate limit).
 * Capacity: 50 entries; oldest evicted when exceeded.
 * Key: torrent info_hash (40-char hex, lowercase).
 */

import type { SwarmMembersResult } from '../types';

const TTL_MS = 65_000;
const MAX_ENTRIES = 50;

interface CacheEntry {
  timestamp: number;
  result: SwarmMembersResult;
}

// Map preserves insertion order for simple LRU-style eviction.
const cache = new Map<string, CacheEntry>();

export function getCachedMembers(infoHash: string): SwarmMembersResult | null {
  const entry = cache.get(infoHash);
  if (!entry) return null;
  if (Date.now() - entry.timestamp > TTL_MS) {
    cache.delete(infoHash);
    return null;
  }
  return entry.result;
}

export function setCachedMembers(infoHash: string, result: SwarmMembersResult): void {
  // Evict oldest entry when at capacity.
  if (!cache.has(infoHash) && cache.size >= MAX_ENTRIES) {
    const oldest = cache.keys().next().value;
    if (oldest !== undefined) cache.delete(oldest);
  }
  cache.set(infoHash, { timestamp: Date.now(), result });
}

/** Remove a stale entry explicitly (e.g. on torrent removal). */
export function evictCachedMembers(infoHash: string): void {
  cache.delete(infoHash);
}
