import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  buildFileTree,
  createLatestGate,
  mergeFileStats,
  nodeCompletedBytes,
  nodeProgress,
  progressPercent,
} from '../src/lib/torrentFiles.ts';

const GIB = 1024 ** 3;
const file = (index: number, name: string, size: number, progress: number, priority = 1) => ({
  index,
  name,
  size_bytes: size,
  progress,
  priority,
});

const aika = (p1: number, p2: number) => [
  file(0, 'AIKa (1997-1999)/AIKA_DISC_1.ISO', 6.17 * GIB, p1),
  file(1, 'AIKa (1997-1999)/AIKA_DISC_1.MDS', 8220, 1),
  file(2, 'AIKa (1997-1999)/AIKA_DISC_2.ISO', 5.27 * GIB, p2),
  file(3, 'AIKa (1997-1999)/AIKA_DISC_2.MDS', 8230, 1),
];

test('folder progress is completed bytes over total bytes, not an average of percentages', () => {
  const tree = buildFileTree(aika(0.28, 0.05));
  const folder = tree.children[0];
  const done = 0.28 * 6.17 * GIB + 8220 + 0.05 * 5.27 * GIB + 8230;
  const total = 6.17 * GIB + 8220 + 5.27 * GIB + 8230;
  assert.ok(Math.abs(nodeCompletedBytes(folder) - done) < 1);
  assert.ok(Math.abs(nodeProgress(folder) - done / total) < 1e-9);
  // an unweighted average of the four file percentages would be ~0.58
  assert.ok(nodeProgress(folder) < 0.2);
});

test('nested folders aggregate over every descendant file', () => {
  const tree = buildFileTree([
    file(0, 'a/b/x.bin', 100, 1),
    file(1, 'a/b/y.bin', 300, 0),
    file(2, 'a/z.bin', 600, 0.5),
  ]);
  const a = tree.children[0];
  assert.equal(a.size_bytes, 1000);
  assert.equal(nodeProgress(a), (100 + 0 + 300) / 1000);
  assert.equal(nodeProgress(a.children[0]), 100 / 400);
});

test('a fresh per-file snapshot moves the folder aggregate with it', () => {
  const before = buildFileTree(aika(0.28, 0.05)).children[0];
  const after = buildFileTree(aika(0.8, 0.8)).children[0];
  assert.ok(nodeProgress(after) > nodeProgress(before));
  assert.ok(Math.abs(nodeProgress(after) - 0.8 * (6.17 + 5.27) / (6.17 + 5.27)) < 1e-3);
});

test('mergeFileStats keeps identity of unchanged files and the array when nothing changed', () => {
  const prev = aika(0.28, 0.05);
  const same = aika(0.28, 0.05);
  assert.equal(mergeFileStats(prev, same), prev);
  const next = aika(0.5, 0.05);
  const merged = mergeFileStats(prev, next);
  assert.notEqual(merged, prev);
  assert.equal(merged[1], prev[1]);
  assert.equal(merged[2], prev[2]);
  assert.equal(merged[0], next[0]);
  assert.equal(merged[0].progress, 0.5);
});

test('mergeFileStats picks up priority changes and a different file list', () => {
  const prev = aika(0.1, 0.1);
  const changed = aika(0.1, 0.1);
  changed[2] = { ...changed[2], priority: 3 };
  assert.equal(mergeFileStats(prev, changed)[2].priority, 3);
  const shorter = prev.slice(0, 2);
  assert.equal(mergeFileStats(prev, shorter), shorter);
  assert.equal(mergeFileStats(undefined, prev), prev);
});

test('percent labels never round an unfinished item up to 100', () => {
  assert.equal(progressPercent(0.996), 99);
  assert.equal(progressPercent(0.5), 50);
  assert.equal(progressPercent(1), 100);
  assert.equal(progressPercent(0), 0);
});

test('latest gate rejects an older response once a newer one was applied', () => {
  const gate = createLatestGate();
  const first = gate.begin('t1');
  const second = gate.begin('t1');
  assert.equal(gate.accept('t1', second), true);
  assert.equal(gate.accept('t1', first), false);
  const other = gate.begin('t2');
  assert.equal(gate.accept('t2', other), true);
  // an older response is still used when it is the only one that arrives
  const a = gate.begin('t3');
  gate.begin('t3');
  assert.equal(gate.accept('t3', a), true);
});

test('store and view refresh the open Files tab and apply details only to their own torrent', () => {
  const store = readFileSync(new URL('../src/stores/torrentStore.ts', import.meta.url), 'utf8');
  const view = readFileSync(new URL('../src/components/manager/TorrentManagerView.tsx', import.meta.url), 'utf8');
  assert.match(store, /refreshTorrentFiles: async \(id: string\)/);
  assert.match(store, /detailGate\.accept\(id, seq\)/);
  assert.match(store, /t\.id === id/);
  assert.match(view, /activeManagerTab !== 'files'/);
  assert.match(view, /refreshTorrentFiles\(filesTargetId\)/);
  assert.match(view, /setInterval\(tick, filesComplete \? 5000 : 2000\)/);
});
