import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const read = (path: string) =>
  readFileSync(new URL(path, import.meta.url), 'utf8');

test('native and HTML drops share one multi-path dispatcher', () => {
  const app = read('../src/App.tsx');
  const modal = read('../src/components/modals/AddTorrentModal.tsx');
  const store = read('../src/stores/uiStore.ts');

  // safeListen wraps listen with an unmount guard; both forms are the same subscription.
  assert.match(app, /(?:safeListen|listen)<\{ paths\?: string\[\]; position\?: \{ x: number; y: number \} \}>\('tauri:\/\/drag-drop'/);
  assert.match(app, /(?:safeListen|listen)<string\[\]>\('nebulae:\/\/paths-dropped'/);
  assert.match(app, /classify_dropped_media_path/);
  assert.match(app, /Promise\.all\(files\.map\(async \(file\)/);
  assert.match(app, /readTorrentFileAsDataUrl\(file\)/);
  assert.match(app, /openAddTorrentWithSources\(torrentSources\)/);
  assert.doesNotMatch(modal, /tauri:\/\/drag-drop|nebulae:\/\/paths-dropped/);
  assert.match(modal, /new CustomEvent<string\[\]>\('nebulae:open-paths'/);
  assert.match(modal, /Promise\.all\(files\.map\(async \(file\)/);
  assert.match(modal, /readTorrentFileAsDataUrl\(file\)/);
  assert.match(modal, /isEmbeddedMetainfo/);
  assert.doesNotMatch(modal, /files\.find\(/);
  assert.match(store, /queuedAddTorrentSources:\s*\[\]/);
});

test('dropped media defers automatic library insertion for the optional prompt', () => {
  const app = read('../src/App.tsx');
  const player = read('../src/lib/playerStore.ts');

  assert.match(app, /openInPlayer\(path,\s*\{\s*deferLibraryImport:\s*true\s*\}\)/);
  assert.match(player, /!opts\?\.deferLibraryImport/);
  assert.match(app, /invoke\('add_external_media'/);
});

test('Windows association registration offers file handlers without setting defaults', () => {
  const associations = read('../src-tauri/src/commands/association.rs');

  assert.match(associations, /OpenWithProgids/);
  assert.match(associations, /Capabilities\\FileAssociations/);
  assert.doesNotMatch(associations, /Put \('\.' \+ \$extension\) '' \$id/);
});
