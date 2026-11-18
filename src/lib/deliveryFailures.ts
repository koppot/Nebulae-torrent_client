export interface DeliveryFailure {
  key: string;
  token?: string;
  title?: string;
  reason: string;
}

/** Which settings, if any, can fix a failure; drives the dialog's actions. */
export type DeliveryFailureKind = 'destination' | 'remote' | 'server' | 'other';

export function classifyDeliveryFailure(reason: string): DeliveryFailureKind {
  if (/download destination|destination .*unavailable/i.test(reason)) return 'destination';
  if (/remote server|nebulae server/i.test(reason)) return 'remote';
  if (/server error|HTTP 5\d\d|HTTP status 5\d\d|network error|timed out|rate limited/i.test(reason)) {
    return 'server';
  }
  return 'other';
}

const TOKEN_RE = /[0-9a-f]{64}/i;

export function extractDeliveryTokenFromUri(uri: string): string | undefined {
  return uri.match(TOKEN_RE)?.[0].toLowerCase();
}

/** Stable identity of a failure: the request token when known, else the reason text. */
export function deliveryFailureIdentity(token: string | null | undefined, reason: string): string {
  return token ? token.toLowerCase() : `reason:${reason}`;
}

/** Dismissed alerts are remembered per request and reason, so a different problem still surfaces. */
export function dismissalKey(failure: Pick<DeliveryFailure, 'key' | 'reason'>): string {
  return `${failure.key}|${failure.reason}`;
}

export function addDeliveryFailure(
  current: DeliveryFailure[],
  dismissed: ReadonlySet<string>,
  incoming: { token?: string | null; title?: string | null; reason: string },
): DeliveryFailure[] {
  const key = deliveryFailureIdentity(incoming.token, incoming.reason);
  const next: DeliveryFailure = {
    key,
    token: incoming.token ? incoming.token.toLowerCase() : undefined,
    title: incoming.title?.trim() || undefined,
    reason: incoming.reason,
  };
  if (dismissed.has(dismissalKey(next))) return current;
  const existing = current.find((f) => f.key === key);
  if (existing && existing.reason === next.reason && existing.title === next.title) return current;
  return [...current.filter((f) => f.key !== key), next].slice(-10);
}

/** Forget dismissals for a request when the user deliberately retries it. */
export function clearDismissalsForToken(dismissed: Set<string>, token: string): void {
  const prefix = `${token.toLowerCase()}|`;
  for (const entry of [...dismissed]) {
    if (entry.startsWith(prefix)) dismissed.delete(entry);
  }
}
