import React from 'react';
import { useTorrentStore } from '../../stores/torrentStore';
import { useUiStore } from '../../stores/uiStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { formatBytes, formatSpeed, formatEta } from '../../lib/formatters';
import {
  Play,
  Pause,
  ArrowDown,
  ArrowUp,
  LayoutGrid,
  Film,
  Minimize2,
} from 'lucide-react';

export const CompactBarView: React.FC = () => {
  const { torrents, pauseAll, resumeAll, pauseTorrent, resumeTorrent } = useTorrentStore();
  const { setViewMode } = useUiStore();
  const { settings } = useSettingsStore();

  const totalDownSpeed = torrents.reduce((acc, t) => acc + (t.download_rate || 0), 0);
  const totalUpSpeed = torrents.reduce((acc, t) => acc + (t.upload_rate || 0), 0);
  const activeCount = torrents.filter((t) => t.status === 'Downloading' || t.status === 'Seeding').length;
  const errorCount = torrents.filter((t) => t.status === 'Error').length;

  const handleMinimizeToTray = async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const win = getCurrentWindow();
      if (settings.minimize_to_tray) {
        await win.hide();
      } else {
        await win.minimize();
      }
    } catch {
      // Browser preview fallback
    }
  };

  return (
    <div className="flex-1 flex flex-col bg-adc-dark text-white select-none p-4 overflow-hidden">
      {/* Top Aggregated Controls Bar */}
      <div className="flex items-center justify-between p-3 rounded-xl bg-adc-cardDark border border-adc-cardBorderDark shadow-md">
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2">
            <span className="w-2.5 h-2.5 rounded-full bg-adc-emerald animate-pulse" />
            <span className="text-xs font-bold text-white tracking-wide uppercase">Nebulae Mini-Bar</span>
          </div>

          <div className="h-4 w-[1px] bg-adc-cardBorderDark" />

          {/* Aggregated Speed Gauges */}
          <div className="flex items-center gap-4 font-mono text-xs">
            <div className="flex items-center gap-1.5 text-adc-cyan">
              <ArrowDown className="w-3.5 h-3.5" />
              <span className="font-semibold">{formatSpeed(totalDownSpeed)}</span>
            </div>
            <div className="flex items-center gap-1.5 text-adc-emerald">
              <ArrowUp className="w-3.5 h-3.5" />
              <span className="font-semibold">{formatSpeed(totalUpSpeed)}</span>
            </div>
            <div className="text-slate-400 text-[11px]">
              <span>{activeCount} active transfers</span>
              {errorCount > 0 && (
                <span
                  className="ml-2 px-1.5 py-0.5 rounded bg-red-500/20 text-red-400 border border-red-500/40 font-semibold"
                  title="One or more transfers are in an error state — expand to Manager mode and inspect them"
                >
                  {errorCount} error{errorCount === 1 ? '' : 's'}
                </span>
              )}
            </div>
          </div>
        </div>

        {/* Global Controls & Mode Switcher */}
        <div className="flex items-center gap-2">
          <button
            onClick={() => resumeAll()}
            className="flex items-center gap-1 px-2.5 py-1 rounded-lg bg-adc-emerald/20 hover:bg-adc-emerald/30 text-adc-emerald border border-adc-emerald/30 text-xs font-semibold transition"
            title="Resume All Active Transfers"
          >
            <Play className="w-3 h-3 fill-current" />
            <span>Resume All</span>
          </button>
          <button
            onClick={() => pauseAll()}
            className="flex items-center gap-1 px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 border border-adc-cardBorderDark text-xs font-semibold transition"
            title="Pause All Transfers"
          >
            <Pause className="w-3 h-3 fill-current" />
            <span>Pause All</span>
          </button>

          <div className="h-4 w-[1px] bg-adc-cardBorderDark mx-1" />

          <button
            onClick={handleMinimizeToTray}
            className="p-1.5 rounded-lg hover:bg-slate-800 text-slate-400 hover:text-white transition"
            title="Minimize to System Tray"
          >
            <Minimize2 className="w-4 h-4" />
          </button>
          <button
            onClick={() => setViewMode('manager')}
            className="p-1.5 rounded-lg hover:bg-slate-800 text-slate-400 hover:text-white transition"
            title="Restore Full Manager Mode"
          >
            <LayoutGrid className="w-4 h-4" />
          </button>
          <button
            onClick={() => setViewMode('media')}
            className="p-1.5 rounded-lg hover:bg-slate-800 text-slate-400 hover:text-white transition"
            title="Switch to Media View"
          >
            <Film className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Mini Active Transfer List */}
      <div className="flex-1 overflow-y-auto mt-3 space-y-2 pr-1">
        {torrents.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-48 text-slate-500 text-xs">
            <span>No torrents active</span>
          </div>
        ) : (
          torrents.map((t) => {
            const progressPct = Math.round(t.progress * 100);
            const isDownloading = t.status === 'Downloading';

            return (
              <div
                key={t.id}
                className="flex items-center justify-between gap-3 p-2.5 rounded-lg bg-adc-cardDark/70 hover:bg-adc-cardDark border border-adc-cardBorderDark text-xs transition"
              >
                <div className="flex-1 min-w-0 space-y-1">
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-semibold text-white truncate text-xs" title={t.name}>
                      {t.name}
                    </span>
                    <span
                      className={`font-mono text-[10px] px-1.5 py-0.5 rounded bg-adc-dark border border-adc-cardBorderDark shrink-0 ${
                        t.status === 'Error' ? 'text-red-400 border-red-500/40 bg-red-500/10' : ''
                      }`}
                    >
                      {t.status}
                    </span>
                  </div>

                  {/* Progress Line */}
                  <div className="w-full h-1.5 bg-adc-dark rounded-full overflow-hidden">
                    <div
                      className={`h-full transition-all duration-300 ${
                        t.status === 'Seeding' ? 'bg-adc-emerald' : 'bg-adc-accent'
                      }`}
                      style={{ width: `${progressPct}%` }}
                    />
                  </div>

                  <div className="flex items-center justify-between text-[10px] font-mono text-slate-400">
                    <span>
                      {t.status === 'Checking' || t.status === 'Queued for checking'
                        ? t.status === 'Queued for checking'
                          ? 'Queued for checking'
                          : `Verifying… ${progressPct}%`
                        : t.status === 'Seeding' || t.status === 'Completed' || t.progress >= 1
                          ? `${formatBytes(t.uploaded_bytes)} uploaded`
                          : `${formatBytes(t.downloaded_bytes)} of ${formatBytes(t.total_bytes)} (${progressPct}%)`}
                    </span>
                    <div className="flex items-center gap-2">
                      {isDownloading && (
                        <span className="text-adc-cyan">{formatSpeed(t.download_rate)}</span>
                      )}
                      {t.status === 'Seeding' && (
                        <span className="text-adc-emerald">{formatSpeed(t.upload_rate)}</span>
                      )}
                      {t.eta_seconds != null && (
                        <span>ETA: {formatEta(t.eta_seconds)}</span>
                      )}
                    </div>
                  </div>
                </div>

                {/* Per-item Pause/Resume */}
                <button
                  onClick={() => (isDownloading ? pauseTorrent(t.id) : resumeTorrent(t.id))}
                  className="p-2 rounded-lg bg-adc-dark hover:bg-adc-cardBorderDark text-slate-300 hover:text-white transition shrink-0"
                  title={isDownloading ? 'Pause' : 'Resume'}
                >
                  {isDownloading ? <Pause className="w-3.5 h-3.5 fill-current" /> : <Play className="w-3.5 h-3.5 fill-current" />}
                </button>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};

export default CompactBarView;
