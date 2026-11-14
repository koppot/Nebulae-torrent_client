import assert from 'node:assert/strict';
import { test } from 'node:test';
import { normalizeOpenPath, openPathKey } from '../src/utils/openPath.ts';

test('normalizes local file URLs to filesystem paths', () => {
  assert.equal(
    normalizeOpenPath('file:///Users/dev/Media/My%20Film.iso'),
    '/Users/dev/Media/My Film.iso',
  );
  assert.equal(
    normalizeOpenPath('file:///C:/Users/dev/Torrents/My%20File.torrent'),
    'C:/Users/dev/Torrents/My File.torrent',
  );
});

test('preserves UNC hosts and non-file URL sources', () => {
  assert.equal(
    normalizeOpenPath('file://nas.local/Media/My%20Film.mkv'),
    '//nas.local/Media/My Film.mkv',
  );
  assert.equal(
    normalizeOpenPath('magnet:?xt=urn:btih:abc'),
    'magnet:?xt=urn:btih:abc',
  );
});

test('deduplicates Windows paths without folding Unix path case', () => {
  assert.equal(openPathKey('C:\\Torrents\\FILE.TORRENT', true), 'c:/torrents/file.torrent');
  assert.equal(openPathKey('/Media/Film.mkv', false), '/Media/Film.mkv');
});
