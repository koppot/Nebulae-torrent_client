export type CastChannel = 'url' | 'airplay' | 'renderer' | null;

export interface CastStatus {
  active: boolean;
  renderer: boolean;
}

export type CastReconcile = 'clear' | 'adopt' | 'keep';

/**
 * Decide what the overlay does with its renderer-cast panel state given what
 * the backend reports. 'clear': the backend session is gone, drop "Stop
 * casting". 'adopt': a renderer cast is alive but the panel forgot it (picker
 * was closed and reopened). 'keep': nothing to change.
 */
export function reconcileCast(channel: CastChannel, status: CastStatus): CastReconcile {
  if (channel === 'renderer' && !status.active) return 'clear';
  if (channel === null && status.active && status.renderer) return 'adopt';
  return 'keep';
}
