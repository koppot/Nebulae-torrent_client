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
} from 'lucide-react';
import {
  MAX_SCREENSHOTS,
  SCREENSHOT_EXTENSIONS,
  SCREENSHOT_LIMITS_TEXT,
  checkScreenshots,
  formatMegabytes,
} from '../../lib/bugReportAttachments';

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
  os: string;
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

export const BugReportModal: React.FC = () => {
  const { showBugReport, setShowBugReport, bugReportContext } = useUiStore();

  const [title, setTitle] = useState('');
  const [category, setCategory] = useState(bugReportContext?.category || 'Playback & Media Center');
  const [description, setDescription] = useState('');
  const [includeLogs, setIncludeLogs] = useState(true);
  const [showPreview, setShowPreview] = useState(false);
  const [previewBundle, setPreviewBundle] = useState<BugReportBundle | null>(null);
  const [copied, setCopied] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [statusNotice, setStatusNotice] = useState<string | null>(null);

  const [screenshots, setScreenshots] = useState<Screenshot[]>([]);
  const [screenshotErrors, setScreenshotErrors] = useState<string[]>([]);
  const [step, setStep] = useState<'compose' | 'finish'>('compose');
  const [copiedShot, setCopiedShot] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const screenshotsRef = useRef<Screenshot[]>([]);
  screenshotsRef.current = screenshots;

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
    const { accepted, errors } = checkScreenshots(screenshots.length, chosen);
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
      setStatusNotice(
        `Could not copy the image (${String(err)}). Drag the original file from your computer into the GitHub comment box instead.`,
      );
    }
  };

  const debounceTimerRef = useRef<NodeJS.Timeout | null>(null);

  // Restore draft or initial context on modal open
  useEffect(() => {
    if (!showBugReport) return;

    if (bugReportContext?.category) {
      setCategory(bugReportContext.category);
    }
    if (bugReportContext?.error) {
      setTitle(`Error: ${bugReportContext.error.slice(0, 60)}`);
      setDescription(`Encountered error:\n${bugReportContext.error}\n\nSteps leading to error:\n1. `);
    } else {
      // Load saved draft from SQLite if not overriding with contextual error
      invoke<string | null>('get_bug_report_draft')
        .then((draftJson) => {
          if (draftJson && draftJson.trim()) {
            try {
              const draft = JSON.parse(draftJson);
              if (draft.title) setTitle(draft.title);
              if (draft.category) setCategory(draft.category);
              if (draft.description) setDescription(draft.description);
            } catch {
              setDescription(draftJson);
            }
          }
        })
        .catch(() => {});
    }
  }, [showBugReport, bugReportContext]);

  // Debounced auto-save draft to SQLite
  useEffect(() => {
    if (!showBugReport) return;
    if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current);

    debounceTimerRef.current = setTimeout(() => {
      const payload = JSON.stringify({ title, category, description });
      invoke('save_bug_report_draft', { draft: payload }).catch(() => {});
    }, 600);

    return () => {
      if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current);
    };
  }, [title, category, description, showBugReport]);

  // Update preview bundle on form changes
  useEffect(() => {
    if (!showBugReport) return;

    invoke<BugReportBundle>('generate_bug_report_bundle', {
      title: title || 'Untitled Issue',
      description: description || 'No description provided',
      category,
      contextError: bugReportContext?.error || null,
      includeLogs,
      screenshotCount: screenshots.length,
    })
      .then((bundle) => {
        setPreviewBundle(bundle);
      })
      .catch(() => {});
  }, [title, category, description, includeLogs, bugReportContext, showBugReport, screenshots.length]);

  const handleCopyMarkdown = async () => {
    if (!previewBundle) return;
    try {
      await navigator.clipboard.writeText(previewBundle.formatted_markdown);
      setCopied(true);
      setTimeout(() => setCopied(false), 2500);
    } catch {
      setStatusNotice('Could not copy to clipboard automatically.');
    }
  };

  const handleClearDraft = async () => {
    setTitle('');
    setDescription('');
    setCategory('Playback & Media Center');
    resetScreenshots();
    await invoke('clear_bug_report_draft').catch(() => {});
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim() || !description.trim()) {
      setStatusNotice('Please provide both a title and description.');
      return;
    }

    setSubmitting(true);
    setStatusNotice(null);

    try {
      const bundle = await invoke<BugReportBundle>('generate_bug_report_bundle', {
        title,
        description,
        category,
        contextError: bugReportContext?.error || null,
        includeLogs,
        screenshotCount: screenshots.length,
      });

      // Always copy full markdown to clipboard so the user has the complete trace
      try {
        await navigator.clipboard.writeText(bundle.formatted_markdown);
      } catch {}

      // Open prefilled GitHub issue URL in default browser
      await invoke('open_external_link', { url: bundle.github_issue_url });

      // The draft is kept: the browser may open to an offline error page, and the
      // report must survive that. "Done" / "Discard Draft" clear it explicitly.

      if (screenshots.length > 0) {
        // GitHub's prefilled-issue link cannot carry files: hand the user to the
        // browser form and keep this dialog (and the report) open for the last step.
        setSubmitting(false);
        setStep('finish');
        setStatusNotice(null);
        return;
      }

      setStatusNotice(
        bundle.is_url_truncated
          ? 'GitHub issue opened in your browser. The full diagnostic log was copied to your clipboard! Your report stays saved here as a draft until you discard it.'
          : 'GitHub issue opened in your browser. Your report stays saved here as a draft until you discard it.',
      );

      setTimeout(() => {
        setSubmitting(false);
        setShowBugReport(false);
      }, 1500);
    } catch (err) {
      setSubmitting(false);
      setStatusNotice(`Failed to open issue link: ${String(err)}`);
    }
  };

  if (!showBugReport) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm select-none font-sans">
      <div className="relative w-full max-w-2xl bg-[#141414] border border-[#333333] rounded-xl shadow-2xl overflow-hidden flex flex-col max-h-[90vh] text-white">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-[#333333] bg-[#1f1f1f]">
          <div className="flex items-center gap-2">
            <Bug className="w-4 h-4 text-[#e50914]" />
            <h2 className="text-sm font-bold text-white">Report an Issue / Bug</h2>
            {previewBundle && (
              <span className="font-mono text-[10px] px-1.5 py-0.5 rounded bg-[#262626] text-[#a3a3a3]">
                ID: {previewBundle.error_fingerprint}
              </span>
            )}
          </div>
          <button
            onClick={() => setShowBugReport(false)}
            className="p-1 rounded text-[#a3a3a3] hover:text-white transition"
            aria-label="Close"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Public Disclosure Notice */}
        <div className="px-5 py-2.5 bg-amber-950/30 border-b border-amber-900/40 text-[11px] text-amber-200/90 flex items-start gap-2.5">
          <AlertTriangle className="w-4 h-4 text-amber-400 flex-shrink-0 mt-0.5" />
          <div className="leading-relaxed">
            <span className="font-semibold text-white">Public Issue Tracker:</span> Submitting
            this report generates a prefilled issue form on GitHub in your web browser (<code className="text-amber-300">koppot/Nebulae-torrent_client</code>).
            Zero embedded tokens or automated background posts occur.
            Automated privacy redaction removes passkeys, tokens, tracker URLs, personal file paths,
            and public IPs. You can review the exact text before opening.
          </div>
        </div>

        {step === 'finish' && (
          <div className="flex-1 p-5 overflow-y-auto space-y-4 text-xs" data-testid="bug-report-finish">
            <div className="p-3 rounded-lg bg-[#1e293b] border border-[#38bdf8] text-sky-100 leading-relaxed">
              <p className="font-semibold text-white mb-1">GitHub opened in your browser. One step is left.</p>
              <p>
                Your screenshots are <span className="font-semibold">not attached automatically</span> and
                have not been uploaded anywhere. GitHub issue links cannot carry files, so attach them
                yourself in the GitHub comment box before pressing &quot;Submit new issue&quot;:
              </p>
              <ol className="list-decimal ml-5 mt-2 space-y-0.5">
                <li>Click &quot;Copy image&quot; below, then paste (Ctrl/Cmd+V) into the GitHub comment box.</li>
                <li>Or drag the original image file from your computer into the comment box.</li>
                <li>Check that every image appears in the preview, then submit.</li>
              </ol>
              <p className="mt-2 text-sky-200/80">
                If the comment box shows only a short note, the full diagnostic text is on your clipboard
                (use &quot;Copy report text&quot; to copy it again; it replaces a copied image).
              </p>
            </div>
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
            {statusNotice && (
              <div className="p-2.5 rounded bg-[#1e293b] border border-[#38bdf8] text-sky-200 text-xs">{statusNotice}</div>
            )}
          </div>
        )}

        {/* Body Form */}
        {step === 'compose' && (
        <form onSubmit={handleSubmit} className="flex-1 p-5 overflow-y-auto space-y-4 text-xs">
          {/* Status Message */}
          {statusNotice && (
            <div className="p-2.5 rounded bg-[#1e293b] border border-[#38bdf8] text-sky-200 text-xs flex items-center justify-between">
              <span>{statusNotice}</span>
              <button
                type="button"
                onClick={() => setStatusNotice(null)}
                className="text-sky-400 hover:text-white"
              >
                <X className="w-3.5 h-3.5" />
              </button>
            </div>
          )}

          {/* Captured Error Banner if contextual */}
          {bugReportContext?.error && (
            <div className="p-3 bg-[#1f1f1f] border border-[#404040] rounded-lg">
              <span className="text-[10px] text-[#e50914] font-bold uppercase tracking-wider block mb-1">
                Contextual Error Captured
              </span>
              <code className="text-[11px] text-[#cccccc] font-mono whitespace-pre-wrap break-all block">
                {bugReportContext.error}
              </code>
            </div>
          )}

          {/* Category & Title */}
          <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
            <div>
              <label className="block text-[11px] font-medium text-[#a3a3a3] mb-1">
                Category
              </label>
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
                Issue Summary / Title <span className="text-[#e50914]">*</span>
              </label>
              <input
                type="text"
                required
                placeholder="e.g. Window controls unresponsive after restoring from taskbar"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                className="w-full px-3 py-1.5 bg-[#1f1f1f] border border-[#333333] rounded text-white text-xs placeholder:text-[#666666] focus:outline-none focus:border-[#e50914] transition"
              />
            </div>
          </div>

          {/* Description */}
          <div>
            <label className="block text-[11px] font-medium text-[#a3a3a3] mb-1">
              Description & Reproduction Steps <span className="text-[#e50914]">*</span>
            </label>
            <textarea
              required
              rows={4}
              placeholder="What happened? What were you doing when the bug occurred? Expected behavior vs actual result..."
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              className="w-full px-3 py-2 bg-[#1f1f1f] border border-[#333333] rounded text-white text-xs placeholder:text-[#666666] focus:outline-none focus:border-[#e50914] transition resize-y leading-relaxed font-sans"
            />
          </div>

          {/* Screenshots (optional) */}
          <div className="p-3 bg-[#1a1a1a] border border-[#2d2d2d] rounded-lg space-y-2" data-testid="bug-report-screenshots">
            <div className="flex items-center justify-between">
              <div>
                <span className="text-white font-medium">Screenshots (optional)</span>
                <span className="block text-[11px] text-[#737373]">
                  {SCREENSHOT_LIMITS_TEXT}. Nothing is captured or uploaded by Nebulae.
                </span>
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
                accept={SCREENSHOT_EXTENSIONS.map((e) => `.${e}`).join(',')}
                onChange={handlePickScreenshots}
                className="hidden"
                data-testid="bug-report-file-input"
              />
            </div>
            <div className="text-[11px] text-amber-200/90 leading-relaxed">
              Review every screenshot before you attach it: images can show passkeys, tokens, usernames,
              private file paths or other personal details. Screenshots are{' '}
              <span className="font-semibold">not covered by the automatic text redaction</span>, and they are{' '}
              <span className="font-semibold">not attached automatically</span>: after GitHub opens you
              add them to the issue yourself, and nothing is sent until you submit it there.
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

          {/* Log Bundle Toggle */}
          <div className="p-3 bg-[#1a1a1a] border border-[#2d2d2d] rounded-lg flex items-center justify-between">
            <label className="flex items-center gap-2 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={includeLogs}
                onChange={(e) => setIncludeLogs(e.target.checked)}
                className="rounded border-[#333333] bg-[#222222] text-[#e50914] focus:ring-0"
              />
              <span className="text-white font-medium">Include sanitized diagnostic logs</span>
            </label>
            <span className="text-[11px] text-[#737373]">
              {includeLogs ? 'Latest 100 log lines attached' : 'Logs excluded'}
            </span>
          </div>

          {/* Collapsible Disclosure Preview */}
          <div className="border border-[#2d2d2d] rounded-lg overflow-hidden bg-[#181818]">
            <button
              type="button"
              onClick={() => setShowPreview(!showPreview)}
              className="w-full px-3 py-2 flex items-center justify-between text-[#a3a3a3] hover:text-white transition bg-[#1c1c1c]"
            >
              <div className="flex items-center gap-2">
                <FileText className="w-3.5 h-3.5 text-[#e50914]" />
                <span className="font-medium text-xs">
                  {showPreview ? 'Hide exact submission preview' : 'Review exact submission preview'}
                </span>
              </div>
              {showPreview ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
            </button>

            {showPreview && previewBundle && (
              <div className="p-3 border-t border-[#2d2d2d] space-y-2">
                <div className="flex items-center justify-between text-[11px] text-[#737373]">
                  <span>Markdown preview ({previewBundle.formatted_markdown.length} bytes):</span>
                  <button
                    type="button"
                    onClick={handleCopyMarkdown}
                    className="flex items-center gap-1 text-[#e50914] hover:underline"
                  >
                    {copied ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
                    <span>{copied ? 'Copied' : 'Copy Preview'}</span>
                  </button>
                </div>
                <pre className="p-3 bg-[#111111] border border-[#262626] rounded text-[11px] text-[#a3a3a3] font-mono max-h-48 overflow-y-auto whitespace-pre-wrap select-all leading-tight">
                  {previewBundle.formatted_markdown}
                </pre>
              </div>
            )}
          </div>
        </form>
        )}

        {step === 'finish' && (
          <div className="flex items-center justify-between px-5 py-3 border-t border-[#333333] bg-[#1f1f1f]">
            <button
              type="button"
              onClick={() => setStep('compose')}
              className="px-3 py-1.5 rounded hover:bg-[#262626] text-[#a3a3a3] hover:text-white text-xs transition"
            >
              Back to report
            </button>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={handleCopyMarkdown}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-[#262626] hover:bg-[#333333] text-white text-xs font-medium transition"
              >
                {copied ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                <span>{copied ? 'Copied!' : 'Copy report text'}</span>
              </button>
              <button
                type="button"
                onClick={() => {
                  resetScreenshots();
                  setTitle('');
                  setDescription('');
                  setStep('compose');
                  void invoke('clear_bug_report_draft').catch(() => {});
                  setShowBugReport(false);
                }}
                className="px-4 py-1.5 rounded bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold transition"
              >
                Done
              </button>
            </div>
          </div>
        )}

        {/* Footer Actions */}
        {step === 'compose' && (
        <div className="flex items-center justify-between px-5 py-3 border-t border-[#333333] bg-[#1f1f1f]">
          <button
            type="button"
            onClick={handleClearDraft}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded hover:bg-[#262626] text-[#a3a3a3] hover:text-red-400 text-xs transition"
            title="Discard current draft"
          >
            <Trash2 className="w-3.5 h-3.5" />
            <span>Discard Draft</span>
          </button>

          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={handleCopyMarkdown}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-[#262626] hover:bg-[#333333] text-white text-xs font-medium transition"
              title="Copy sanitized markdown to clipboard"
            >
              {copied ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
              <span>{copied ? 'Copied!' : 'Copy to Clipboard'}</span>
            </button>

            <button
              type="button"
              onClick={handleSubmit}
              disabled={submitting || !title.trim() || !description.trim()}
              className="flex items-center gap-1.5 px-4 py-1.5 rounded bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold transition disabled:opacity-40 shadow-sm"
            >
              <ExternalLink className="w-3.5 h-3.5" />
              <span>{submitting ? 'Opening…' : 'Open Issue on GitHub'}</span>
            </button>
          </div>
        </div>
        )}
      </div>
    </div>
  );
};

export default BugReportModal;
