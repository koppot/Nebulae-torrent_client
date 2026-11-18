export type PeerBlockCapability = 'Unsupported' | 'RepeatedDisconnect';

const IPV4 = /^(25[0-5]|2[0-4]\d|1?\d?\d)(\.(25[0-5]|2[0-4]\d|1?\d?\d)){3}$/;
const IPV6 = /^[0-9a-f:.]+$/i;

/** A peer row can be blocked only when its address is a literal IPv4/IPv6 address. */
export function isBlockableIp(ip: string | null | undefined): boolean {
  const value = (ip ?? '').trim();
  if (!value) return false;
  if (IPV4.test(value)) return value !== '0.0.0.0';
  return value.includes(':') && value.length <= 45 && IPV6.test(value) && value !== '::';
}

export interface PeerBlockAction {
  kind: 'block' | 'unblock' | 'none';
  label: string;
  disabled: boolean;
  title?: string;
}

/**
 * The action every Peer History row offers, connected or not: Unblock is always available
 * for a blocked address (even once the peer has left), Block needs an address and an engine
 * that can enforce it.
 */
export function peerBlockAction(
  ip: string,
  isBlocked: boolean,
  capability: PeerBlockCapability | null,
): PeerBlockAction {
  if (!isBlockableIp(ip)) return { kind: 'none', label: '', disabled: true };
  if (isBlocked) {
    return { kind: 'unblock', label: 'Unblock peer', disabled: false, title: 'Remove this IP from the global block list' };
  }
  if (capability === 'RepeatedDisconnect') {
    return { kind: 'block', label: 'Block peer', disabled: false, title: 'Block this IP across all torrents' };
  }
  return {
    kind: 'block',
    label: 'Block peer',
    disabled: true,
    title:
      capability === null
        ? 'Checking whether the active engine can enforce blocks…'
        : 'The active engine cannot enforce peer blocks',
  };
}

export function peerConnectionLabel(connected: boolean, isBlocked: boolean): string {
  if (connected) return isBlocked ? 'Blocked · disconnecting' : 'Connected now';
  return isBlocked ? 'Blocked' : 'Disconnected';
}
