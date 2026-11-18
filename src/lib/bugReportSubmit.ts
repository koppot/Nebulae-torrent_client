/** Public HTTPS endpoint of the project-controlled report service. No credentials are embedded:
 * the service authenticates nothing about the reporter, so a shipped key would prove nothing. */
export const DEFAULT_REPORT_ENDPOINT = 'https://nebulae-reports.asiandvdclub.org';

export const MAX_DIAGNOSTIC_CHARS = 19000;

export interface ReportFields {
  summary: string;
  description: string;
  steps: string;
  expected: string;
  actual: string;
  category: string;
  contactEmail: string;
}

export interface ReportEnvironment {
  appVersion: string;
  buildId: string;
  os: string;
  osVersion: string;
  arch: string;
  errorFingerprint: string;
}

export type ReportPayload = Record<string, string>;

export function newSubmissionId(): string {
  return crypto.randomUUID();
}

/** Keeps the most recent part of the (already redacted) log text, which is where the failure is. */
export function trimDiagnostics(text: string): string {
  return text.length > MAX_DIAGNOSTIC_CHARS ? text.slice(text.length - MAX_DIAGNOSTIC_CHARS) : text;
}

export function buildPayload(
  fields: ReportFields,
  env: ReportEnvironment,
  diagnostics: string,
  submissionId: string,
): ReportPayload {
  const payload: ReportPayload = {
    summary: fields.summary.trim(),
    description: fields.description.trim(),
    steps: fields.steps.trim(),
    expected: fields.expected.trim(),
    actual: fields.actual.trim(),
    category: fields.category,
    app_version: env.appVersion,
    build_id: env.buildId,
    os: env.os,
    os_version: env.osVersion,
    arch: env.arch,
    diagnostics: trimDiagnostics(diagnostics.trim()),
    error_fingerprint: env.errorFingerprint,
    submission_id: submissionId,
  };
  const email = fields.contactEmail.trim();
  if (email) payload.contact_email = email;
  return payload;
}

/** Plain-text rendering of exactly what will leave this computer. */
export function buildOutgoingPreview(
  payload: ReportPayload,
  screenshots: { name: string; size: number }[],
): string {
  const lines: string[] = [];
  const add = (label: string, value: string | undefined) => {
    if (value) lines.push(`${label}:\n${value}\n`);
  };
  add('Summary', payload.summary);
  add('Category', payload.category);
  add('Description', payload.description);
  add('Steps to reproduce', payload.steps);
  add('Expected behavior', payload.expected);
  add('Actual behavior', payload.actual);
  lines.push(
    `App: ${payload.app_version} (build ${payload.build_id})\nOS: ${payload.os} ${payload.os_version} (${payload.arch})\n`,
  );
  lines.push(
    payload.contact_email
      ? 'Contact e-mail: provided. It is kept only on the report server for follow-up and is not forwarded to Linear or Discord.\n'
      : 'Contact e-mail: not provided.\n',
  );
  if (payload.diagnostics) {
    lines.push(`Diagnostic excerpt (${payload.diagnostics.length} characters, shown in the box above):\n${payload.diagnostics}\n`);
  } else {
    lines.push('Diagnostic excerpt: none.\n');
  }
  lines.push(
    screenshots.length === 0
      ? 'Screenshots: none.'
      : `Screenshots (${screenshots.length}, not redacted):\n${screenshots
          .map((s, i) => `  ${i + 1}. ${s.name} (${(s.size / 1024).toFixed(0)} KiB)`)
          .join('\n')}`,
  );
  return lines.join('\n');
}

export type SubmitOutcome =
  | { kind: 'received'; reportRef: string; receiptToken: string; replay: boolean }
  | { kind: 'rejected'; message: string; code: string }
  | { kind: 'failed_before_receipt'; message: string }
  | { kind: 'unknown'; message: string };

export interface SubmitOptions {
  fetchImpl?: typeof fetch;
  timeoutMs?: number;
  isOnline?: () => boolean;
}

function trimBase(url: string): string {
  return url.replace(/\/+$/, '');
}

interface ErrorBody {
  error?: string;
  message?: string;
}

/**
 * Sends the report. The caller reuses `payload.submission_id` on every retry, so a retry after an
 * unknown outcome returns the original receipt instead of creating a second report.
 */
export async function submitReport(
  baseUrl: string,
  payload: ReportPayload,
  files: File[],
  opts: SubmitOptions = {},
): Promise<SubmitOutcome> {
  const doFetch = opts.fetchImpl ?? fetch;
  const online = opts.isOnline ?? (() => (typeof navigator === 'undefined' ? true : navigator.onLine !== false));
  if (!online()) {
    return { kind: 'failed_before_receipt', message: 'You appear to be offline. Nothing was sent; your report is still here.' };
  }
  const form = new FormData();
  form.append('report', JSON.stringify(payload));
  for (const f of files) form.append('screenshots[]', f, f.name);

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), opts.timeoutMs ?? 120_000);
  let resp: Response;
  try {
    resp = await doFetch(`${trimBase(baseUrl)}/v1/reports`, {
      method: 'POST',
      headers: { 'Idempotency-Key': payload.submission_id },
      body: form,
      signal: controller.signal,
    });
  } catch {
    clearTimeout(timer);
    return online()
      ? {
          kind: 'unknown',
          message:
            'No answer from the report service. The report may or may not have arrived. Sending again uses the same submission ID, so it cannot create a duplicate.',
        }
      : { kind: 'failed_before_receipt', message: 'The connection was lost. Your report is still here; try again when you are online.' };
  }
  clearTimeout(timer);

  let body: unknown = null;
  try {
    body = await resp.json();
  } catch {
    body = null;
  }
  const b = (body ?? {}) as Record<string, unknown> & ErrorBody;
  if ((resp.status === 201 || resp.status === 200) && typeof b.report_ref === 'string' && typeof b.receipt_token === 'string') {
    return { kind: 'received', reportRef: b.report_ref, receiptToken: b.receipt_token, replay: b.replay === true };
  }
  if (typeof b.error === 'string') {
    const message = typeof b.message === 'string' ? b.message : 'The report service rejected the report.';
    // The service answers with an error body only when it stored nothing.
    return resp.status >= 500 || resp.status === 429
      ? { kind: 'failed_before_receipt', message }
      : { kind: 'rejected', message, code: b.error };
  }
  return {
    kind: 'unknown',
    message: `Unexpected answer from the report service (HTTP ${resp.status}). The report may or may not have arrived; sending again is safe.`,
  };
}

export type DeliveryState = 'pending' | 'retrying' | 'delayed' | 'done';

export interface ReportStatus {
  reportRef: string;
  linear: DeliveryState;
  discord: DeliveryState;
}

export async function fetchStatus(
  baseUrl: string,
  receiptToken: string,
  fetchImpl: typeof fetch = fetch,
): Promise<ReportStatus | null> {
  try {
    const resp = await fetchImpl(`${trimBase(baseUrl)}/v1/reports/status`, {
      headers: { Authorization: `Bearer ${receiptToken}` },
    });
    if (!resp.ok) return null;
    const j = (await resp.json()) as Record<string, string>;
    return { reportRef: j.report_ref, linear: j.linear as DeliveryState, discord: j.discord as DeliveryState };
  } catch {
    return null;
  }
}

export function describeDelivery(s: ReportStatus | null): { text: string; complete: boolean } {
  if (!s) return { text: 'Received by the report service. Delivery to the team is processing.', complete: false };
  if (s.linear === 'done' && s.discord === 'done') {
    return { text: 'Delivered: the report is in the team issue tracker and the team has been notified.', complete: true };
  }
  const part = (name: string, st: DeliveryState) =>
    st === 'done' ? `${name} done` : st === 'delayed' ? `${name} delayed (the team will retry)` : `${name} processing`;
  return { text: `Received. ${part('Issue tracker', s.linear)}; ${part('team notification', s.discord)}.`, complete: false };
}
