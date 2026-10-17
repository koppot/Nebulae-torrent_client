import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { RefreshCw, X } from 'lucide-react';
import brandLogo from '../../assets/brand-logo.png';

interface Props {
  onClose: () => void;
}

/// Owner request: clicking the little nebula icon opens a popup with
/// detailed information about this release (the app build) and an
/// Update button that runs the bundled updater.
export default function AppInfoModal({ onClose }: Props) {
  const [version, setVersion] = useState<string>('…');
  const [updateState, setUpdateState] = useState<'idle' | 'checking' | 'downloading' | 'ready' | 'uptodate' | 'error'>('idle');
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion('unknown'));
  }, []);

  const runUpdate = async () => {
    setUpdateState('checking');
    setNote(null);
    try {
      const { check } = await import('@tauri-apps/plugin-updater');
      const update = await check();
      if (!update) {
        setUpdateState('uptodate');
        return;
      }
      setUpdateState('downloading');
      await update.downloadAndInstall();
      setUpdateState('ready');
      try {
        const { relaunch } = await import('@tauri-apps/plugin-process');
        (window as unknown as { __nebulaeRelaunch?: () => void }).__nebulaeRelaunch = relaunch;
      } catch { /* process plugin missing */ }
    } catch (err) {
      setUpdateState('error');
      setNote(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <div className="fixed inset-0 z-[300] flex items-center justify-center bg-black/60" onClick={onClose}>
      <div
        className="w-[360px] rounded-xl bg-[#1c1c1c] border border-[#333333] shadow-2xl shadow-black/80 p-5"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between">
          <img src={brandLogo} alt="Nebulae" className="w-12 h-12 rounded-lg" draggable={false} />
          <button type="button" onClick={onClose} className="text-[#a3a3a3] hover:text-white transition" title="Close">
            <X className="w-4 h-4" />
          </button>
        </div>
        <h3 className="mt-3 text-base font-bold text-white">Nebulae</h3>
        <p className="text-[11px] text-[#a3a3a3]">Client by Asian DVD Club</p>
        <dl className="mt-4 space-y-1.5 text-[11px]">
          <div className="flex justify-between">
            <dt className="text-[#a3a3a3]">Version</dt>
            <dd className="font-mono text-white">{version}</dd>
          </div>
        </dl>
        <p className="mt-3 text-[10px] uppercase tracking-[0.15em] text-[#a3a3a3]">Features</p>
        <ul className="mt-1.5 space-y-1 text-[11px] text-[#d4d4d4]">
          <li className="flex gap-2"><span className="text-[#e50914]">•</span>Torrent manager with swarm detail, RSS and trackers</li>
          <li className="flex gap-2"><span className="text-[#e50914]">•</span>Media Center — optical cinema gallery with TMDB posters</li>
          <li className="flex gap-2"><span className="text-[#e50914]">•</span>Embedded VLC player with interactive DVD / Blu-ray menus</li>
          <li className="flex gap-2"><span className="text-[#e50914]">•</span>Chapter navigation, audio & subtitle language control</li>
          <li className="flex gap-2"><span className="text-[#e50914]">•</span>ISO and multi-disc releases, resume points, external media library</li>
          <li className="flex gap-2"><span className="text-[#e50914]">•</span>VPN kill switch, connectivity doctor, ADC ratio profile</li>
        </ul>

        <button
          type="button"
          onClick={runUpdate}
          disabled={updateState === 'checking' || updateState === 'downloading'}
          className="mt-4 w-full flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-[#e50914] hover:bg-[#f40612] disabled:opacity-60 text-white text-xs font-semibold transition"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${updateState === 'checking' || updateState === 'downloading' ? 'animate-spin' : ''}`} />
          {updateState === 'idle' && 'Check for Update'}
          {updateState === 'checking' && 'Checking…'}
          {updateState === 'downloading' && 'Downloading…'}
          {updateState === 'ready' && 'Update Installed — Restart'}
          {updateState === 'uptodate' && 'You Are Up to Date'}
          {updateState === 'error' && 'Retry Update'}
        </button>
        {updateState === 'ready' && (
          <button
            type="button"
            onClick={() => {
              const rel = (window as unknown as { __nebulaeRelaunch?: () => void }).__nebulaeRelaunch;
              if (rel) rel();
              else invoke('plugin:process|restart').catch(() => {});
            }}
            className="mt-2 w-full px-3 py-2 rounded-lg bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white text-xs font-medium transition"
          >
            Restart Now
          </button>
        )}
        {note && <p className="mt-2 text-[10px] text-[#e50914] break-words">{note}</p>}
      </div>
    </div>
  );
}
