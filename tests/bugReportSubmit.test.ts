import test from 'node:test';
import assert from 'node:assert/strict';
import {
  buildOutgoingPreview,
  buildPayload,
  describeDelivery,
  fetchStatus,
  MAX_DIAGNOSTIC_CHARS,
  newSubmissionId,
  submitReport,
  trimDiagnostics,
} from '../src/lib/bugReportSubmit.ts';

const fields = {
  summary: ' Crash on open ',
  description: 'It crashed',
  steps: '',
  expected: '',
  actual: '',
  category: 'Other',
  contactEmail: '',
};
const env = { appVersion: '0.2.8', buildId: 'abc', os: 'macOS', osVersion: '15.1', arch: 'aarch64', errorFingerprint: 'FP1' };
const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
const online = { isOnline: () => true };
const asFetch = (f: unknown) => f as typeof fetch;

test('payload omits the contact field when no e-mail is given and trims text', () => {
  const p = buildPayload(fields, env, 'log', newSubmissionId());
  assert.equal(p.summary, 'Crash on open');
  assert.ok(!('contact_email' in p));
  const withMail = buildPayload({ ...fields, contactEmail: ' me@example.org ' }, env, '', newSubmissionId());
  assert.equal(withMail.contact_email, 'me@example.org');
});

test('diagnostics keep the most recent characters within the service limit', () => {
  const long = 'a'.repeat(MAX_DIAGNOSTIC_CHARS) + 'TAIL';
  const t = trimDiagnostics(long);
  assert.equal(t.length, MAX_DIAGNOSTIC_CHARS);
  assert.ok(t.endsWith('TAIL'));
});

test('preview states exactly what is sent and how the e-mail is handled', () => {
  const p = buildPayload({ ...fields, contactEmail: 'me@example.org' }, env, 'engine ok', 'id');
  const text = buildOutgoingPreview(p, [{ name: 'a.png', size: 2048 }]);
  assert.match(text, /Crash on open/);
  assert.match(text, /not forwarded to Linear or Discord/);
  assert.doesNotMatch(text, /me@example\.org/);
  assert.match(text, /Screenshots \(1, not redacted\)/);
  assert.match(text, /a\.png/);
});

test('a 201 answer is a durable receipt and carries the same submission id header', async () => {
  let seen: RequestInit | undefined;
  const fetchImpl = asFetch(async (_u: string, init?: RequestInit) => {
    seen = init;
    return json(201, { status: 'received', report_ref: 'NBR-ABC', receipt_token: 'tok', replay: false });
  });
  const payload = buildPayload(fields, env, '', 'sid-1');
  const out = await submitReport('https://x.test/', payload, [], { fetchImpl, ...online });
  assert.deepEqual(out, { kind: 'received', reportRef: 'NBR-ABC', receiptToken: 'tok', replay: false });
  assert.equal((seen?.headers as Record<string, string>)['Idempotency-Key'], 'sid-1');
  assert.ok(seen?.body instanceof FormData);
  assert.equal((seen?.body as FormData).getAll('screenshots[]').length, 0);
});

test('screenshots are sent as files in the same request', async () => {
  let body: FormData | undefined;
  const fetchImpl = asFetch(async (_u: string, init?: RequestInit) => {
    body = init?.body as FormData;
    return json(201, { report_ref: 'NBR-1', receipt_token: 't' });
  });
  const file = new File([new Uint8Array([1, 2, 3])], 's.png', { type: 'image/png' });
  await submitReport('https://x.test', buildPayload(fields, env, '', 'sid'), [file, file], { fetchImpl, ...online });
  assert.equal(body?.getAll('screenshots[]').length, 2);
});

test('server rejections surface the service message and are not receipts', async () => {
  const fetchImpl = asFetch(async () => json(415, { error: 'screenshot_type', message: 'Screenshots must be PNG, JPEG or WebP images.' }));
  const out = await submitReport('https://x.test', buildPayload(fields, env, '', 'sid'), [], { fetchImpl, ...online });
  assert.equal(out.kind, 'rejected');
  assert.match((out as { message: string }).message, /PNG, JPEG or WebP/);
});

test('503 with an error body means nothing was stored', async () => {
  const fetchImpl = asFetch(async () => json(503, { error: 'storage_full', message: 'Cannot accept attachments.' }));
  const out = await submitReport('https://x.test', buildPayload(fields, env, '', 'sid'), [], { fetchImpl, ...online });
  assert.equal(out.kind, 'failed_before_receipt');
});

test('an HTML 502 from a proxy is an unknown outcome, never a receipt', async () => {
  const fetchImpl = asFetch(async () => new Response('<html>Bad gateway</html>', { status: 502 }));
  const out = await submitReport('https://x.test', buildPayload(fields, env, '', 'sid'), [], { fetchImpl, ...online });
  assert.equal(out.kind, 'unknown');
});

test('a network error while online is unknown; offline is failed before receipt', async () => {
  const boom = asFetch(async () => {
    throw new TypeError('network');
  });
  const p = buildPayload(fields, env, '', 'sid');
  assert.equal((await submitReport('https://x.test', p, [], { fetchImpl: boom, ...online })).kind, 'unknown');
  assert.equal((await submitReport('https://x.test', p, [], { fetchImpl: boom, isOnline: () => false })).kind, 'failed_before_receipt');
});

test('a timeout aborts the request and reports an unknown outcome', async () => {
  const hang = asFetch(
    (_u: string, init?: RequestInit) =>
      new Promise((_r, reject) => {
        init?.signal?.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')));
      }),
  );
  const out = await submitReport('https://x.test', buildPayload(fields, env, '', 'sid'), [], { fetchImpl: hang, timeoutMs: 20, ...online });
  assert.equal(out.kind, 'unknown');
  assert.match((out as { message: string }).message, /same submission ID/);
});

test('a retry with the same submission id receives the original receipt', async () => {
  let calls = 0;
  const fetchImpl = asFetch(async () => {
    calls += 1;
    if (calls === 1) throw new TypeError('lost');
    return json(200, { report_ref: 'NBR-SAME', receipt_token: 'tok', replay: true });
  });
  const p = buildPayload(fields, env, '', 'sid-retry');
  assert.equal((await submitReport('https://x.test', p, [], { fetchImpl, ...online })).kind, 'unknown');
  const second = await submitReport('https://x.test', p, [], { fetchImpl, ...online });
  assert.deepEqual(second, { kind: 'received', reportRef: 'NBR-SAME', receiptToken: 'tok', replay: true });
});

test('status uses a bearer token header, never the URL, and maps delivery text', async () => {
  let url = '';
  let auth = '';
  const fetchImpl = asFetch(async (u: string, init?: RequestInit) => {
    url = u;
    auth = (init?.headers as Record<string, string>).Authorization;
    return json(200, { report_ref: 'NBR-1', linear: 'done', discord: 'retrying' });
  });
  const s = await fetchStatus('https://x.test', 'secret-token', fetchImpl);
  assert.ok(!url.includes('secret-token'));
  assert.equal(auth, 'Bearer secret-token');
  const d = describeDelivery(s);
  assert.equal(d.complete, false);
  assert.match(d.text, /Issue tracker done/);
  assert.equal(describeDelivery({ reportRef: 'x', linear: 'done', discord: 'done' }).complete, true);
  assert.match(describeDelivery(null).text, /Received by the report service/);
});
