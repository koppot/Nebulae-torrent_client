import React, { useEffect, useState } from 'react';
import { useUiStore } from '../../stores/uiStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { ExternalLink, X } from 'lucide-react';

export const SplashScreen: React.FC = () => {
  const { setSplashDismissed } = useUiStore();
  const { settings } = useSettingsStore();
  const [progress, setProgress] = useState(25);

  useEffect(() => {
    if (!settings.enable_splash) {
      setSplashDismissed(true);
      return;
    }

    const t1 = setTimeout(() => setProgress(65), 500);
    const t2 = setTimeout(() => setProgress(100), 1200);
    const t3 = setTimeout(() => setSplashDismissed(true), 1700);

    return () => {
      clearTimeout(t1);
      clearTimeout(t2);
      clearTimeout(t3);
    };
  }, [settings.enable_splash, setSplashDismissed]);

  const handleOpenCommunity = (e: React.MouseEvent) => {
    e.stopPropagation();
    window.open(settings.splash_url || 'https://asiandvdclub.org', '_blank');
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-[#141414] text-white select-none overflow-hidden font-sans">
      {/* Top Skip Button */}
      <button
        onClick={() => setSplashDismissed(true)}
        className="absolute top-6 right-6 flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-xs text-[#a3a3a3] hover:text-white transition"
        title="Skip loading screen"
      >
        <span>Skip</span>
        <X className="w-3.5 h-3.5" />
      </button>

      {/* Central Content */}
      <div className="relative z-10 flex flex-col items-center max-w-sm w-full px-6 text-center space-y-6">
        {/* App Icon */}
        <div className="relative">
          <div className="w-24 h-24 rounded-2xl bg-[#1f1f1f] border border-[#333333] shadow-xl overflow-hidden p-3 flex items-center justify-center">
            <img
              src="/icon-master.svg"
              alt="Nebulae"
              className="w-full h-full object-contain"
            />
          </div>
        </div>

        {/* Brand Name & Subtitle */}
        <div>
          <h1 className="text-2xl font-bold tracking-tight text-white mb-1">
            Nebulae
          </h1>
          <p className="text-xs text-[#a3a3a3] font-mono">
            Torrent Client by Asian DVD Club
          </p>
        </div>

        {/* Community Link */}
        <div
          onClick={handleOpenCommunity}
          className="w-full p-3.5 rounded-lg bg-[#1f1f1f] border border-[#333333] hover:border-[#e50914] cursor-pointer transition text-left"
        >
          <p className="text-xs font-medium text-white mb-1">
            Consider joining our community
          </p>
          <div className="flex items-center gap-1 text-[11px] text-[#e50914] font-mono">
            <span>asiandvdclub.org</span>
            <ExternalLink className="w-3 h-3" />
          </div>
        </div>

        {/* Loading Progress */}
        <div className="w-full space-y-2">
          <div className="w-full h-1 rounded-full bg-[#1f1f1f] overflow-hidden">
            <div
              className="h-full bg-[#e50914] transition-all duration-300"
              style={{ width: `${progress}%` }}
            />
          </div>
        </div>
      </div>
    </div>
  );
};

export default SplashScreen;
