/**
 * menuMoveGuard – in-flight guard with latest-position coalescing for
 * disc-menu mouse-move IPC dispatches.
 *
 * # Problem
 *
 * Without a guard, every 40 ms tick dispatches a new player_menu_mouse_move
 * Tauri command. Because Tauri v2 on Windows serialises command dispatch
 * through the WebView2/main thread, a backlog of unresolved invoke() Promises
 * stalls run_on_main_thread inside ensure_overlay and can delay the
 * poster-to-player-window transition. The guard limits the backlog to at most
 * one in-flight call at a time within a menu session.
 *
 * # Concurrency bound
 *
 * The guard guarantees **at most one in-flight call per menu session**. On
 * menu exit/re-entry, the old session's pending call may still be outstanding
 * while the new session issues its first call. This brief two-call overlap is
 * intentional: the old call is harmless (its result is discarded; its
 * .finally() is a no-op via the generation check) and blocking the new session
 * on it would delay the first highlight update after re-entry.
 *
 * # Latest-position coalescing
 *
 * When a move event arrives while a call is in-flight, the event is not
 * queued (that would recreate the backlog problem). Instead, the **latest
 * dropped point** is stored. When the in-flight call completes, if there is
 * a pending point it is dispatched immediately, ensuring the menu is
 * eventually updated to the last pointer position even if the pointer stopped
 * moving before the call resolved.
 *
 * On menu exit, the pending point is discarded so an old session's stale
 * position is not delivered at the start of a new session.
 *
 * # Generation counter
 *
 * A simple boolean flag is unsafe across rapid menu exit/re-entry cycles: a
 * stale .finally() arriving after menu exit would clear the flag for a new,
 * unrelated in-flight call. The generation counter makes each outstanding call
 * uniquely identifiable:
 *   - On dispatch, the counter is incremented and the new value is captured as
 *     the generation for that call.
 *   - The .finally() callback clears the guard only when the current generation
 *     still matches the captured value.
 *   - On menu exit, the guard is reset to 0. Any still-pending .finally() will
 *     see (0 !== capturedGen) and be a no-op.
 *
 * # Click-activation path
 *
 * onVideoClick calls player_menu_mouse_move directly (bypassing this guard)
 * immediately before player_menu_navigate. Click calls must never be blocked
 * by a pending move; they use a separate 350 ms rate limit. This guard does
 * not interfere with that path.
 *
 * # Menu-entry wakeup
 *
 * The first at_menu poll tick dispatches a single warmup move via invoke()
 * directly (bypassing this guard). It is a one-time event that must always
 * fire; it is not part of the continuous mouse-move stream.
 *
 * Usage:
 *   const guard = createMenuMoveGuard(invoke);
 *   // In onVideoMouseMove:
 *   guard.dispatch(point);          // dropped or coalesced if in-flight
 *   // On menu exit:
 *   guard.onMenuExit();
 */

export interface MenuMoveGuard {
  /**
   * Attempt to dispatch a mouse-move IPC call with the given point.
   *
   * - If idle: dispatches immediately and returns true.
   * - If in-flight: stores point as the latest pending position (replacing any
   *   previous pending point) and returns false. When the current call
   *   completes, the pending point is automatically dispatched.
   */
  dispatch(point: { x: number; y: number }): boolean;

  /**
   * Reset the guard on menu exit. Discards any pending coalesced point and
   * marks the guard idle. Any still-pending .finally() callback from the last
   * move call will be a no-op after this.
   */
  onMenuExit(): void;

  /** True when a dispatched call has not yet resolved or rejected. */
  isInFlight(): boolean;

  /**
   * The latest point that was dropped because a call was in-flight.
   * null when nothing is pending. Visible for testing.
   */
  pendingPoint(): { x: number; y: number } | null;
}

/**
 * Create a MenuMoveGuard that wraps the given invoke function.
 *
 * @param invoke – function that sends a mouse-move IPC call and returns a
 *   Promise that resolves/rejects when the backend has processed the message.
 */
export function createMenuMoveGuard(
  invoke: (point: { x: number; y: number }) => Promise<unknown>,
): MenuMoveGuard {
  // 0 = idle; > 0 = in-flight (holds the current dispatch generation)
  let inFlightGen = 0;
  // Monotone counter; always increments so each call has a unique generation.
  let counter = 0;
  // Latest position dropped while a call was in-flight. Dispatched when the
  // call completes. null when nothing is pending.
  let pending: { x: number; y: number } | null = null;

  function doDispatch(point: { x: number; y: number }): void {
    const gen = ++counter;
    inFlightGen = gen;
    invoke(point)
      .finally(() => {
        if (inFlightGen !== gen) return; // stale: a newer call or menu exit already took over
        inFlightGen = 0;
        // Deliver the latest position that arrived while we were in-flight.
        if (pending !== null) {
          const next = pending;
          pending = null;
          doDispatch(next);
        }
      })
      .catch(() => {
        /* errors handled by .finally which already cleared/coalesced */
      });
  }

  return {
    dispatch(point) {
      if (inFlightGen !== 0) {
        // In-flight: coalesce — replace whatever pending point was stored.
        pending = point;
        return false;
      }
      pending = null;
      doDispatch(point);
      return true;
    },

    onMenuExit() {
      // Reset to idle and discard any stale pending position from this session.
      // Any still-pending .finally() will see (0 !== capturedGen) → no-op.
      inFlightGen = 0;
      pending = null;
    },

    isInFlight() {
      return inFlightGen !== 0;
    },

    pendingPoint() {
      return pending;
    },
  };
}
