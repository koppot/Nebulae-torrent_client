import { invoke } from '@tauri-apps/api/core';
import type { PeerAddr, SwarmMember, SwarmMembersResult } from '../types';

export const ADC_SWARM_MEMBERS_COMMAND = 'adc_swarm_members';

type InvokeCommand = (command: string, args: Record<string, unknown>) => Promise<unknown>;

export type SwarmMembersLookup =
  | { kind: 'result'; result: SwarmMembersResult }
  | { kind: 'ipc_error' };

/**
 * Tauri's default command naming maps frontend `infoHash` to Rust `info_hash`.
 * The separate HTTP endpoint body uses its own `info_hash` field.
 */
export async function lookupSwarmMembers(
  infoHash: string,
  peers: PeerAddr[],
  invokeCommand: InvokeCommand = invoke,
): Promise<SwarmMembersLookup> {
  try {
    const result = await invokeCommand(ADC_SWARM_MEMBERS_COMMAND, { infoHash, peers });
    return { kind: 'result', result: result as SwarmMembersResult };
  } catch {
    // Do not log arbitrary IPC error objects. They may contain request details.
    return { kind: 'ipc_error' };
  }
}

export function getSwarmMembersCacheKey(
  infoHash: string,
  credentialRevision: number,
  peers: PeerAddr[],
): string {
  const peerKey = peers.map((peer) => `${peer.ip}:${peer.port}`).sort().join(',');
  return `${infoHash}:${credentialRevision}:${peerKey}`;
}

export function getSwarmMembersFetchKey(cacheKey: string, retry: number): string {
  return `${cacheKey}:${retry}`;
}

export function shouldCacheSwarmMembers(status: string): boolean {
  return ['matched', 'no_matching_peers', 'endpoint_unsupported'].includes(status);
}

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
    if (parsed.protocol === 'https:' && parsed.hostname.endsWith('asiandvdclub.org')) {
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
      parsed.hostname.endsWith('asiandvdclub.org') &&
      /^\/profile\/\d+/.test(parsed.pathname)
    );
  } catch {
    return false;
  }
}

export function toSwarmMemberMap(matches: SwarmMember[]): Map<string, SwarmMember> {
  const members = new Map<string, SwarmMember>();
  for (const member of matches) {
    const normalized: SwarmMember = {
      ...member,
      profile_url: normalizeProfileUrl(member.profile_url, member.user_id),
    };
    members.set(`${normalized.ip}:${normalized.port}`, normalized);
  }
  return members;
}

export function getAdcSwarmNotice(status: string): string | null {
  switch (status) {
    case 'credential_required':
    case 'not_linked':
      return 'ADC credentials are required to identify swarm members.';
    case 'credential_access_denied':
      return 'Keychain access needs authorization. Unlock or relink your ADC account.';
    case 'credential_storage_unavailable':
    case 'credential_storage_error':
      return 'The secure credential store is unavailable.';
    case 'authentication_failed':
      return 'ADC rejected the stored credential. Relink your account.';
    case 'no_matching_peers':
      return 'No connected peers matched an ADC member.';
    case 'endpoint_unsupported':
      return 'The ADC peer-information endpoint is unavailable.';
    case 'rate_limited':
      return 'ADC is rate limiting peer lookups. Refreshing automatically once cooldown clears.';
    case 'cooling_down':
      return 'Waiting for ADC cooldown period before refreshing new peers.';
    case 'ipc_error':
      return 'Internal error communicating with the peer-information service.';
    case 'network_error':
      return 'Could not reach the ADC peer-information service.';
    case 'parse_error':
    case 'http_error':
      return 'ADC returned an unexpected peer-information response.';
    case 'invalid_info_hash':
      return 'The selected torrent has an invalid info hash.';
    default:
      return null;
  }
}
