/**
 * Focused tests for the player-close regression fix (Q / X shared close).
 *
 * Runs with Node's built-in test runner (Node >= 24 strips the types):
 *   node --test tests/playerSessionFence.test.ts
 *
 * The module under test is dependency-free; the Tauri invoke and the store
 * are injected as fakes here.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { CloseCoordinator } from '../src/lib/playerSessionFence.ts';

function deferred(): { promise: Promise<number>; resolve: (v: number) => void } {
  let resolve: (v: number) => void = () => {};
  const promise = new Promise<number>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

test('close restores the UI synchronously, before the backend answers', async () => {
  let storeClosed = false;
  const d = deferred();
  const c = new CloseCoordinator({
    invokeClose: () => d.promise,
    closeStore: () => {
      storeClosed = true;
    },
  });
  const p = c.close();
  // The whole point of the fix: the library UI is restored immediately,
  // even while the backend teardown (VLC stop + native overlay hide) is
  // still running - it used to wait for the invoke and could trap forever.
  assert.equal(storeClosed, true);
  d.resolve(2);
  await p;
});

test('close resolves even when the backend close never settles', () => {
  let storeClosed = false;
  const c = new CloseCoordinator({
    invokeClose: () => new Promise<number>(() => {}), // wedged backend
    closeStore: () => {
      storeClosed = true;
    },
  });
  c.close();
  assert.equal(storeClosed, true);
});

test('close resolves (does not throw) when the backend rejects', async () => {
  let storeClosed = false;
  const c = new CloseCoordinator({
    invokeClose: () => Promise.reject(new Error('boom')),
    closeStore: () => {
      storeClosed = true;
    },
  });
  await c.close();
  assert.equal(storeClosed, true);
});

test('concurrent closes (Q + X + error modal) share one backend teardown', async () => {
  let calls = 0;
  const c = new CloseCoordinator({
    invokeClose: async () => {
      calls++;
      return 3;
    },
    closeStore: () => {},
  });
  await Promise.all([c.close(), c.close(), c.close()]);
  assert.equal(calls, 1);
  // Once settled, a later close is a fresh teardown (idempotent, not sticky).
  await c.close();
  assert.equal(calls, 2);
});

test('generations at/below the closed one are stale; later ones are not', async () => {
  const c = new CloseCoordinator({
    invokeClose: async () => 6,
    closeStore: () => {},
  });
  c.noteEngineGen(5);
  assert.equal(c.isStaleEngineGen(5), false); // live session, not closed yet
  await c.close();
  assert.equal(c.isStaleEngineGen(5), true); // the session that was closed
  assert.equal(c.isStaleEngineGen(6), true); // the close's own gen bump
  assert.equal(c.isStaleEngineGen(7), false); // a session opened later
  assert.equal(c.isStaleEngineGen(undefined), false); // legacy payload: keep old behavior
  assert.equal(c.isStaleEngineGen(null), false);
});

test('the fence holds while the backend close is still in flight', async () => {
  const d = deferred();
  const c = new CloseCoordinator({
    invokeClose: () => d.promise,
    closeStore: () => {},
  });
  c.noteEngineGen(5);
  const p = c.close();
  // Teardown has not resolved: an in-flight poll (gen 5) or the close's own
  // bump (gen 6) must already be fenced, or they reopen the dead player.
  assert.equal(c.isStaleEngineGen(5), true);
  assert.equal(c.isStaleEngineGen(6), true);
  d.resolve(6);
  await p;
  assert.equal(c.isStaleEngineGen(6), true);
  assert.equal(c.isStaleEngineGen(7), false);
});

test('close during an in-flight open orphans the open', async () => {
  const c = new CloseCoordinator({
    invokeClose: async () => 2,
    closeStore: () => {},
  });
  const epoch = await c.beginOpen();
  await c.close();
  assert.equal(c.isOpenCurrent(epoch), false);
  const epoch2 = await c.beginOpen();
  assert.equal(c.isOpenCurrent(epoch2), true);
});

test('beginOpen waits for an in-flight close so the close cannot kill the new session', async () => {
  const order: string[] = [];
  const d = deferred();
  const c = new CloseCoordinator({
    invokeClose: () =>
      d.promise.then((v) => {
        order.push('close-done');
        return v;
      }),
    closeStore: () => {
      order.push('store-closed');
    },
  });
  const closeP = c.close();
  const openP = c.beginOpen().then(() => {
    order.push('open-begin');
  });
  await new Promise((r) => setTimeout(r, 20));
  assert.deepEqual(order, ['store-closed']); // open must NOT have begun
  d.resolve(2);
  await Promise.all([closeP, openP]);
  assert.deepEqual(order, ['store-closed', 'close-done', 'open-begin']);
});
