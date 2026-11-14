import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const read = (path: string) =>
  readFileSync(new URL(path, import.meta.url), 'utf8');

test('peer history commands and background sampler are registered', () => {
  const lib = read('../src-tauri/src/lib.rs');
  const commands = read('../src-tauri/src/commands/peering.rs');
  const telemetry = read('../src-tauri/src/engine/telemetry.rs');
  const database = read('../src-tauri/src/db/mod.rs');
  const schema = read('../src-tauri/src/db/schema.rs');

  assert.match(lib, /commands::peering::get_peer_history/);
  assert.match(lib, /commands::peering::clear_peer_history/);
  assert.match(lib, /engine::telemetry::spawn_peer_history_loop/);
  assert.match(commands, /pub fn get_peer_history\(/);
  assert.match(commands, /pub fn clear_peer_history\(/);
  assert.match(telemetry, /Duration::from_secs\(30\)/);
  assert.match(telemetry, /MissedTickBehavior::Skip/);
  assert.match(telemetry, /tokio::task::spawn_blocking/);
  assert.match(telemetry, /db\.record_peer_history\(/);
  assert.match(database, /MAX_PEER_HISTORY_ROWS: i64 = 100_000/);
  assert.match(database, /PEER_HISTORY_RETENTION_MS: i64 = 90 \* 24 \* 60 \* 60 \* 1000/);
  assert.match(schema, /CREATE TABLE IF NOT EXISTS peer_history/);
});

test('peer history UI reads and clears only the selected torrent history', () => {
  const panel = read('../src/components/manager/PeerHistoryPanel.tsx');

  assert.match(panel, /invoke<PeerHistoryEntry\[\]>\('get_peer_history', \{ infoHash: torrentHash \}\)/);
  assert.match(panel, /invoke<number>\('clear_peer_history', \{ infoHash: torrentHash \}\)/);
  assert.match(panel, /clear_peer_history/);
  assert.match(panel, /New observations resume while Nebulae is running/);
  assert.match(panel, /never stores ADC members\/profiles, client names, or geo\/ASN metadata/);
});
