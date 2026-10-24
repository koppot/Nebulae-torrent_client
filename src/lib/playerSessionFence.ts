/**
 * Player session-generation fence and idempotent close coordinator.
 *
 * Why this exists (player-close regression):
 *  - The backend emits `player-opened` at 0/1.5/4/8s after every open (to
 *    survive webview-load races), and the shell polls `player_engine_playback`
 *    every second while the player is closed. A repeat or poll response that
 *    was produced before a user close could arrive AFTER it and reopen the
 *    player UI over a session that no longer exists.
 *  - Closing used to restore the library UI only after the backend teardown
 *    resolved; a stuck teardown (e.g. the native overlay hide waiting on a
 *    wedged main thread) trapped the UI in the dead player.
 *
 * The engine bumps its session generation at the START of every stop, and
 * every open stops first - so generations are strictly monotonic across
 * sessions, and every engine callback (open result, `player-opened` repeat,
 * readiness-poll response) advertises the generation it belongs to. Anything
 * at or below the generation the user closed is stale and must never reopen
 * the player UI.
 *
 * This module is dependency-free on purpose: it is unit-tested with Node's
 * built-in test runner (tests/playerSessionFence.test.ts), so it must stay
 * free of Tauri/React imports and of non-erasable TypeScript syntax.
 */

export interface CloseCoordinatorDeps {
  /** Invoke the backend close; resolves with the post-close generation. */
  invokeClose: () => Promise<number | null | undefined>;
  /** Synchronously restore the library UI (close the player store). */
  closeStore: () => void;
}

export class CloseCoordinator {
  private lastSeenGen = 0;
  private closedGen = 0;
  private openEpoch = 0;
  private closeInFlight: Promise<void> | null = null;
  private readonly deps: CloseCoordinatorDeps;

  constructor(deps: CloseCoordinatorDeps) {
    this.deps = deps;
  }

  /** Record the generation advertised by any engine callback/poll/result. */
  noteEngineGen(gen: number | null | undefined): void {
    if (typeof gen === 'number' && Number.isFinite(gen) && gen > this.lastSeenGen) {
      this.lastSeenGen = gen;
    }
  }

  /**
   * True when an engine callback belongs to a session the user already
   * closed. Payloads without a generation cannot be proven stale, so they
   * keep the legacy adopt behavior.
   */
  isStaleEngineGen(gen: number | null | undefined): boolean {
    return typeof gen === 'number' && Number.isFinite(gen) && gen <= this.closedGen;
  }

  /**
   * Idempotent close shared by Q, the player X button and the error modal:
   * the store closes FIRST (the UI is never held hostage by a slow or stuck
   * backend teardown), and no matter how many callers ask while a close is
   * in flight, the backend teardown runs exactly once for all of them.
   */
  close(): Promise<void> {
    if (this.closeInFlight) return this.closeInFlight;
    // Invalidate any in-flight open so its late resolve cannot reopen the UI.
    this.openEpoch++;
    this.deps.closeStore();
    // Optimistic fence: the backend close bumps the generation by exactly
    // one at the start of teardown, so the dying session AND the close's own
    // bump - everything the engine can still report until the teardown
    // lands - are both at or below lastSeenGen + 1. A session opened later
    // always gets a strictly higher generation. The fence is tightened to
    // the authoritative generation once the close resolves.
    this.closedGen = Math.max(this.closedGen, this.lastSeenGen + 1);
    this.closeInFlight = this.deps
      .invokeClose()
      .then((gen) => {
        if (typeof gen === 'number' && Number.isFinite(gen)) {
          this.closedGen = Math.max(this.closedGen, gen);
        }
      })
      .catch(() => {
        // The store is already closed; keep the optimistic fence so a
        // session that survived a failed close cannot silently reopen.
      })
      .finally(() => {
        this.closeInFlight = null;
      });
    return this.closeInFlight;
  }

  /**
   * Await any in-flight close (a late player_close would otherwise stop the
   * NEW session - backend commands run concurrently), then return this
   * open's epoch token for isOpenCurrent().
   */
  async beginOpen(): Promise<number> {
    const pending = this.closeInFlight;
    if (pending) {
      try {
        await pending;
      } catch {
        // close() never rejects; defensive only.
      }
    }
    return this.openEpoch;
  }

  /**
   * True when no close happened since beginOpen(). False means the freshly
   * opened session is orphaned: the caller must close it instead of opening
   * the UI the user already dismissed.
   */
  isOpenCurrent(epoch: number): boolean {
    return epoch === this.openEpoch;
  }
}
