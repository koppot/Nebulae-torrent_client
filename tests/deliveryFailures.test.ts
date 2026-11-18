import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  addDeliveryFailure,
  classifyDeliveryFailure,
  clearDismissalsForToken,
  dismissalKey,
  extractDeliveryTokenFromUri,
  type DeliveryFailure,
} from '../src/lib/deliveryFailures.ts';

const TOKEN = 'ab'.repeat(32);
const REASON = "Configured download destination '~/Downloads' is unavailable";

test('repeated events for one request collapse into a single entry', () => {
  const dismissed = new Set<string>();
  let list: DeliveryFailure[] = [];
  for (let i = 0; i < 50; i += 1) {
    list = addDeliveryFailure(list, dismissed, { token: TOKEN, title: 'Glass Fleet', reason: REASON });
  }
  assert.equal(list.length, 1);
  assert.equal(list[0].token, TOKEN);
});

test('same request with the same reason stays dismissed on later events', () => {
  const dismissed = new Set<string>();
  let list = addDeliveryFailure([], dismissed, { token: TOKEN, reason: REASON });
  dismissed.add(dismissalKey(list[0]));
  list = [];
  list = addDeliveryFailure(list, dismissed, { token: TOKEN.toUpperCase(), reason: REASON });
  assert.equal(list.length, 0);
});

test('a different reason for a dismissed request is still surfaced', () => {
  const dismissed = new Set<string>();
  const first = addDeliveryFailure([], dismissed, { token: TOKEN, reason: REASON });
  dismissed.add(dismissalKey(first[0]));
  const second = addDeliveryFailure([], dismissed, { token: TOKEN, reason: 'Delivery request has expired.' });
  assert.equal(second.length, 1);
});

test('deliberate retry clears dismissal so the next failure is visible', () => {
  const dismissed = new Set<string>();
  const first = addDeliveryFailure([], dismissed, { token: TOKEN, reason: REASON });
  dismissed.add(dismissalKey(first[0]));
  clearDismissalsForToken(dismissed, TOKEN);
  assert.equal(addDeliveryFailure([], dismissed, { token: TOKEN, reason: REASON }).length, 1);
});

test('different requests are tracked independently', () => {
  const other = 'cd'.repeat(32);
  const list = addDeliveryFailure(
    addDeliveryFailure([], new Set(), { token: TOKEN, reason: REASON }),
    new Set(),
    { token: other, reason: REASON },
  );
  assert.equal(list.length, 2);
});

test('extracts the opaque token from delivery URIs', () => {
  assert.equal(extractDeliveryTokenFromUri(`adc://delivery/${TOKEN.toUpperCase()}`), TOKEN);
  assert.equal(extractDeliveryTokenFromUri('adc://torrent/123'), undefined);
});

test('failures are classified so only relevant settings are offered', () => {
  assert.equal(classifyDeliveryFailure(REASON), 'destination');
  assert.equal(
    classifyDeliveryFailure('Remote server is not reachable: connection refused'),
    'remote',
  );
  assert.equal(
    classifyDeliveryFailure(
      'AsianDVDClub server error (HTTP 500) while preparing the torrent. The request stays queued; retry shortly.',
    ),
    'server',
  );
  assert.equal(classifyDeliveryFailure('Delivery poll network error: timed out'), 'server');
  assert.equal(classifyDeliveryFailure('Delivery request has expired.'), 'other');
});
