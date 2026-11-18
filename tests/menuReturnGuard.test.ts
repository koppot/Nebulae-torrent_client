import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  armMenuReturnGuard,
  evaluateMenuReturnGuard,
  type MenuReturnGuardPoll,
  type MenuReturnGuardState,
} from '../src/lib/menuReturnGuard.ts';

test('armMenuReturnGuard: does nothing if already at menu', () => {
  const poll: MenuReturnGuardPoll = {
    at_menu: true,
    playing: true,
    length_ms: 600_000,
    title: 0,
    time_ms: 15_000,
  };
  const guard = armMenuReturnGuard(poll, 1000);
  assert.equal(guard, null);
});

test('armMenuReturnGuard: does nothing if in short title or near start', () => {
  const shortTitle: MenuReturnGuardPoll = {
    at_menu: false,
    playing: true,
    length_ms: 120_000,
    title: 1,
    time_ms: 15_000,
  };
  assert.equal(armMenuReturnGuard(shortTitle, 1000), null);

  const nearStart: MenuReturnGuardPoll = {
    at_menu: false,
    playing: true,
    length_ms: 600_000,
    title: 1,
    time_ms: 5_000, // < 10_000 ms
  };
  assert.equal(armMenuReturnGuard(nearStart, 1000), null);
});

test('armMenuReturnGuard: successfully arms during feature playback', () => {
  const feature: MenuReturnGuardPoll = {
    at_menu: false,
    playing: true,
    length_ms: 1_200_000,
    title: 2,
    time_ms: 500_000,
  };
  const guard = armMenuReturnGuard(feature, 1000, 5000);
  assert.deepEqual(guard, {
    title: 2,
    timeMs: 500_000,
    deadline: 6000,
  });
});

test('evaluateMenuReturnGuard: disarms when real menu confirms open (>= 2 polls)', () => {
  const guard: MenuReturnGuardState = {
    title: 1,
    timeMs: 120_000,
    deadline: 6000,
  };
  const poll: MenuReturnGuardPoll = {
    at_menu: true,
    playing: true,
    length_ms: 600_000,
    title: 0,
    time_ms: 1000,
  };
  const { nextGuard, result } = evaluateMenuReturnGuard(guard, poll, 2000, 2);
  assert.equal(nextGuard, null);
  assert.equal(result.action, 'disarm_menu_confirmed');
});

test('evaluateMenuReturnGuard: disarms on deadline expiration', () => {
  const guard: MenuReturnGuardState = {
    title: 1,
    timeMs: 120_000,
    deadline: 6000,
  };
  const poll: MenuReturnGuardPoll = {
    at_menu: false,
    playing: true,
    length_ms: 1_200_000,
    title: 1,
    time_ms: 125_000,
  };
  const { nextGuard, result } = evaluateMenuReturnGuard(guard, poll, 7000, 0);
  assert.equal(nextGuard, null);
  assert.equal(result.action, 'disarm_expired');
});

test('evaluateMenuReturnGuard: resets guard when transitioning to a different title', () => {
  const guard: MenuReturnGuardState = {
    title: 1,
    timeMs: 300_000,
    deadline: 6000,
  };
  // User or disc jumps to Title 2 (e.g. bonus feature or next chapter/title)
  const poll: MenuReturnGuardPoll = {
    at_menu: false,
    playing: true,
    length_ms: 1_200_000,
    title: 2,
    time_ms: 1000,
  };
  const { nextGuard, result } = evaluateMenuReturnGuard(guard, poll, 2000, 0);
  assert.equal(nextGuard, null);
  assert.deepEqual(result, {
    action: 'disarm_title_changed',
    previousTitle: 1,
    newTitle: 2,
  });
});

test('evaluateMenuReturnGuard: triggers restart recovery when same title resets to <5s', () => {
  const guard: MenuReturnGuardState = {
    title: 1,
    timeMs: 450_000,
    deadline: 6000,
  };
  // Top Menu command caused Title 1 to restart at 0:00 without opening menu
  const poll: MenuReturnGuardPoll = {
    at_menu: false,
    playing: true,
    length_ms: 1_500_000,
    title: 1,
    time_ms: 800, // restarted to 800ms
  };
  const { nextGuard, result } = evaluateMenuReturnGuard(guard, poll, 2500, 0);
  assert.equal(nextGuard, null);
  assert.deepEqual(result, {
    action: 'recover_restart',
    restoreTimeMs: 450_000,
    mode: 5,
  });
});
