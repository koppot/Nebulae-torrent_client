import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { relaunch } from '@tauri-apps/plugin-process';
import { DiagnosticSummary } from '../../types';
import { AlertTriangle, RefreshCw, Copy, Check, Power } from 'lucide-react';

interface RecoveryScreenProps {
  error?: string | null;
}

export const RecoveryScreen: React.FC<RecoveryScreenProps> = ({ error: errorProp }) => {
  const [isRestarting, setIsRestarting] = useState(false);
  const [copiedReport, setCopiedReport] = useState(false);
  const [engineError, setEngineError] = useState<string | null>(errorProp ?? null);
  const [showTechnical, setShowTechnical] = useState(false);

  // Surface the real underlying failure instead of only the generic copy.
  useEffect(() => {
    invoke<string | null>('get_engine_init_error')
      .then((err) => {
        if (err) setEngineError(err);
      })
      .catch(() => {});
  }, []);

  const isPortConflict =
    !!engineError &&
    /(address already in use|addrinuse|os error (48|98|10048))/i.test(engineError);

  const handleRestart = async () => {
    setIsRestarting(true);
    try {
      await relaunch();
    } catch (err) {
      console.error('Failed to restart automatically:', err);
      setIsRestarting(false);
    }
  };

  const handleCopyReport = async () => {
    try {
      const summary = await invoke<DiagnosticSummary>('get_diagnostic_summary');
      const logs = await invoke<string>('export_logs');
      const text = [
        '=== Nebulae Diagnostic Report ===',
        `App: ${summary.app_name} v${summary.version}`,
        `OS: ${summary.os} (${summary.arch})`,
        `Engine Status: ${summary.engine_status}`,
        `Storage Status: ${summary.internal_storage_status}`,
        `Database: ${summary.database_status}`,
        '',
        '=== Session Logs ===',
        logs,
      ].join('\n');

      await navigator.clipboard.writeText(text);
      setCopiedReport(true);
      setTimeout(() => setCopiedReport(false), 2000);
    } catch {
      // Fallback: copy minimal info
      await navigator.clipboard.writeText('Nebulae Engine Initialization Error');
      setCopiedReport(true);
      setTimeout(() => setCopiedReport(false), 2000);
    }
  };

  const handleQuit = async () => {
    try {
      await invoke('quit_app');
    } catch {
      window.close();
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-[#141414] text-white p-6 font-sans select-none">
      <div className="relative w-full max-w-lg bg-[#1f1f1f] border border-[#333333] rounded-2xl p-8 shadow-2xl flex flex-col items-center text-center">
        {/* Brand Icon */}
        <div className="w-16 h-16 rounded-full bg-[#e50914]/20 border border-[#e50914]/40 flex items-center justify-center text-[#e50914] mb-6">
          <AlertTriangle className="w-8 h-8" />
        </div>

        {/* Title */}
        <h1 className="text-xl font-bold tracking-tight text-white mb-2">
          Engine Initialization Required
        </h1>

        {/* Product Attribution */}
        <p className="text-xs uppercase tracking-widest text-[#a3a3a3] font-semibold mb-6">
          Nebulae — Torrent Client by Asian DVD Club
        </p>

        {/* Friendly explanation */}
        <div className="p-4 rounded-xl bg-[#141414] border border-[#2e2e2e] text-xs text-[#a3a3a3] leading-relaxed mb-6 text-left space-y-2">
          {isPortConflict ? (
            <>
              <p>
                The engine could not start because its listening port (51413) is already
                in use. This is almost always another Nebulae instance still running in
                the menu bar / system tray &mdash; closing the window does not quit
                Nebulae. It can also be another torrent client on the same port.
              </p>
              <p>
                Quit the other Nebulae instance (tray icon &rarr; Quit Nebulae) or the
                other torrent client, or pick a different port in Settings, then restart
                Nebulae.
              </p>
            </>
          ) : (
            <>
              <p>
                The BitTorrent transfer engine could not be initialized on your system. This typically
                occurs if port 51413 is currently in use by another torrent client, or if file
                permissions prevent access to the configured download directory.
              </p>
              <p>
                Please verify that other BitTorrent applications are closed, or restart Nebulae to
                allow the engine to re-bind to its network interface.
              </p>
            </>
          )}
        </div>

        {/* Real underlying error */}
        {engineError && (
          <div className="mb-6 text-left">
            <button
              type="button"
              onClick={() => setShowTechnical(!showTechnical)}
              className="text-[11px] text-[#737373] hover:text-[#a3a3a3] transition"
            >
              {showTechnical ? 'Hide technical details' : 'Show technical details'}
            </button>
            {showTechnical && (
              <pre className="mt-2 p-3 rounded-lg bg-[#141414] border border-[#2e2e2e] text-[10px] leading-relaxed text-[#c8a2a2] whitespace-pre-wrap break-words max-h-32 overflow-y-auto font-mono">
                {engineError}
              </pre>
            )}
          </div>
        )}

        {/* Primary and Secondary Actions */}
        <div className="flex flex-col sm:flex-row items-center justify-center gap-3 w-full">
          <button
            onClick={handleRestart}
            disabled={isRestarting}
            className="w-full sm:w-auto flex-1 flex items-center justify-center gap-2 px-5 py-2.5 rounded-lg bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold shadow transition disabled:opacity-50"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${isRestarting ? 'animate-spin' : ''}`} />
            <span>{isRestarting ? 'Restarting...' : 'Restart Nebulae'}</span>
          </button>

          <button
            onClick={handleCopyReport}
            className="w-full sm:w-auto flex-1 flex items-center justify-center gap-2 px-4 py-2.5 rounded-lg border border-[#333333] bg-[#141414] hover:bg-[#262626] text-white text-xs font-semibold transition"
          >
            {copiedReport ? (
              <>
                <Check className="w-3.5 h-3.5 text-green-400" />
                <span>Copied!</span>
              </>
            ) : (
              <>
                <Copy className="w-3.5 h-3.5 text-[#a3a3a3]" />
                <span>Copy Diagnostic Report</span>
              </>
            )}
          </button>
        </div>

        {/* Quit link */}
        <button
          onClick={handleQuit}
          className="mt-6 flex items-center gap-1.5 text-xs text-[#737373] hover:text-[#a3a3a3] transition"
        >
          <Power className="w-3 h-3" />
          <span>Quit Application</span>
        </button>
      </div>
    </div>
  );
};
