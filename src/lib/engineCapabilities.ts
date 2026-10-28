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

/** Shown wherever a first/last-piece priority control cannot take effect. */
export const FIRST_LAST_PIECE_UNSUPPORTED_MESSAGE =
  'First/last piece priority is not supported by the current engine.';

const FIRST_LAST_PIECE_SUPPORT: Record<EngineMode, boolean> = {
  local: false,
  rtorrent_server: false,
};

export function supportsFirstLastPiece(engineMode: string): boolean {
  return FIRST_LAST_PIECE_SUPPORT[engineMode as EngineMode] ?? false;
}
