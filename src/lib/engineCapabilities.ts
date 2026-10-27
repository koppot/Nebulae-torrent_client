/**
 * Engine capability gates for the UI.
 *
 * A control must not offer an operation the active engine cannot perform, and
 * a stored preference must not be shown as proof that the engine is doing
 * something. Each capability has its own gate, so an engine that really
 * supports it is enabled by changing one entry here without touching the
 * controls.
 */

/** Engine modes reported by the backend `get_engine_mode` command. */
export type EngineMode = 'local' | 'rtorrent_server';

/** Shown wherever a superseeding control or saved choice cannot take effect. */
export const SUPERSEEDING_UNSUPPORTED_MESSAGE =
  'Superseeding is not supported by the current engine.';

/**
 * Whether the engine performs a superseeding operation.
 *
 * - `local` (Transmission adapter): the inspected adapter does not perform an
 *   engine operation for superseeding, and the referenced Transmission RPC
 *   documentation exposes no corresponding control.
 * - `rtorrent_server` (remote adapters): they refuse the operation.
 *
 * Real superseeding support is tracked separately (ASI-13). Set an entry to
 * true only when that engine actually performs the operation. Unknown modes
 * count as unsupported.
 */
const SUPERSEEDING_SUPPORT: Record<EngineMode, boolean> = {
  local: false,
  rtorrent_server: false,
};

export function supportsSuperseeding(engineMode: string): boolean {
  return SUPERSEEDING_SUPPORT[engineMode as EngineMode] ?? false;
}
