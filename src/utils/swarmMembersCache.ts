/**
 * Client-side cache and coordination for adc_swarm_members results.
 *
 * Requirements:
 * 1. Continuous display: Keep resolved ADC member details visible immediately.
 * 2. Scoped: Cache entries are scoped to (credentialRevision, infoHash, ip:port).
 * 3. Rate-limit resilience: Never clear displayed member data when rate limited.
 * 4. Automatic recovery: Cooldown-aware background retry.
 * 5. Normalization: Normalizes profile URLs to /profile/<id>.
 */

import type { PeerAddr, SwarmMember, SwarmMembersResult } from '../types';

const MEMBER_TTL_MS = 30 * 60 * 1000; // 30 minutes
const UNMATCHED_TTL_MS = 5 * 60 * 1000; // 5 minutes
const LOOKUP_FAILURE_RETRY_MS = 30 * 1000;
const DEFAULT_COOLDOWN_MS = 60 * 1000; // 60s account-wide rate limit
const MAX_ENTRIES = 500;

/**
 * Normalizes an ADC profile URL to canonical /profile/<id>.
 * If the input URL uses /user/<id>, converts it to /profile/<id>.
 * Validates HTTPS and Asian DVD Club hostname.
 */
export function normalizeProfileUrl(rawUrl?: string | null, userId?: number): string {
  const fallback = userId ? `https://asiandvdclub.org/profile/${userId}` : 'https://asiandvdclub.org';
  if (!rawUrl || typeof rawUrl !== 'string') return fallback;
  const trimmed = rawUrl.trim();
  if (!trimmed) return fallback;

  try {
    const parsed = new URL(trimmed);
    const hostname = parsed.hostname.toLowerCase();
    if (
      parsed.protocol === 'https:' &&
      (hostname === 'asiandvdclub.org' || hostname.endsWith('.asiandvdclub.org'))
    ) {
      const match = parsed.pathname.match(/^\/(?:user|profile)\/(\d+)/);
      if (match) {
        return `https://${parsed.host}/profile/${match[1]}`;
      }
      if (userId) {
        return `https://${parsed.host}/profile/${userId}`;
      }
      return `https://${parsed.host}${parsed.pathname}`;
    }
  } catch {
    // Malformed URL, use fallback
  }

  return fallback;
}

/**
 * Validates that an ADC profile URL is HTTPS and belongs to asiandvdclub.org.
 */
export function isValidAdcProfileUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    return (
      parsed.protocol === 'https:' &&
      (parsed.hostname.toLowerCase() === 'asiandvdclub.org' ||
        parsed.hostname.toLowerCase().endsWith('.asiandvdclub.org')) &&
      /^\/profile\/\d+/.test(parsed.pathname)
    );
  } catch {
    return false;
  }
}

interface PeerCacheRecord {
  kind: 'matched' | 'unmatched' | 'failed';
  member?: SwarmMember;
  timestamp: number;
  retryAfterMs?: number;
  failedAt?: number;
}

// Scoped key: `${credentialRevision}:${infoHash}:${ip}:${port}`
const peerCache = new Map<string, PeerCacheRecord>();

// Account-wide cooldown timestamp
let cooldownUntil = 0;
let lastKnownRevision = 0;
let activeRetryTimer: ReturnType<typeof setTimeout> | null = null;
let transientRetryTimer: ReturnType<typeof setTimeout> | null = null;
let transientRetryAt = 0;
const retryListeners = new Set<() => void>();

export function getCooldownRemaining(): number {
  const remaining = cooldownUntil - Date.now();
  return remaining > 0 ? remaining : 0;
}

export function setAccountCooldown(durationMs: number = DEFAULT_COOLDOWN_MS): void {
  cooldownUntil = Math.max(cooldownUntil, Date.now() + durationMs);
  notifyCooldownListeners(getCooldownRemaining());
}

export function clearAccountCooldown(): void {
  cooldownUntil = 0;
  if (activeRetryTimer) {
    clearTimeout(activeRetryTimer);
    activeRetryTimer = null;
  }
}

function notifyCooldownListeners(delayMs: number): void {
  if (activeRetryTimer) {
    clearTimeout(activeRetryTimer);
  }
  if (delayMs <= 0) {
    for (const listener of retryListeners) {
      try {
        listener();
      } catch {
        /* ignore listener errors */
      }
    }
    return;
  }
  activeRetryTimer = setTimeout(() => {
    activeRetryTimer = null;
    for (const listener of retryListeners) {
      try {
        listener();
      } catch {
        /* ignore listener errors */
      }
    }
  }, delayMs);
}

function notifyRetryListeners(): void {
  for (const listener of retryListeners) {
    try {
      listener();
    } catch {
      /* ignore listener errors */
    }
  }
}

function scheduleTransientRetry(delayMs: number): void {
  const retryAt = Date.now() + delayMs;
  if (transientRetryTimer && transientRetryAt <= retryAt) return;
  if (transientRetryTimer) clearTimeout(transientRetryTimer);
  transientRetryAt = retryAt;
  transientRetryTimer = setTimeout(() => {
    transientRetryTimer = null;
    transientRetryAt = 0;
    notifyRetryListeners();
  }, delayMs);
}

export function registerCooldownListener(listener: () => void): () => void {
  retryListeners.add(listener);
  return () => {
    retryListeners.delete(listener);
  };
}

/**
 * Collapses IPv4-mapped IPv6 (::ffff:x.x.x.x) to plain IPv4 and lowercases
 * the result. This ensures frontend cache keys match regardless of whether the
 * ADC API or the engine returns the IPv4-mapped form.
 */
function normalizeIp(ip: string): string {
  const v4mapped = ip.match(/^::ffff:(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})$/i);
  if (v4mapped) return v4mapped[1];
  return ip.toLowerCase();
}

function getPeerKey(credentialRevision: number, infoHash: string, ip: string, port: number): string {
  return `${credentialRevision}:${infoHash.toLowerCase()}:${normalizeIp(ip)}:${port}`;
}

/**
 * Returns any already-resolved members for the given peers and identifies
 * which peers are unresolved and need to be looked up.
 */
export function getResolvedSwarm(
  infoHash: string,
  credentialRevision: number,
  peers: PeerAddr[],
): {
  cachedMap: Map<string, SwarmMember>;
  unresolvedPeers: PeerAddr[];
  hasRecentFailure: boolean;
  hasRecentNoMatch: boolean;
} {
  invalidateOnRevisionChange(credentialRevision);

  const cachedMap = new Map<string, SwarmMember>();
  const unresolvedPeers: PeerAddr[] = [];
  let hasRecentFailure = false;
  let hasRecentNoMatch = false;
  const now = Date.now();

  for (const peer of peers) {
    const key = getPeerKey(credentialRevision, infoHash, peer.ip, peer.port);
    const record = peerCache.get(key);

    if (record) {
      if (record.kind === 'matched' && record.member) {
        // Stale-while-revalidate: return cached member even if TTL is old,
        // but if TTL exceeded, we can also let it be re-queried if needed.
        const normalized: SwarmMember = {
          ...record.member,
          profile_url: normalizeProfileUrl(record.member.profile_url, record.member.user_id),
        };
        cachedMap.set(`${peer.ip}:${peer.port}`, normalized);
        if (
          now - record.timestamp > MEMBER_TTL_MS &&
          !(record.failedAt && now - record.failedAt < (record.retryAfterMs ?? LOOKUP_FAILURE_RETRY_MS))
        ) {
          unresolvedPeers.push(peer);
        } else if (record.failedAt && now - record.failedAt < (record.retryAfterMs ?? LOOKUP_FAILURE_RETRY_MS)) {
          hasRecentFailure = true;
        }
      } else if (record.kind === 'unmatched') {
        if (now - record.timestamp > UNMATCHED_TTL_MS) {
          unresolvedPeers.push(peer);
        } else {
          hasRecentNoMatch = true;
        }
      } else if (record.kind === 'failed') {
        const retryAfterMs = record.retryAfterMs ?? LOOKUP_FAILURE_RETRY_MS;
        if (now - record.timestamp >= retryAfterMs) {
          unresolvedPeers.push(peer);
        } else {
          hasRecentFailure = true;
        }
      }
    } else {
      unresolvedPeers.push(peer);
    }
  }

  return { cachedMap, unresolvedPeers, hasRecentFailure, hasRecentNoMatch };
}

/**
 * Stores matches and marks unreturned peers as unmatched (negative cache).
 */
export function storeSwarmResults(
  infoHash: string,
  credentialRevision: number,
  requestedPeers: PeerAddr[],
  result: SwarmMembersResult,
): void {
  invalidateOnRevisionChange(credentialRevision);
  const now = Date.now();

  // If rate limited, update cooldown
  if (result.status === 'rate_limited') {
    const retrySecs = (result as { retry_after_seconds?: number }).retry_after_seconds ?? 60;
    setAccountCooldown(retrySecs * 1000);
  } else if (
    // Set cooldown on normal successful response as well (server rate limits to 60s per account)
    (result.status === 'matched' || result.status === 'no_matching_peers') &&
    !result.from_cache_only
  ) {
    setAccountCooldown(DEFAULT_COOLDOWN_MS);
  }

  const matchedKeys = new Set<string>();

  for (const member of result.matches) {
    const normalized: SwarmMember = {
      ...member,
      profile_url: normalizeProfileUrl(member.profile_url, member.user_id),
    };
    const key = getPeerKey(credentialRevision, infoHash, member.ip, member.port);
    matchedKeys.add(`${normalizeIp(member.ip)}:${member.port}`);

    if (peerCache.size >= MAX_ENTRIES && !peerCache.has(key)) {
      const oldestKey = peerCache.keys().next().value;
      if (oldestKey !== undefined) peerCache.delete(oldestKey);
    }

    peerCache.set(key, {
      kind: 'matched',
      member: normalized,
      timestamp: normalized.matched_at_epoch_ms ?? now,
    });
  }

  // Mark requested peers that did not match as negative-cached
  if (result.status === 'matched' || result.status === 'no_matching_peers') {
    for (const peer of requestedPeers) {
      if (!matchedKeys.has(`${normalizeIp(peer.ip)}:${peer.port}`)) {
        const key = getPeerKey(credentialRevision, infoHash, peer.ip, peer.port);
        if (peerCache.size >= MAX_ENTRIES && !peerCache.has(key)) {
          const oldestKey = peerCache.keys().next().value;
          if (oldestKey !== undefined) peerCache.delete(oldestKey);
        }
        peerCache.set(key, {
          kind: 'unmatched',
          timestamp: now,
        });
      }
    }
  } else if (result.status !== 'rate_limited') {
    const retryAfterMs =
      result.status === 'endpoint_unsupported'
        ? 10 * 60 * 1000
        : ['credential_required', 'not_linked', 'credential_access_denied',
            'credential_storage_unavailable', 'credential_storage_error',
            'authentication_failed'].includes(result.status)
          ? 60 * 1000
          : LOOKUP_FAILURE_RETRY_MS;
    scheduleTransientRetry(retryAfterMs);
    for (const peer of requestedPeers) {
      const key = getPeerKey(credentialRevision, infoHash, peer.ip, peer.port);
      const existing = peerCache.get(key);
      if (existing?.kind === 'matched') {
        peerCache.set(key, { ...existing, failedAt: now, retryAfterMs });
        continue;
      }
      peerCache.set(key, { kind: 'failed', timestamp: now, retryAfterMs });
    }
  }
}

/** Marks IPC rejections as transient failures without replacing a cached match. */
export function markSwarmLookupFailed(
  infoHash: string,
  credentialRevision: number,
  peers: PeerAddr[],
): void {
  invalidateOnRevisionChange(credentialRevision);
  const now = Date.now();
  scheduleTransientRetry(LOOKUP_FAILURE_RETRY_MS);
  for (const peer of peers) {
    const key = getPeerKey(credentialRevision, infoHash, peer.ip, peer.port);
    const existing = peerCache.get(key);
    if (existing?.kind === 'matched') {
      peerCache.set(key, {
        ...existing,
        failedAt: now,
        retryAfterMs: LOOKUP_FAILURE_RETRY_MS,
      });
      continue;
    }
    peerCache.set(key, {
      kind: 'failed',
      timestamp: now,
      retryAfterMs: LOOKUP_FAILURE_RETRY_MS,
    });
  }
}

/** Explicit user retry bypasses only transient failures, not success/no-match TTLs. */
export function clearSwarmRetryState(
  infoHash: string,
  credentialRevision: number,
  peers: PeerAddr[],
): void {
  invalidateOnRevisionChange(credentialRevision);
  for (const peer of peers) {
    const key = getPeerKey(credentialRevision, infoHash, peer.ip, peer.port);
    const existing = peerCache.get(key);
    if (existing?.kind === 'failed') {
      peerCache.delete(key);
    } else if (existing?.kind === 'matched' && existing.failedAt) {
      peerCache.set(key, { ...existing, failedAt: undefined, retryAfterMs: undefined });
    }
  }
}

/**
 * Invalidates cache if credentialRevision has changed to prevent cross-account leakage.
 */
export function invalidateOnRevisionChange(currentRevision: number): void {
  if (lastKnownRevision !== currentRevision) {
    peerCache.clear();
    clearAccountCooldown();
    if (transientRetryTimer) clearTimeout(transientRetryTimer);
    transientRetryTimer = null;
    transientRetryAt = 0;
    lastKnownRevision = currentRevision;
  }
}

export function clearAllSwarmCache(): void {
  peerCache.clear();
  clearAccountCooldown();
  if (transientRetryTimer) clearTimeout(transientRetryTimer);
  transientRetryTimer = null;
  transientRetryAt = 0;
  lastKnownRevision = 0;
}

// ── Legacy API compatibility ──────────────────────────────────────────────────

interface LegacyCacheEntry {
  timestamp: number;
  result: SwarmMembersResult;
}
const legacyCache = new Map<string, LegacyCacheEntry>();

export function getCachedMembers(key: string): SwarmMembersResult | null {
  const entry = legacyCache.get(key);
  if (!entry) return null;
  if (Date.now() - entry.timestamp > 65_000) {
    legacyCache.delete(key);
    return null;
  }
  return entry.result;
}

export function setCachedMembers(key: string, result: SwarmMembersResult): void {
  if (!legacyCache.has(key) && legacyCache.size >= 50) {
    const oldest = legacyCache.keys().next().value;
    if (oldest !== undefined) legacyCache.delete(oldest);
  }
  legacyCache.set(key, { timestamp: Date.now(), result });
}

export function evictCachedMembers(key: string): void {
  legacyCache.delete(key);
}
