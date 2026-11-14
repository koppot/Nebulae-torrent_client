/**
 * Source-level regression guard for the remote-server startup render loop.
 * This test prevents reintroducing the unstable Zustand object selector; it
 * does not launch React or prove packaged-window rendering.
 *
 * Run with:
 *   node --experimental-strip-types --test tests/remoteServerStartupSource.test.ts
 */
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const appSource = readFileSync(new URL('../src/App.tsx', import.meta.url), 'utf8');
const remoteStoreSource = readFileSync(
  new URL('../src/stores/remoteServerStore.ts', import.meta.url),
  'utf8',
);

test('App uses scalar remote-settings selectors and does not subscribe to action objects', () => {
  assert.match(
    appSource,
    /useSettingsStore\(\(s\) => s\.settings\.rtorrent_server_enabled\)/,
  );
  assert.match(
    appSource,
    /useSettingsStore\(\(s\) => s\.settings\.rtorrent_server_url\)/,
  );
  assert.match(
    appSource,
    /useSettingsStore\(\(s\) => s\.settings\.rtorrent_server_token\)/,
  );
  assert.doesNotMatch(appSource, /useSettingsStore\(\(s\) => \(\{/);
  assert.match(appSource, /useRemoteServerStore\.getState\(\)\.configure\(\)/);
  assert.match(appSource, /useRemoteServerStore\.getState\(\)\.stopPolling\(\)/);
});

test('stopPolling returns before set when the store is already clean', () => {
  const start = remoteStoreSource.indexOf('stopPolling() {');
  const end = remoteStoreSource.indexOf('\n    },', start);
  assert.notEqual(start, -1, 'stopPolling action exists');
  assert.notEqual(end, -1, 'stopPolling action has a closing boundary');

  const body = remoteStoreSource.slice(start, end);
  const cleanStateGuard = body.match(/if\s*\([\s\S]*?s\.selectedId === null[\s\S]*?\)\s*\{\s*return;\s*\}/);
  assert.ok(cleanStateGuard, 'clean disconnected state returns early');
  assert.ok(
    body.indexOf('set({') > cleanStateGuard.index + cleanStateGuard[0].length,
    'store update occurs only after the clean-state guard',
  );
});
