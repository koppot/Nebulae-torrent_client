import React, { useEffect, useState } from 'react';
import { useUiStore } from '../../stores/uiStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { Sparkles, ExternalLink, X } from 'lucide-react';

export const SplashScreen: React.FC = () => {
  const { setSplashDismissed } = useUiStore();
  const { settings } = useSettingsStore();
  const [stepIndex, setStepIndex] = useState(0);
  const [progress, setProgress] = useState(15);

  const steps = [
    'Verifying isolated internal storage (ADC/internal)...',
    'Opening internal SQLite persistence (nebula.db)...',
    'Initializing local-first BitTorrent engine (rqbit)...',
    'Ready',
  ];

  useEffect(() => {
    // If user disabled splash in settings, dismiss immediately
    if (!settings.enable_splash) {
      setSplashDismissed(true);
      return;
    }

    const t1 = setTimeout(() => {
      setStepIndex(1);
      setProgress(45);
    }, 600);

    const t2 = setTimeout(() => {
      setStepIndex(2);
      setProgress(80);
    }, 1300);

    const t3 = setTimeout(() => {
      setStepIndex(3);
      setProgress(100);
    }, 2000);

    const t4 = setTimeout(() => {
      setSplashDismissed(true);
    }, 2600);

    return () => {
      clearTimeout(t1);
      clearTimeout(t2);
      clearTimeout(t3);
      clearTimeout(t4);
    };
  }, [settings.enable_splash, setSplashDismissed]);

  const handleOpenCommunity = (e: React.MouseEvent) => {
    e.stopPropagation();
    window.open(settings.splash_url || 'https://asiandvdclub.org', '_blank');
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950 text-white select-none overflow-hidden font-sans">
      {/* Cinematic Ambient Background Lighting */}
      <div className="absolute inset-0 pointer-events-none">
        <div className="absolute top-1/4 left-1/4 w-96 h-96 bg-pink-600/25 rounded-full blur-3xl animate-pulse"></div>
        <div className="absolute bottom-1/4 right-1/4 w-96 h-96 bg-cyan-600/20 rounded-full blur-3xl animate-pulse delay-1000"></div>
        <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-80 h-80 bg-amber-500/15 rounded-full blur-2xl"></div>
        {/* Subtle Film Grain / Perforation Lines Motif */}
        <div className="absolute inset-0 opacity-[0.03] bg-[radial-gradient(#fff_1px,transparent_1px)] [background-size:16px_16px]"></div>
      </div>

      {/* Top Skip Button */}
      <button
        onClick={() => setSplashDismissed(true)}
        className="absolute top-6 right-6 flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-white/10 hover:bg-white/20 text-xs text-slate-300 hover:text-white backdrop-blur transition-all"
        title="Skip loading screen"
      >
        <span>Skip</span>
        <X className="w-3.5 h-3.5" />
      </button>

      {/* Central Cinematic Badge & Content */}
      <div className="relative z-10 flex flex-col items-center max-w-md w-full px-8 text-center">
        {/* Multicolor Cinematic Monogram Mark */}
        <div className="relative mb-6 group">
          <div className="absolute -inset-2 rounded-2xl bg-gradient-to-r from-pink-500 via-amber-400 to-cyan-400 opacity-75 blur-md group-hover:opacity-100 transition duration-500"></div>
          <div className="relative flex items-center justify-center w-28 h-28 rounded-2xl bg-slate-900 border border-white/20 shadow-2xl overflow-hidden p-2">
            <img
              src="/src/assets/app-icon.png"
              alt="ADC Emblem"
              className="w-full h-full object-contain rounded-xl"
            />
          </div>
        </div>

        {/* Brand Name & Subtitle */}
        <h1 className="text-3xl font-bold tracking-tight text-adc-ivory mb-1 flex items-center gap-2">
          <span>Nebulae</span>
          <Sparkles className="w-4 h-4 text-adc-gold" />
        </h1>
        <p className="text-xs text-adc-ivory/80 mb-8 font-mono">
          Torrent Client by Asian DVD Club
        </p>

        {/* Community Callout (Clickable link with accessible contrast) */}
        <div
          onClick={handleOpenCommunity}
          className="w-full mb-8 p-4 rounded-xl bg-gradient-to-b from-white/10 to-white/5 border border-pink-500/30 hover:border-pink-500/60 backdrop-blur-md cursor-pointer transition-all hover:scale-[1.02] shadow-lg group"
        >
          <p className="text-sm font-medium text-slate-200 group-hover:text-white mb-1 whitespace-pre-line leading-relaxed">
            {settings.splash_message || 'Consider joining our community\nasiandvdclub.org'}
          </p>
          <div className="inline-flex items-center gap-1 text-xs font-semibold text-cyan-400 group-hover:text-cyan-300 group-hover:underline mt-1 font-mono">
            <span>Visit asiandvdclub.org</span>
            <ExternalLink className="w-3.5 h-3.5" />
          </div>
        </div>

        {/* Progress Bar & Subsystem Initialization Status */}
        <div className="w-full">
          <div className="flex justify-between items-center text-[11px] text-slate-400 mb-2 font-mono">
            <span className="truncate pr-2">{steps[stepIndex]}</span>
            <span className="font-semibold text-cyan-400">{progress}%</span>
          </div>
          <div className="w-full h-1.5 rounded-full bg-slate-800 overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-pink-500 via-amber-400 to-cyan-400 transition-all duration-300 rounded-full"
              style={{ width: `${progress}%` }}
            ></div>
          </div>
        </div>
      </div>
    </div>
  );
};
