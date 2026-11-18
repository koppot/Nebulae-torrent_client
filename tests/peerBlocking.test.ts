import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { isBlockableIp, peerBlockAction, peerConnectionLabel } from '../src/lib/peerBlocking.ts';

test('only literal IP addresses are blockable', () => {
  assert.equal(isBlockableIp('203.0.113.17'), true);
  assert.equal(isBlockableIp('2001:db8::1'), true);
  assert.equal(isBlockableIp('example.org'), false);
  assert.equal(isBlockableIp('999.1.1.1'), false);
  assert.equal(isBlockableIp(''), false);
  assert.equal(isBlockableIp('0.0.0.0'), false);
});

test('disconnected historical peer can be blocked and unblocked', () => {
  assert.deepEqual(peerBlockAction('203.0.113.17', false, 'RepeatedDisconnect').kind, 'block');
  assert.equal(peerBlockAction('203.0.113.17', false, 'RepeatedDisconnect').disabled, false);
  const unblock = peerBlockAction('203.0.113.17', true, 'RepeatedDisconnect');
  assert.equal(unblock.kind, 'unblock');
  assert.equal(unblock.label, 'Unblock peer');
});

test('unblock stays available when the engine cannot enforce blocks', () => {
  assert.equal(peerBlockAction('203.0.113.17', true, 'Unsupported').disabled, false);
  assert.equal(peerBlockAction('203.0.113.17', false, 'Unsupported').disabled, true);
  assert.equal(peerBlockAction('203.0.113.17', false, null).disabled, true);
});

test('invalid addresses offer no action', () => {
  assert.equal(peerBlockAction('not-an-ip', false, 'RepeatedDisconnect').kind, 'none');
});

test('connection label reflects blocked state for connected and historical peers', () => {
  assert.equal(peerConnectionLabel(true, false), 'Connected now');
  assert.equal(peerConnectionLabel(true, true), 'Blocked · disconnecting');
  assert.equal(peerConnectionLabel(false, true), 'Blocked');
  assert.equal(peerConnectionLabel(false, false), 'Disconnected');
});

test('Peer History rows render the action for every row, not only live peers', () => {
  const panel = readFileSync(new URL('../src/components/manager/PeerHistoryPanel.tsx', import.meta.url), 'utf8');
  assert.match(panel, /peerBlockAction\(row\.ip, isBlocked, capability\)/);
  assert.doesNotMatch(panel, /row\.live && !isBlocked/);
  assert.match(panel, /connected: Boolean\(row\.live\)/);
});
