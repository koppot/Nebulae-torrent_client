import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reconcileCast } from '../src/lib/castStatus.ts';

test('renderer cast whose backend session ended is cleared', () => {
  assert.equal(reconcileCast('renderer', { active: false, renderer: false }), 'clear');
});

test('live renderer cast is kept', () => {
  assert.equal(reconcileCast('renderer', { active: true, renderer: true }), 'keep');
});

test('a live renderer cast the panel forgot is adopted', () => {
  assert.equal(reconcileCast(null, { active: true, renderer: true }), 'adopt');
});

test('url and airplay channels are never cleared by this check', () => {
  assert.equal(reconcileCast('url', { active: false, renderer: false }), 'keep');
  assert.equal(reconcileCast('airplay', { active: false, renderer: false }), 'keep');
});

test('no cast and no channel: nothing to do', () => {
  assert.equal(reconcileCast(null, { active: false, renderer: false }), 'keep');
  assert.equal(reconcileCast(null, { active: true, renderer: false }), 'keep');
});
