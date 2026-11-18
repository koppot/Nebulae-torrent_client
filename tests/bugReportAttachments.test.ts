import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  MAX_SCREENSHOTS,
  MAX_SCREENSHOT_BYTES,
  checkScreenshots,
} from '../src/lib/bugReportAttachments.ts';

const img = (name: string, size = 1000, type = 'image/png') => ({ name, size, type });

test('accepts supported images within the limits', () => {
  const { accepted, errors } = checkScreenshots(0, [img('a.png'), img('b.JPG', 5, 'image/jpeg'), img('c.webp', 5, 'image/webp')]);
  assert.equal(accepted.length, 3);
  assert.deepEqual(errors, []);
});

test('rejects unsupported types with a clear message', () => {
  const { accepted, errors } = checkScreenshots(0, [img('notes.txt', 10, 'text/plain'), img('clip.mp4', 10, 'video/mp4'), img('x.png', 10, 'application/pdf')]);
  assert.equal(accepted.length, 0);
  assert.equal(errors.length, 3);
  assert.match(errors[0], /not a supported image/);
});

test('rejects oversized and empty files', () => {
  const { accepted, errors } = checkScreenshots(0, [img('big.png', MAX_SCREENSHOT_BYTES + 1), img('zero.png', 0)]);
  assert.equal(accepted.length, 0);
  assert.match(errors[0], /limit is 10\.0 MB/);
  assert.match(errors[1], /empty/);
});

test('enforces the total count across selections', () => {
  const first = checkScreenshots(0, Array.from({ length: MAX_SCREENSHOTS + 2 }, (_, i) => img(`s${i}.png`)));
  assert.equal(first.accepted.length, MAX_SCREENSHOTS);
  assert.equal(first.errors.length, 2);
  const more = checkScreenshots(MAX_SCREENSHOTS, [img('late.png')]);
  assert.equal(more.accepted.length, 0);
  assert.match(more.errors[0], /at most 5/);
});

test('one bad file does not block the good ones', () => {
  const { accepted, errors } = checkScreenshots(0, [img('ok.png'), img('bad.exe', 5, ''), img('ok2.gif', 5, 'image/gif')]);
  assert.deepEqual(accepted.map((f) => f.name), ['ok.png', 'ok2.gif']);
  assert.equal(errors.length, 1);
});

test('modal never uploads: no network call, no auto capture, plain hand-off wording', () => {
  const src = readFileSync(new URL('../src/components/modals/BugReportModal.tsx', import.meta.url), 'utf8');
  assert.doesNotMatch(src, /fetch\(|XMLHttpRequest|getDisplayMedia|screencapture/);
  assert.match(src, /not attached automatically/i);
  assert.match(src, /not covered by the automatic text redaction/i);
});

test('title bar exposes Report a Bug and opens the shared dialog', () => {
  const src = readFileSync(new URL('../src/components/layout/TitleBar.tsx', import.meta.url), 'utf8');
  assert.match(src, /aria-label="Report a Bug"/);
  assert.match(src, /setShowBugReport\(true\)/);
});
