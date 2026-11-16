/**
 * Focused tests for menuMoveGuard – in-flight guard with latest-position
 * coalescing for disc-menu mouse-move IPC dispatches.
 *
 * Run with:
 *   node --test tests/menuMouseMoveGuard.test.ts
 *
 * Node >= 22.6 supports --experimental-strip-types (or >= 23 without the flag).
 * The file is dependency-free and does not import any React or Tauri module.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createMenuMoveGuard } from '../src/lib/menuMoveGuard.ts';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

type Deferred = {
  promise: Promise<void>;
  resolve: () => void;
  reject: (err: Error) => void;
};

function deferred(): Deferred {
  let resolve: () => void = () => {};
  let reject: (err: Error) => void = () => {};
  const promise = new Promise<void>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** Drain microtasks so .finally() callbacks execute synchronously. */
function flushMicrotasks(): Promise<void> {
  return new Promise<void>((res) => setImmediate(res));
}

// ---------------------------------------------------------------------------
// 1. Normal path: dispatch → guard clears after resolution
// ---------------------------------------------------------------------------
test('single dispatch: guard is in-flight then clears on resolution', async () => {
  const d = deferred();
  let dispatched = 0;
  const guard = createMenuMoveGuard((_pt) => {
    dispatched++;
    return d.promise;
  });

  assert.equal(guard.isInFlight(), false, 'idle initially');
  const sent = guard.dispatch({ x: 10, y: 20 });
  assert.equal(sent, true, 'first dispatch accepted');
  assert.equal(dispatched, 1, 'invoke called once');
  assert.equal(guard.isInFlight(), true, 'guard in-flight after dispatch');

  d.resolve();
  await flushMicrotasks();

  assert.equal(guard.isInFlight(), false, 'guard cleared after resolution');
});

// ---------------------------------------------------------------------------
// 2. Rejected IPC: guard still clears via .finally()
// ---------------------------------------------------------------------------
test('rejected IPC: guard clears so next event can dispatch', async () => {
  const d = deferred();
  let callCount = 0;
  const guard = createMenuMoveGuard((_pt) => {
    callCount++;
    return d.promise;
  });

  guard.dispatch({ x: 5, y: 5 });
  assert.equal(guard.isInFlight(), true);

  d.reject(new Error('IPC error'));
  await flushMicrotasks();

  assert.equal(guard.isInFlight(), false, 'guard cleared after rejection');
  assert.equal(callCount, 1, 'invoke called once');
});

// ---------------------------------------------------------------------------
// 3. Coalescing: latest dropped point is delivered when in-flight call completes
// ---------------------------------------------------------------------------
test('coalescing: latest dropped point is dispatched on completion', async () => {
  const deferreds: Deferred[] = [];
  const receivedPoints: { x: number; y: number }[] = [];
  // Each invoke() gets its own deferred so we can control timing independently.
  const guard = createMenuMoveGuard((pt) => {
    receivedPoints.push(pt);
    const d = deferred();
    deferreds.push(d);
    return d.promise;
  });

  // First dispatch goes through
  guard.dispatch({ x: 1, y: 1 });
  assert.equal(guard.isInFlight(), true);
  assert.equal(deferreds.length, 1, 'one invoke call outstanding');

  // These are dropped; each replaces the previous pending point
  const dropped1 = guard.dispatch({ x: 2, y: 2 });
  assert.equal(dropped1, false, 'second dispatch dropped');
  assert.deepEqual(guard.pendingPoint(), { x: 2, y: 2 }, 'pending is B');

  const dropped2 = guard.dispatch({ x: 3, y: 3 });
  assert.equal(dropped2, false, 'third dispatch dropped');
  assert.deepEqual(guard.pendingPoint(), { x: 3, y: 3 }, 'pending replaced by C (latest)');

  // Pointer stops; first call completes
  deferreds[0].resolve();
  await flushMicrotasks();

  // Guard should have dispatched the coalesced point {3,3}
  assert.equal(receivedPoints.length, 2, 'invoke called for initial + coalesced point');
  assert.deepEqual(receivedPoints[1], { x: 3, y: 3 }, 'latest pending point delivered');
  assert.equal(guard.pendingPoint(), null, 'no further pending after coalesced dispatch');
  // The coalesced call is now in-flight (deferreds[1] not yet resolved)
  assert.equal(guard.isInFlight(), true, 'in-flight for coalesced call');

  // Resolve coalesced call; guard should go idle
  deferreds[1].resolve();
  await flushMicrotasks();
  assert.equal(guard.isInFlight(), false, 'guard idle after coalesced call completes');
});

// ---------------------------------------------------------------------------
// 4. Rapid menu exit/re-entry: stale .finally() is a no-op; pending discarded
// ---------------------------------------------------------------------------
test('rapid menu exit/re-entry: stale completion does not block new session', async () => {
  const d = deferred(); // represents the in-flight call before exit
  const guard = createMenuMoveGuard((_pt) => d.promise);

  guard.dispatch({ x: 1, y: 1 });
  guard.dispatch({ x: 2, y: 2 }); // coalesced pending
  assert.equal(guard.isInFlight(), true);
  assert.deepEqual(guard.pendingPoint(), { x: 2, y: 2 });

  // Exit the menu while the previous call is still pending.
  guard.onMenuExit();
  assert.equal(guard.isInFlight(), false, 'guard idle after onMenuExit');
  assert.equal(guard.pendingPoint(), null, 'pending discarded on exit');

  // New session dispatch goes through immediately.
  const d2 = deferred();
  let session2Calls = 0;
  const guard2 = createMenuMoveGuard((_pt) => {
    session2Calls++;
    return d2.promise;
  });
  guard2.dispatch({ x: 10, y: 10 });
  assert.equal(guard2.isInFlight(), true, 'new session in-flight');

  // The old call now resolves — its .finally() must be a no-op for guard.
  d.resolve();
  await flushMicrotasks();

  // guard is still idle (the old stale completion should not change guard2)
  assert.equal(guard.isInFlight(), false, 'old guard still idle after stale completion');
  // guard2 should still be in-flight
  assert.equal(guard2.isInFlight(), true, 'new session still in-flight (independent guard)');

  d2.resolve();
  await flushMicrotasks();
  assert.equal(guard2.isInFlight(), false, 'new session clears on completion');
});

// ---------------------------------------------------------------------------
// 5. Multiple exit/re-entry cycles: counter keeps incrementing, never stuck
// ---------------------------------------------------------------------------
test('multiple exit/re-entry cycles: guard never gets stuck', async () => {
  const deferreds: Deferred[] = [];
  const guard = createMenuMoveGuard((_pt) => {
    const d = deferred();
    deferreds.push(d);
    return d.promise;
  });

  for (let cycle = 0; cycle < 5; cycle++) {
    guard.dispatch({ x: cycle, y: cycle });
    assert.equal(guard.isInFlight(), true, `cycle ${cycle}: in-flight`);
    guard.onMenuExit();
    assert.equal(guard.isInFlight(), false, `cycle ${cycle}: idle after exit`);
    assert.equal(guard.pendingPoint(), null, `cycle ${cycle}: pending cleared`);
  }

  // Resolve all pending deferred promises in arbitrary order.
  for (const d of deferreds) {
    d.resolve();
  }
  await flushMicrotasks();

  // Guard must be idle; a fresh dispatch must go through.
  let finalCalled = false;
  const finalD = deferred();
  guard.dispatch({ x: 99, y: 99 }); // reuse same guard instance
  // We can't easily track the call in the same guard after cycles without
  // replacing it; verify via isInFlight that it went through.
  assert.equal(guard.isInFlight(), true, 'fresh dispatch after cycles is in-flight');
});

// ---------------------------------------------------------------------------
// 6. Pointer stationary: guard clears on completion with no new events
// ---------------------------------------------------------------------------
test('pointer stationary: guard clears on completion with no new events', async () => {
  const d = deferred();
  const guard = createMenuMoveGuard((_pt) => d.promise);

  guard.dispatch({ x: 50, y: 50 });
  assert.equal(guard.isInFlight(), true);

  // No new events arrive.
  d.resolve();
  await flushMicrotasks();

  assert.equal(guard.isInFlight(), false, 'guard clears without additional events');
  assert.equal(guard.pendingPoint(), null, 'no pending point');
});

// ---------------------------------------------------------------------------
// 7. Click path is independent of the move guard
// ---------------------------------------------------------------------------
test('click path is not blocked by the move guard', async () => {
  // The click handler in PlayerOverlay.tsx calls player_menu_mouse_move
  // directly (bypassing this guard), immediately before player_menu_navigate.
  // This test verifies the guard state machine does not interfere.
  const moveD = deferred();
  const guard = createMenuMoveGuard((_pt) => moveD.promise);

  guard.dispatch({ x: 10, y: 10 });
  assert.equal(guard.isInFlight(), true, 'move guard in-flight');

  // Simulate click path (direct invoke, bypasses guard).
  let clickFired = false;
  const clickFn = async () => { clickFired = true; };
  await clickFn();

  assert.equal(clickFired, true, 'click fires regardless of move guard');
  assert.equal(guard.isInFlight(), true, 'move guard still in-flight (click did not clear it)');

  moveD.resolve();
  await flushMicrotasks();
  assert.equal(guard.isInFlight(), false, 'move guard clears after move completes');
});

// ---------------------------------------------------------------------------
// 8. Dispatch passes point to invoke unchanged
// ---------------------------------------------------------------------------
test('dispatch passes the point to invoke unchanged', async () => {
  const d = deferred();
  let receivedPoint: { x: number; y: number } | null = null;
  const guard = createMenuMoveGuard((pt) => {
    receivedPoint = pt;
    return d.promise;
  });

  guard.dispatch({ x: 123, y: 456 });
  assert.deepEqual(receivedPoint, { x: 123, y: 456 });

  d.resolve();
  await flushMicrotasks();
});

// ---------------------------------------------------------------------------
// 9. Coalesced dispatch: old session's pending position not delivered after exit
// ---------------------------------------------------------------------------
test('menu exit discards pending point before coalesced dispatch fires', async () => {
  const d1 = deferred();
  const calledWith: { x: number; y: number }[] = [];
  const guard = createMenuMoveGuard((pt) => {
    calledWith.push(pt);
    return d1.promise;
  });

  guard.dispatch({ x: 1, y: 1 });
  guard.dispatch({ x: 2, y: 2 }); // coalesced pending

  // Exit before first call resolves
  guard.onMenuExit();
  assert.equal(guard.pendingPoint(), null, 'pending discarded on exit');

  // Resolve the old call: .finally() fires but pending is null, so no new call
  d1.resolve();
  await flushMicrotasks();

  assert.equal(calledWith.length, 1, 'old session pending position not dispatched after exit');
  assert.equal(guard.isInFlight(), false, 'guard idle after stale resolve with no pending');
});

// ---------------------------------------------------------------------------
// 10. Coalescing chain: coalesced call itself may coalesce further
// ---------------------------------------------------------------------------
test('coalescing chain: further drops during coalesced call are themselves coalesced', async () => {
  const deferreds: Deferred[] = [];
  const receivedPoints: { x: number; y: number }[] = [];

  const guard = createMenuMoveGuard((pt) => {
    receivedPoints.push(pt);
    const d = deferred();
    deferreds.push(d);
    return d.promise;
  });

  // Initial dispatch
  guard.dispatch({ x: 1, y: 1 });
  // Two more arrive while A is in-flight
  guard.dispatch({ x: 2, y: 2 });
  guard.dispatch({ x: 3, y: 3 }); // latest

  // A completes → coalesced point {3,3} dispatches (B)
  deferreds[0].resolve();
  await flushMicrotasks();

  assert.equal(receivedPoints.length, 2);
  assert.deepEqual(receivedPoints[1], { x: 3, y: 3 });

  // While B is in-flight, two more arrive
  guard.dispatch({ x: 4, y: 4 });
  guard.dispatch({ x: 5, y: 5 }); // latest

  // B completes → coalesced point {5,5} dispatches (C)
  deferreds[1].resolve();
  await flushMicrotasks();

  assert.equal(receivedPoints.length, 3);
  assert.deepEqual(receivedPoints[2], { x: 5, y: 5 });

  // C completes with no pending
  deferreds[2].resolve();
  await flushMicrotasks();
  assert.equal(guard.isInFlight(), false);
  assert.equal(guard.pendingPoint(), null);
});

// ---------------------------------------------------------------------------
// 11. Click barrier: discards pending coalesced move
// ---------------------------------------------------------------------------
test('click barrier: discards pending coalesced move', async () => {
  const d = deferred();
  const guard = createMenuMoveGuard((_pt) => d.promise);

  guard.dispatch({ x: 1, y: 1 });
  guard.dispatch({ x: 2, y: 2 });
  assert.deepEqual(guard.pendingPoint(), { x: 2, y: 2 }, 'pending point before click');

  let clickExecuted = false;
  await guard.runClick(async () => {
    clickExecuted = true;
    assert.equal(guard.pendingPoint(), null, 'pending discarded on click barrier entry');
  });

  assert.equal(clickExecuted, true);
  assert.equal(guard.pendingPoint(), null, 'pending remains null after click');
  d.resolve();
  await flushMicrotasks();
});

// ---------------------------------------------------------------------------
// 12. Click barrier: ignores post-click completion of an earlier in-flight move
// ---------------------------------------------------------------------------
test('click barrier: ignores post-click completion of an earlier in-flight move', async () => {
  const d = deferred();
  let invokeCount = 0;
  const guard = createMenuMoveGuard((_pt) => {
    invokeCount++;
    return d.promise;
  });

  guard.dispatch({ x: 1, y: 1 });
  assert.equal(invokeCount, 1);

  await guard.runClick(async () => {
    // click executed while move was in flight
  });

  // Now resolve the earlier in-flight move. Its completion must be ignored.
  d.resolve();
  await flushMicrotasks();

  assert.equal(invokeCount, 1, 'earlier move completion does not trigger new dispatch');
  assert.equal(guard.isInFlight(), false, 'guard is idle');
  assert.equal(guard.pendingPoint(), null);
});

// ---------------------------------------------------------------------------
// 13. Click barrier: preserves subsequent move dispatch after click completes
// ---------------------------------------------------------------------------
test('click barrier: preserves subsequent move dispatch after click completes', async () => {
  const d1 = deferred();
  const dispatchedPoints: { x: number; y: number }[] = [];
  const guard = createMenuMoveGuard((pt) => {
    dispatchedPoints.push(pt);
    return d1.promise;
  });

  guard.dispatch({ x: 1, y: 1 });
  assert.equal(dispatchedPoints.length, 1);

  await guard.runClick(async () => {
    // click barrier
  });

  // Dispatch a new move after click has completed
  const d2 = deferred();
  const accepted = guard.dispatch({ x: 10, y: 20 });
  assert.equal(accepted, true, 'move after click accepted immediately');
  assert.equal(dispatchedPoints.length, 2);
  assert.deepEqual(dispatchedPoints[1], { x: 10, y: 20 });

  d1.resolve();
  d2.resolve();
  await flushMicrotasks();
  assert.equal(guard.isInFlight(), false);
});

