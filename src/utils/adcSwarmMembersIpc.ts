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

export function toSwarmMemberMap(matches: SwarmMember[]): Map<string, SwarmMember> {
  const members = new Map<string, SwarmMember>();
  for (const member of matches) members.set(`${member.ip}:${member.port}`, member);
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
      return 'ADC is rate limiting peer lookups. Retry later.';
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
