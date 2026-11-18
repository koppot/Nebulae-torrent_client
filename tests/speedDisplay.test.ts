import test from 'node:test';
import assert from 'node:assert/strict';
import { speedDisplayControl } from '../src/lib/speedDisplay.ts';

test('windows gets the taskbar overlay label', () => {
  assert.equal(speedDisplayControl('windows')?.label, 'Show taskbar speed overlay');
});

test('macos gets a menu bar label, not the Windows wording', () => {
  const c = speedDisplayControl('macos');
  assert.equal(c?.label, 'Show speeds in the menu bar');
  assert.ok(!/windows/i.test(c?.help ?? ''));
});

test('linux shows no control', () => {
  assert.equal(speedDisplayControl('linux'), null);
});

test('unknown platform falls back to the user agent', () => {
  assert.equal(speedDisplayControl('unknown', 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15)')?.label, 'Show speeds in the menu bar');
  assert.equal(speedDisplayControl('unknown', 'Mozilla/5.0 (Windows NT 10.0)')?.label, 'Show taskbar speed overlay');
  assert.equal(speedDisplayControl('unknown', 'X11; Linux x86_64'), null);
});
