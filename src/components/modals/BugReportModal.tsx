import React, { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useUiStore } from '../../stores/uiStore';
import {
  Bug,
  X,
  ExternalLink,
  Copy,
  Check,
  AlertTriangle,
  ChevronDown,
  ChevronUp,
  FileText,
  Trash2,
  ImagePlus,
  Send,
  Lock,
} from 'lucide-react';
import {
  MAX_SCREENSHOTS,
  SCREENSHOT_EXTENSIONS,
  SCREENSHOT_LIMITS_TEXT,
  checkScreenshots,
  formatMegabytes,
} from '../../lib/bugReportAttachments';
import {
  DEFAULT_REPORT_ENDPOINT,
  buildOutgoingPreview,
  buildPayload,
  describeDelivery,
  fetchStatus,
  newSubmissionId,
  submitReport,
  trimDiagnostics,
  type ReportStatus,
} from '../../lib/bugReportSubmit';

interface Screenshot {
  id: string;
  file: File;
  url: string;
}

async function imageToPngBlob(file: File): Promise<Blob> {
  const bitmap = await createImageBitmap(file);
  const canvas = document.createElement('canvas');
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('Canvas is unavailable');
  ctx.drawImage(bitmap, 0, 0);
  bitmap.close();
  return await new Promise<Blob>((resolve, reject) =>
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error('Could not encode image'))), 'image/png'),
  );
}

interface BugReportBundle {
  title: string;
  category: string;
  description: string;
  app_version: string;
  build_id: string;
  os: string;
  os_version: string;
  arch: string;
  database_status: string;
  engine_status: string;
  logs_preview: string;
  error_fingerprint: string;
  formatted_markdown: string;
  github_issue_url: string;
  is_url_truncated: boolean;
}

const CATEGORIES = [
  'Playback & Media Center',
  'Torrent Engine & Transfers',
  'Tracker Integration & Swarms',
  'Windows UI & Window Controls',
  'Performance & Responsiveness',
  'Other',
];

type Phase = 'compose' | 'sending' | 'received' | 'github';
type Notice = { tone: 'info' | 'error'; text: string };

const inputClass =
  'w-full px-3 py-1.5 bg-[#1f1f1f] border border-[#333333] rounded text-white text-xs placeholder:text-[#666666] focus:outline-none focus:border-[#e50914] transition';

export const BugReportModal: React.FC = () => {
  const { showBugReport, setShowBugReport, bugReportContext } = useUiStore();

  const [title, setTitle] = useState('');
  const [category, setCategory] = useState(bugReportContext?.category || 'Playback & Media Center');
  const [description, setDescription] = useState('');
  const [steps, setSteps] = useState('');
  const [expected, setExpected] = useState('');
  const [actual, setActual] = useState('');
  const [contactEmail, setContactEmail] = useState('');
  const [includeDiag, setIncludeDiag] = useState(true);
  const [diagnostics, setDiagnostics] = useState('');
  const [showPreview, setShowPreview] = useState(false);
  const [showDiag, setShowDiag] = useState(false);
  const [envBundle, setEnvBundle] = useState<BugReportBundle | null>(null);
  const [copied, setCopied] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [phase, setPhase] = useState<Phase>('compose');
  const [submissionId, setSubmissionId] = useState(() => newSubmissionId());
  const [attempted, setAttempted] = useState(false);
  const [receipt, setReceipt] = useState<{ ref: string; token: string } | null>(null);
  const [delivery, setDelivery] = useState<ReportStatus | null>(null);
  const [endpoint, setEndpoint] = useState<string>(DEFAULT_REPORT_ENDPOINT);
  const [githubBundle, setGithubBundle] = useState<BugReportBundle | null>(null);

  const flushDraft = () => {
    if (phase === 'received') return;
    if (!title.trim() && !description.trim() && !steps.trim()) return;
    const currentDraft = JSON.stringify({ title, category, description, steps, expected, actual, contactEmail, submissionId });
    void invoke('save_bug_report_draft', { draft: currentDraft }).catch(() => {});
  };

  const [screenshots, setScreenshots] = useState<Screenshot[]>([]);
  const [screenshotErrors, setScreenshotErrors] = useState<string[]>([]);
  const [copiedShot, setCopiedShot] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const screenshotsRef = useRef<Screenshot[]>([]);
  screenshotsRef.current = screenshots;
  const restoredRef = useRef(false);
  const diagLoadedRef = useRef(false);
  const sendingRef = useRef(false);

  useEffect(
    () => () => {
      screenshotsRef.current.forEach((s) => URL.revokeObjectURL(s.url));
    },
    [],
  );

  const handlePickScreenshots = (e: React.ChangeEvent<HTMLInputElement>) => {
    const chosen = Array.from(e.target.files ?? []);
    e.target.value = '';
    if (chosen.length === 0) return;
    const attachedBytes = screenshots.reduce((n, s) => n + s.file.size, 0);
    const { accepted, errors } = checkScreenshots(screenshots.length, chosen, attachedBytes);
    setScreenshotErrors(errors);
    if (accepted.length > 0) {
      setScreenshots((prev) => [
        ...prev,
        ...accepted.map((file) => ({
          id: `${file.name}-${file.size}-${Math.random().toString(36).slice(2)}`,
          file,
          url: URL.createObjectURL(file),
        })),
      ]);
    }
  };

  const removeScreenshot = (id: string) => {
    setScreenshots((prev) => {
      const gone = prev.find((s) => s.id === id);
      if (gone) URL.revokeObjectURL(gone.url);
      return prev.filter((s) => s.id !== id);
    });
    setScreenshotErrors([]);
  };

  const resetScreenshots = () => {
    screenshotsRef.current.forEach((s) => URL.revokeObjectURL(s.url));
    setScreenshots([]);
    setScreenshotErrors([]);
    setCopiedShot(null);
  };

  const handleCopyScreenshot = async (shot: Screenshot) => {
    try {
      const png = await imageToPngBlob(shot.file);
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': png })]);
      setCopiedShot(shot.id);
      setTimeout(() => setCopiedShot((cur) => (cur === shot.id ? null : cur)), 2500);
    } catch (err) {
      setNotice({
        tone: 'error',
        text: `Could not copy the image (${String(err)}). Drag the original file from your computer into the GitHub comment box instead.`,
      });
    }
  };

  const debounceTimerRef = useRef<NodeJS.Timeout | null>(null);

  // On first open: restore the saved draft (text only) and load environment + redacted diagnostics.
  useEffect(() => {
    if (!showBugReport) return;
    if (bugReportContext?.category) setCategory(bugReportContext.category);

    if (!restoredRef.current) {
      restoredRef.current = true;
      invoke<string | null>('get_bug_report_draft')
        .then((draftJson) => {
          let hasDraft = false;
          if (draftJson && draftJson.trim()) {
            try {
              const d = JSON.parse(draftJson);
              if (d.title || d.description || d.steps) {
                hasDraft = true;
                if (d.title) setTitle(d.title);
                if (d.category) setCategory(d.category);
                if (d.description) setDescription(d.description);
                if (d.steps) setSteps(d.steps);
                if (d.expected) setExpected(d.expected);
                if (d.actual) setActual(d.actual);
                if (d.contactEmail) setContactEmail(d.contactEmail);
                if (d.submissionId) setSubmissionId(d.submissionId);
              }
            } catch {
              if (draftJson.trim()) {
                hasDraft = true;
                setDescription(draftJson);
              }
            }
          }
          if (!hasDraft && bugReportContext?.error) {
            setTitle(`Error: ${bugReportContext.error.slice(0, 60)}`);
            setDescription(`Encountered error:\n${bugReportContext.error}\n\nSteps leading to error:\n1. `);
          }
        })
        .catch(() => {
          if (bugReportContext?.error) {
            setTitle(`Error: ${bugReportContext.error.slice(0, 60)}`);
            setDescription(`Encountered error:\n${bugReportContext.error}\n\nSteps leading to error:\n1. `);
          }
        });
    }

    invoke<string>('bug_report_endpoint')
      .then(setEndpoint)
      .catch(() => {});
    invoke<BugReportBundle>('generate_bug_report_bundle', {
      title: 'Untitled Issue',
      description: 'No description provided',
      category,
      contextError: bugReportContext?.error || null,
      includeLogs: true,
      screenshotCount: 0,
    })
      .then((b) => {
        setEnvBundle(b);
        if (!diagLoadedRef.current) {
          diagLoadedRef.current = true;
          setDiagnostics(trimDiagnostics(b.logs_preview));
        }
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showBugReport, bugReportContext]);

  // Debounced local draft save. Drafts are never transmitted without an explicit Send.
  useEffect(() => {
    if (!showBugReport || phase === 'received') return;
    if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current);
    debounceTimerRef.current = setTimeout(() => {
      const payload = JSON.stringify({ title, category, description, steps, expected, actual, contactEmail, submissionId });
      invoke('save_bug_report_draft', { draft: payload }).catch(() => {});
    }, 600);
    return () => {
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
        if (title.trim() || description.trim() || steps.trim()) {
          const payload = JSON.stringify({ title, category, description, steps, expected, actual, contactEmail, submissionId });
          void invoke('save_bug_report_draft', { draft: payload }).catch(() => {});
        }
      }
    };
  }, [title, category, description, steps, expected, actual, contactEmail, submissionId, showBugReport, phase]);

  // After the service has durably received the report, follow its delivery state.
  useEffect(() => {
    if (phase !== 'received' || !receipt || !endpoint) return;
    let cancelled = false;
    let tries = 0;
    const tick = async () => {
      const s = await fetchStatus(endpoint, receipt.token);
      if (cancelled) return;
      if (s) setDelivery(s);
      tries += 1;
      const done = s && describeDelivery(s).complete;
      if (!done && tries < 40) timer = setTimeout(tick, 5000);
    };
    let timer = setTimeout(tick, 1500);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [phase, receipt, endpoint]);

  const fieldsNow = () => ({ summary: title, description, steps, expected, actual, category, contactEmail });

  const outgoingPayload = () => {
    const b = envBundle;
    return buildPayload(
      fieldsNow(),
      {
        appVersion: b?.app_version ?? 'unknown',
        buildId: b?.build_id ?? 'unknown',
        os: b?.os ?? 'unknown',
        osVersion: b?.os_version ?? '',
        arch: b?.arch ?? 'unknown',
        errorFingerprint: '',
      },
      includeDiag ? diagnostics : '',
      submissionId,
    );
  };

  const previewText = () =>
    buildOutgoingPreview(
      outgoingPayload(),
      screenshots.map((s) => ({ name: s.file.name, size: s.file.size })),
    );

  const handleCopyPreview = async () => {
    try {
      await navigator.clipboard.writeText(phase === 'github' && githubBundle ? githubBundle.formatted_markdown : previewText());
      setCopied(true);
      setTimeout(() => setCopied(false), 2500);
    } catch {
      setNotice({ tone: 'error', text: 'Could not copy to clipboard automatically.' });
    }
  };

  const startNewReport = async () => {
    setTitle('');
    setDescription('');
    setSteps('');
    setExpected('');
    setActual('');
    setContactEmail('');
    setCategory('Playback & Media Center');
    setDiagnostics('');
    diagLoadedRef.current = false;
    setReceipt(null);
    setDelivery(null);
    setAttempted(false);
    setNotice(null);
    setPhase('compose');
    setGithubBundle(null);
    setSubmissionId(newSubmissionId());
    resetScreenshots();
    await invoke('clear_bug_report_draft').catch(() => {});
  };

  const handleSend = async (e?: React.FormEvent) => {
    e?.preventDefault();
    if (sendingRef.current) return;
    if (!title.trim() || !description.trim()) {
      setNotice({ tone: 'error', text: 'Please provide both a summary and a description.' });
      return;
    }
    if (!endpoint) {
      setNotice({ tone: 'error', text: 'The report service address is not available yet. Please try again.' });
      return;
    }
    sendingRef.current = true;
    setPhase('sending');
    setAttempted(true);
    setNotice({ tone: 'info', text: 'Sending… The report is not confirmed until the service answers.' });
    try {
      const b = await invoke<BugReportBundle>('generate_bug_report_bundle', {
        title,
        description,
        category,
        contextError: bugReportContext?.error || null,
        includeLogs: false,
        screenshotCount: 0,
      });
      const payload = buildPayload(
        fieldsNow(),
        {
          appVersion: b.app_version,
          buildId: b.build_id,
          os: b.os,
          osVersion: b.os_version,
          arch: b.arch,
          errorFingerprint: b.error_fingerprint,
        },
        includeDiag ? diagnostics : '',
        submissionId,
      );
      const outcome = await submitReport(endpoint, payload, screenshots.map((s) => s.file));
      if (outcome.kind === 'received') {
        setReceipt({ ref: outcome.reportRef, token: outcome.receiptToken });
        setDelivery(null);
        setPhase('received');
        setNotice(null);
        // The service now holds the report, so the local draft is no longer needed.
        void invoke('clear_bug_report_draft').catch(() => {});
      } else {
        // Persist draft immediately so failure never loses user text
        const currentDraft = JSON.stringify({ title, category, description, steps, expected, actual, contactEmail, submissionId });
        void invoke('save_bug_report_draft', { draft: currentDraft }).catch(() => {});
        setPhase('compose');
        setNotice({
          tone: 'error',
          text:
            outcome.kind === 'unknown'
              ? outcome.message
              : outcome.kind === 'rejected'
                ? `Not sent: ${outcome.message}`
                : `Failed before receipt: ${outcome.message}`,
        });
      }
    } catch (err) {
      // Persist draft immediately so error never loses user text
      const currentDraft = JSON.stringify({ title, category, description, steps, expected, actual, contactEmail, submissionId });
      void invoke('save_bug_report_draft', { draft: currentDraft }).catch(() => {});
      setPhase('compose');
      setNotice({ tone: 'error', text: `Failed before receipt: ${String(err)}. Your report is still here.` });
    } finally {
      sendingRef.current = false;
    }
  };

  // Secondary path for people who prefer GitHub (needs a GitHub account).
  const handleGithub = async () => {
    if (!title.trim() || !description.trim()) {
      setNotice({ tone: 'error', text: 'Please provide both a summary and a description.' });
      return;
    }
    try {
      const bundle = await invoke<BugReportBundle>('generate_bug_report_bundle', {
        title,
        description,
        category,
        contextError: bugReportContext?.error || null,
        includeLogs: includeDiag,
        screenshotCount: screenshots.length,
      });
      try {
        await navigator.clipboard.writeText(bundle.formatted_markdown);
      } catch {
        /* clipboard may be unavailable */
      }
      await invoke('open_external_link', { url: bundle.github_issue_url });
      setGithubBundle(bundle);
      setPhase('github');
      setNotice(null);
    } catch (err) {
      setNotice({ tone: 'error', text: `Failed to open issue link: ${String(err)}` });
    }
  };

  if (!showBugReport) return null;

  const deliveryInfo = describeDelivery(delivery);
  const canSend = phase !== 'sending' && !!title.trim() && !!description.trim();
  const statusLine =
    phase === 'sending'
      ? 'Sending'
      : phase === 'received'
        ? deliveryInfo.complete
          ? 'Delivery completed'
          : 'Report durably received, processing'
        : attempted
          ? 'Not yet received (your text is kept on this computer)'
          : 'Draft saved locally';

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm select-none font-sans">
      <div className="relative w-full max-w-2xl bg-[#141414] border border-[#333333] rounded-xl shadow-2xl overflow-hidden flex flex-col max-h-[90vh] text-white">
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-[#333333] bg-[#1f1f1f]">
          <div className="flex items-center gap-2">
            <Bug className="w-4 h-4 text-[#e50914]" />
            <h2 className="text-sm font-bold text-white">Report a Bug</h2>
            {receipt && (
              <span className="font-mono text-[10px] px-1.5 py-0.5 rounded bg-[#262626] text-[#a3a3a3]">{receipt.ref}</span>
            )}
          </div>
          <button
            onClick={() => {
              flushDraft();
              setShowBugReport(false);
            }}
            className="p-1 rounded text-[#a3a3a3] hover:text-white transition"
            aria-label="Close"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {(phase === 'compose' || phase === 'sending') && (
          <div
            className="px-5 py-2.5 bg-amber-950/30 border-b border-amber-900/40 text-[11px] text-amber-200/90 flex items-start gap-2.5"
            data-testid="bug-report-disclosure"
          >
            <AlertTriangle className="w-4 h-4 text-amber-400 flex-shrink-0 mt-0.5" />
            <div className="leading-relaxed space-y-1">
              <p>
                <span className="font-semibold text-white">Where your report goes:</span> it is sent over HTTPS to the
                Nebulae project&apos;s report service. No GitHub, Linear or Discord account is needed. The service
                creates an issue in the project&apos;s <span className="font-semibold">Linear workspace</span> (visible to
                everyone who has access to that workspace) and posts a notice with your screenshots in a{' '}
                <span className="font-semibold">private Discord channel</span> for the team.
              </p>
              <p>
                Automatic redaction removes passkeys, tokens, tracker URLs, personal file paths and public IPs from text.
                Screenshots are{' '}
                <span className="font-semibold">not covered by the automatic text redaction</span> and email addresses or
                other personal details you type or leave visible in an image can still be forwarded. Nothing is captured
                automatically, and nothing is sent until you press Send.
              </p>
            </div>
          </div>
        )}

        {phase === 'received' && receipt && (
          <div className="flex-1 p-5 overflow-y-auto space-y-4 text-xs" data-testid="bug-report-received">
            <div className="p-3 rounded-lg bg-[#10241a] border border-emerald-700 text-emerald-100 leading-relaxed">
              <p className="font-semibold text-white mb-1">Report received. Reference {receipt.ref}</p>
              <p data-testid="bug-report-status">{deliveryInfo.text}</p>
              <p className="mt-2 text-emerald-200/80">
                You can close this window; delivery continues on the server. Quote the reference if you contact the team.
              </p>
            </div>
          </div>
        )}

        {phase === 'github' && (
          <div className="flex-1 p-5 overflow-y-auto space-y-4 text-xs" data-testid="bug-report-finish">
            <div className="p-3 rounded-lg bg-[#1e293b] border border-[#38bdf8] text-sky-100 leading-relaxed">
              <p className="font-semibold text-white mb-1">GitHub opened in your browser.</p>
              <p>
                This path needs a GitHub account and is separate from the report service: nothing was sent to the
                Nebulae report service. The report text is on your clipboard. GitHub links cannot carry files, so attach
                any screenshots yourself in the GitHub comment box before pressing &quot;Submit new issue&quot;.
              </p>
            </div>
            {screenshots.length > 0 && (
              <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
                {screenshots.map((shot, i) => (
                  <div key={shot.id} className="border border-[#2d2d2d] rounded-lg overflow-hidden bg-[#181818]">
                    <img src={shot.url} alt={`Screenshot ${i + 1}`} className="w-full h-24 object-cover bg-black" />
                    <div className="p-2 space-y-1.5">
                      <div className="text-[10px] text-[#a3a3a3] truncate" title={shot.file.name}>
                        Screenshot {i + 1} · {formatMegabytes(shot.file.size)}
                      </div>
                      <button
                        type="button"
                        onClick={() => handleCopyScreenshot(shot)}
                        className="w-full flex items-center justify-center gap-1 px-2 py-1 rounded bg-[#262626] hover:bg-[#333333] text-white text-[11px] transition"
                      >
                        {copiedShot === shot.id ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
                        <span>{copiedShot === shot.id ? 'Copied' : 'Copy image'}</span>
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
            {notice && (
              <div className="p-2.5 rounded bg-[#1e293b] border border-[#38bdf8] text-sky-200 text-xs">{notice.text}</div>
            )}
          </div>
        )}

        {(phase === 'compose' || phase === 'sending') && (
          <form onSubmit={handleSend} className="flex-1 p-5 overflow-y-auto space-y-4 text-xs">
            <div className="flex items-center justify-between text-[11px]" data-testid="bug-report-status">
              <span className="text-[#a3a3a3]">Status: <span className="text-white font-medium">{statusLine}</span></span>
              {envBundle && (
                <span className="text-[#737373]">
                  Nebulae {envBundle.app_version} (build {envBundle.build_id}) · {envBundle.os} {envBundle.os_version} · {envBundle.arch}
                </span>
              )}
            </div>

            {notice && (
              <div
                role="alert"
                className={`p-2.5 rounded border text-xs flex items-center justify-between ${
                  notice.tone === 'error'
                    ? 'bg-red-950/40 border-red-800 text-red-200'
                    : 'bg-[#1e293b] border-[#38bdf8] text-sky-200'
                }`}
              >
                <span>{notice.text}</span>
                <button type="button" onClick={() => setNotice(null)} className="text-[#a3a3a3] hover:text-white ml-2">
                  <X className="w-3.5 h-3.5" />
                </button>
              </div>
            )}

            {bugReportContext?.error && (
              <div className="p-3 bg-[#1f1f1f] border border-[#404040] rounded-lg">
                <span className="text-[10px] text-[#e50914] font-bold uppercase tracking-wider block mb-1">Contextual Error Captured</span>
                <code className="text-[11px] text-[#cccccc] font-mono whitespace-pre-wrap break-all block">{bugReportContext.error}</code>
              </div>
            )}

            <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
              <div>
                <label className="block text-[11px] font-medium text-[#a3a3a3] mb-1">Category</label>
                <select
                  value={category}
                  onChange={(e) => setCategory(e.target.value)}
                  className="w-full px-2.5 py-1.5 bg-[#1f1f1f] border border-[#333333] rounded text-white text-xs focus:outline-none focus:border-[#e50914] transition"
                >
                  {CATEGORIES.map((c) => (
                    <option key={c} value={c}>
                      {c}
                    </option>
                  ))}
                </select>
              </div>
              <div className="md:col-span-2">
                <label className="block text-[11px] font-medium text-[#a3a3a3] mb-1">
                  Short summary <span className="text-[#e50914]">*</span>
                </label>
                <input
                  type="text"
                  required
                  maxLength={120}
                  placeholder="e.g. Window controls unresponsive after restoring from taskbar"
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  className={inputClass}
                />
              </div>
            </div>

            <div>
              <label className="block text-[11px] font-medium text-[#a3a3a3] mb-1">
                Description <span className="text-[#e50914]">*</span>
              </label>
              <textarea
                required
                rows={3}
                maxLength={8000}
                placeholder="What happened?"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                className={`${inputClass} resize-y leading-relaxed font-sans`}
              />
            </div>
            <div>
              <label className="block text-[11px] font-medium text-[#a3a3a3] mb-1">Steps to reproduce</label>
              <textarea
                rows={2}
                maxLength={4000}
                placeholder="1. …"
                value={steps}
                onChange={(e) => setSteps(e.target.value)}
                className={`${inputClass} resize-y leading-relaxed font-sans`}
              />
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div>
                <label className="block text-[11px] font-medium text-[#a3a3a3] mb-1">Expected behavior</label>
                <textarea rows={2} maxLength={2000} value={expected} onChange={(e) => setExpected(e.target.value)} className={`${inputClass} resize-y font-sans`} />
              </div>
              <div>
                <label className="block text-[11px] font-medium text-[#a3a3a3] mb-1">Actual behavior</label>
                <textarea rows={2} maxLength={2000} value={actual} onChange={(e) => setActual(e.target.value)} className={`${inputClass} resize-y font-sans`} />
              </div>
            </div>

            <div className="p-3 bg-[#1a1a1a] border border-[#2d2d2d] rounded-lg space-y-1.5">
              <label className="block text-[11px] font-medium text-[#a3a3a3]">
                <span className="inline-flex items-center gap-1">
                  <Lock className="w-3 h-3" /> Contact e-mail (optional)
                </span>
              </label>
              <input
                type="email"
                autoComplete="off"
                maxLength={254}
                placeholder="Only if you want a follow-up about this report"
                value={contactEmail}
                onChange={(e) => setContactEmail(e.target.value)}
                className={inputClass}
                data-testid="bug-report-email"
              />
              <p className="text-[11px] text-[#737373] leading-relaxed">
                Used only to follow up on this report. It is stored only on the report server, is not copied to Linear,
                Discord or GitHub, and is deleted after 90 days. We never fill it in for you and never e-mail you
                automatically.
              </p>
            </div>

            <div className="p-3 bg-[#1a1a1a] border border-[#2d2d2d] rounded-lg space-y-2" data-testid="bug-report-screenshots">
              <div className="flex items-center justify-between">
                <div>
                  <span className="text-white font-medium">Screenshots (optional)</span>
                  <span className="block text-[11px] text-[#737373]">{SCREENSHOT_LIMITS_TEXT}. Nothing is captured automatically.</span>
                </div>
                <button
                  type="button"
                  onClick={() => fileInputRef.current?.click()}
                  disabled={screenshots.length >= MAX_SCREENSHOTS}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-[#262626] hover:bg-[#333333] text-white text-xs font-medium transition disabled:opacity-40"
                >
                  <ImagePlus className="w-3.5 h-3.5" />
                  <span>Add screenshots…</span>
                </button>
                <input
                  ref={fileInputRef}
                  type="file"
                  multiple
                  accept={SCREENSHOT_EXTENSIONS.map((x) => `.${x}`).join(',')}
                  onChange={handlePickScreenshots}
                  className="hidden"
                  data-testid="bug-report-file-input"
                />
              </div>
              <div className="text-[11px] text-amber-200/90 leading-relaxed">
                Review every screenshot before you send it: images can show passkeys, tokens, usernames, private file
                paths or other personal details. Screenshots are{' '}
                <span className="font-semibold">not covered by the automatic text redaction</span>. Selected images are
                sent to the team (Linear and the private Discord channel) only when you press Send.
              </div>
              {screenshotErrors.length > 0 && (
                <ul className="text-[11px] text-red-300 list-disc ml-4 space-y-0.5" role="alert">
                  {screenshotErrors.map((m) => (
                    <li key={m}>{m}</li>
                  ))}
                </ul>
              )}
              {screenshots.length > 0 && (
                <div className="grid grid-cols-3 sm:grid-cols-5 gap-2">
                  {screenshots.map((shot, i) => (
                    <div key={shot.id} className="relative border border-[#2d2d2d] rounded overflow-hidden bg-black">
                      <img src={shot.url} alt={`Screenshot ${i + 1} preview`} className="w-full h-16 object-cover" />
                      <button
                        type="button"
                        onClick={() => removeScreenshot(shot.id)}
                        className="absolute top-0.5 right-0.5 p-0.5 rounded bg-black/70 text-white hover:bg-[#e50914] transition"
                        aria-label={`Remove screenshot ${i + 1}`}
                        title="Remove this screenshot"
                      >
                        <X className="w-3 h-3" />
                      </button>
                      <div className="px-1 py-0.5 text-[9px] text-[#a3a3a3] truncate" title={shot.file.name}>
                        {shot.file.name} · {formatMegabytes(shot.file.size)}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>

            <div className="border border-[#2d2d2d] rounded-lg overflow-hidden bg-[#181818]">
              <div className="p-3 bg-[#1a1a1a] flex items-center justify-between">
                <label className="flex items-center gap-2 cursor-pointer select-none">
                  <input
                    type="checkbox"
                    checked={includeDiag}
                    onChange={(e) => setIncludeDiag(e.target.checked)}
                    className="rounded border-[#333333] bg-[#222222] text-[#e50914] focus:ring-0"
                  />
                  <span className="text-white font-medium">Include a redacted diagnostic excerpt</span>
                </label>
                <button
                  type="button"
                  onClick={() => setShowDiag(!showDiag)}
                  className="text-[11px] text-[#e50914] hover:underline"
                >
                  {showDiag ? 'Hide' : 'Review / edit'}
                </button>
              </div>
              {showDiag && (
                <div className="p-3 border-t border-[#2d2d2d]">
                  <textarea
                    rows={8}
                    value={diagnostics}
                    onChange={(e) => setDiagnostics(e.target.value)}
                    disabled={!includeDiag}
                    className="w-full p-2 bg-[#111111] border border-[#262626] rounded text-[11px] text-[#a3a3a3] font-mono leading-tight disabled:opacity-40"
                    data-testid="bug-report-diagnostics"
                  />
                  <p className="text-[11px] text-[#737373] mt-1">
                    Latest log lines after automatic redaction. You can delete anything you do not want to send. The full log
                    file is never collected.
                  </p>
                </div>
              )}
            </div>

            <div className="border border-[#2d2d2d] rounded-lg overflow-hidden bg-[#181818]">
              <button
                type="button"
                onClick={() => setShowPreview(!showPreview)}
                className="w-full px-3 py-2 flex items-center justify-between text-[#a3a3a3] hover:text-white transition bg-[#1c1c1c]"
              >
                <div className="flex items-center gap-2">
                  <FileText className="w-3.5 h-3.5 text-[#e50914]" />
                  <span className="font-medium text-xs">{showPreview ? 'Hide outgoing report' : 'Review exactly what will be sent'}</span>
                </div>
                {showPreview ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
              </button>
              {showPreview && (
                <div className="p-3 border-t border-[#2d2d2d]">
                  <pre
                    className="p-3 bg-[#111111] border border-[#262626] rounded text-[11px] text-[#a3a3a3] font-mono max-h-56 overflow-y-auto whitespace-pre-wrap select-all leading-tight"
                    data-testid="bug-report-preview"
                  >
                    {previewText()}
                  </pre>
                </div>
              )}
            </div>
          </form>
        )}

        {phase === 'received' && (
          <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-[#333333] bg-[#1f1f1f]">
            <button
              type="button"
              onClick={async () => {
                if (receipt) await navigator.clipboard.writeText(receipt.ref).catch(() => {});
              }}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-[#262626] hover:bg-[#333333] text-white text-xs font-medium transition"
            >
              <Copy className="w-3.5 h-3.5" />
              <span>Copy reference</span>
            </button>
            <button
              type="button"
              onClick={() => {
                void startNewReport();
                setShowBugReport(false);
              }}
              className="px-4 py-1.5 rounded bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold transition"
            >
              Done
            </button>
          </div>
        )}

        {phase === 'github' && (
          <div className="flex items-center justify-between px-5 py-3 border-t border-[#333333] bg-[#1f1f1f]">
            <button
              type="button"
              onClick={() => setPhase('compose')}
              className="px-3 py-1.5 rounded hover:bg-[#262626] text-[#a3a3a3] hover:text-white text-xs transition"
            >
              Back to report
            </button>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={handleCopyPreview}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-[#262626] hover:bg-[#333333] text-white text-xs font-medium transition"
              >
                {copied ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                <span>{copied ? 'Copied!' : 'Copy report text'}</span>
              </button>
              <button
                type="button"
                onClick={() => {
                  void startNewReport();
                  setShowBugReport(false);
                }}
                className="px-4 py-1.5 rounded bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold transition"
              >
                Done
              </button>
            </div>
          </div>
        )}

        {(phase === 'compose' || phase === 'sending') && (
          <div className="flex items-center justify-between px-5 py-3 border-t border-[#333333] bg-[#1f1f1f]">
            <div className="flex items-center gap-1">
              <button
                type="button"
                onClick={() => void startNewReport()}
                disabled={phase === 'sending'}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded hover:bg-[#262626] text-[#a3a3a3] hover:text-red-400 text-xs transition disabled:opacity-40"
                title="Discard current draft"
              >
                <Trash2 className="w-3.5 h-3.5" />
                <span>Discard Draft</span>
              </button>
              <button
                type="button"
                onClick={() => void handleGithub()}
                disabled={phase === 'sending'}
                className="flex items-center gap-1.5 px-2 py-1.5 rounded hover:bg-[#262626] text-[#737373] hover:text-white text-[11px] transition disabled:opacity-40"
                title="Alternative that needs a GitHub account"
              >
                <ExternalLink className="w-3 h-3" />
                <span>Use GitHub instead</span>
              </button>
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={handleCopyPreview}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-[#262626] hover:bg-[#333333] text-white text-xs font-medium transition"
                title="Copy the outgoing report text"
              >
                {copied ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                <span>{copied ? 'Copied!' : 'Copy'}</span>
              </button>
              <button
                type="button"
                onClick={() => void handleSend()}
                disabled={!canSend}
                data-testid="bug-report-send"
                className="flex items-center gap-1.5 px-4 py-1.5 rounded bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold transition disabled:opacity-40 shadow-sm"
              >
                <Send className="w-3.5 h-3.5" />
                <span>{phase === 'sending' ? 'Sending…' : attempted ? 'Send again' : 'Send report'}</span>
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default BugReportModal;
