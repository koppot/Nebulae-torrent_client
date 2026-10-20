import React, { useState } from 'react';
import { open } from '@tauri-apps/plugin-shell';
import { useUiStore } from '../../stores/uiStore';
import {
  X,
  HelpCircle,
  Monitor,
  Apple,
  Cpu,
  Download,
  AlertTriangle,
  LifeBuoy,
} from 'lucide-react';

export const HelpModal: React.FC = () => {
  const { showHelp, setShowHelp } = useUiStore();
  const [platformTab, setPlatformTab] = useState<'windows' | 'mac-arm' | 'mac-intel'>('mac-arm');

  if (!showHelp) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm select-none font-sans">
      <div className="relative w-full max-w-2xl bg-[#141414] border border-[#333333] rounded-xl shadow-2xl overflow-hidden flex flex-col max-h-[85vh] text-white">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-[#333333] bg-[#1f1f1f]">
          <div className="flex items-center gap-2">
            <HelpCircle className="w-4 h-4 text-[#e50914]" />
            <h2 className="text-sm font-bold text-white">User Guide & Installation</h2>
          </div>
          <button
            onClick={() => setShowHelp(false)}
            className="p-1 rounded text-[#a3a3a3] hover:text-white transition"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Platform Selection Tabs */}
        <div className="flex border-b border-[#333333] bg-[#1f1f1f] px-5 pt-2 gap-2 text-xs">
          <button
            onClick={() => setPlatformTab('mac-arm')}
            className={`flex items-center gap-1.5 px-3 py-2 rounded-t font-medium transition ${
              platformTab === 'mac-arm'
                ? 'border-b-2 border-[#e50914] text-white bg-[#141414]'
                : 'text-[#a3a3a3] hover:text-white'
            }`}
          >
            <Apple className="w-3.5 h-3.5" />
            <span>macOS Apple Silicon</span>
          </button>
          <button
            onClick={() => setPlatformTab('mac-intel')}
            className={`flex items-center gap-1.5 px-3 py-2 rounded-t font-medium transition ${
              platformTab === 'mac-intel'
                ? 'border-b-2 border-[#e50914] text-white bg-[#141414]'
                : 'text-[#a3a3a3] hover:text-white'
            }`}
          >
            <Cpu className="w-3.5 h-3.5" />
            <span>macOS Intel</span>
          </button>
          <button
            onClick={() => setPlatformTab('windows')}
            className={`flex items-center gap-1.5 px-3 py-2 rounded-t font-medium transition ${
              platformTab === 'windows'
                ? 'border-b-2 border-[#e50914] text-white bg-[#141414]'
                : 'text-[#a3a3a3] hover:text-white'
            }`}
          >
            <Monitor className="w-3.5 h-3.5" />
            <span>Windows x64</span>
          </button>
        </div>

        {/* Content Pane */}
        <div className="flex-1 p-5 overflow-y-auto space-y-4 text-xs text-[#a3a3a3] leading-relaxed">
          {/* macOS Apple Silicon Guide */}
          {platformTab === 'mac-arm' && (
            <div className="space-y-4">
              <section className="bg-[#1f1f1f] p-3.5 rounded-lg border border-[#333333]">
                <h3 className="font-semibold text-white text-xs mb-1 flex items-center gap-1.5">
                  <Apple className="w-3.5 h-3.5 text-[#e50914]" />
                  <span>1. Apple Silicon Macs (M1/M2/M3/M4/M5)</span>
                </h3>
                <p>
                  Download: <code className="text-white font-mono">Nebulae-macOS-arm64.dmg</code>.
                </p>
              </section>

              <section className="space-y-2">
                <h3 className="font-semibold text-white text-xs flex items-center gap-1.5">
                  <Download className="w-3.5 h-3.5 text-[#e50914]" />
                  <span>2. Installation</span>
                </h3>
                <ol className="list-decimal list-inside space-y-1">
                  <li>Double-click <code className="text-white font-mono">Nebulae-macOS-arm64.dmg</code>.</li>
                  <li>Drag <strong>Nebulae.app</strong> into <strong>Applications</strong>.</li>
                  <li>Launch Nebulae from Applications.</li>
                </ol>
              </section>

              <section className="p-3 bg-[#1f1f1f] border border-[#333333] rounded-lg space-y-1">
                <div className="flex items-center gap-1.5 font-semibold text-white">
                  <AlertTriangle className="w-3.5 h-3.5 text-[#e50914]" />
                  <span>macOS Gatekeeper Verification</span>
                </div>
                <p>
                  If macOS prevents opening because the app is unsigned: Right-click <strong>Nebulae.app</strong> in Applications and choose <strong>Open</strong>, then click <strong>Open</strong>.
                </p>
                <p className="font-mono text-[11px] text-[#a3a3a3] mt-1">
                  Terminal command: <code className="text-white select-all">xattr -cr /Applications/Nebulae.app</code>
                </p>
              </section>
            </div>
          )}

          {/* macOS Intel Guide */}
          {platformTab === 'mac-intel' && (
            <div className="space-y-4">
              <section className="bg-[#1f1f1f] p-3.5 rounded-lg border border-[#333333]">
                <h3 className="font-semibold text-white text-xs mb-1 flex items-center gap-1.5">
                  <Cpu className="w-3.5 h-3.5 text-[#e50914]" />
                  <span>1. Intel Macs (x86_64)</span>
                </h3>
                <p>
                  Download: <code className="text-white font-mono">Nebulae-macOS-x64.dmg</code>.
                </p>
              </section>

              <section className="space-y-2">
                <h3 className="font-semibold text-white text-xs flex items-center gap-1.5">
                  <Download className="w-3.5 h-3.5 text-[#e50914]" />
                  <span>2. Installation</span>
                </h3>
                <ol className="list-decimal list-inside space-y-1">
                  <li>Double-click <code className="text-white font-mono">Nebulae-macOS-x64.dmg</code>.</li>
                  <li>Drag <strong>Nebulae.app</strong> into <strong>Applications</strong>.</li>
                  <li>Launch Nebulae.</li>
                </ol>
              </section>
            </div>
          )}

          {/* Windows Guide */}
          {platformTab === 'windows' && (
            <div className="space-y-4">
              <section className="bg-[#1f1f1f] p-3.5 rounded-lg border border-[#333333]">
                <h3 className="font-semibold text-white text-xs mb-1 flex items-center gap-1.5">
                  <Monitor className="w-3.5 h-3.5 text-[#e50914]" />
                  <span>1. Windows 10 & 11 (64-bit)</span>
                </h3>
                <p>
                  Download installer: <code className="text-white font-mono">Nebulae-Setup-x64.exe</code>.
                </p>
              </section>

              <section className="space-y-2">
                <h3 className="font-semibold text-white text-xs flex items-center gap-1.5">
                  <Download className="w-3.5 h-3.5 text-[#e50914]" />
                  <span>2. Installation</span>
                </h3>
                <ol className="list-decimal list-inside space-y-1">
                  <li>Run <code className="text-white font-mono">Nebulae-Setup-x64.exe</code> and follow prompts.</li>
                  <li>If Windows SmartScreen prompts "Windows protected your PC", click <strong>More info</strong> → <strong>Run anyway</strong>.</li>
                  <li>On first launch, allow access on Private networks when prompted by Windows Firewall.</li>
                </ol>
              </section>
            </div>
          )}

          {/* Discord Support (owner 2026-10-20): invite verified against the
              ADC Discord - it lands in the #nebulae-support forum
              (channel 1561910667357724824) and never expires. */}
          <section className="bg-[#1f1f1f] p-3 rounded-lg border border-[#333333]">
            <h3 className="font-semibold text-white text-xs flex items-center gap-1.5 mb-1">
              <LifeBuoy className="w-3.5 h-3.5 text-[#e50914]" />
              <span>Discord Support</span>
            </h3>
            <p className="text-[11px] text-[#a3a3a3]">
              Live help from staff and members in the Asian DVD Club Discord,
              <span className="text-white"> #nebulae-support</span> forum.
            </p>
            <button
              onClick={() => {
                open('https://discord.gg/pcHMJTh8G2').catch((e) =>
                  console.error('[help] failed to open Discord invite:', e)
                );
              }}
              className="mt-2 px-3 py-1.5 rounded bg-[#e50914] hover:bg-[#f40612] text-white text-[11px] font-semibold transition"
            >
              Open Discord Support
            </button>
          </section>

          {/* Common Storage Note */}
          <section className="bg-[#1f1f1f] p-3 rounded-lg border border-[#333333]">
            <h3 className="font-semibold text-white text-xs flex items-center gap-1.5 mb-1">
              <LifeBuoy className="w-3.5 h-3.5 text-[#e50914]" />
              <span>Internal Storage Isolation</span>
            </h3>
            <p className="text-[11px] text-[#a3a3a3]">
              Application data, state, databases, and logs are isolated strictly within:
              <br />
              <span className="font-mono text-white">~/Library/Application Support/Nebulae/internal/</span> (macOS)
              <br />
              <span className="font-mono text-white">%LOCALAPPDATA%\Nebulae\internal\</span> (Windows)
            </p>
          </section>
        </div>

        {/* Footer */}
        <div className="flex justify-end px-5 py-3 border-t border-[#333333] bg-[#1f1f1f]">
          <button
            onClick={() => setShowHelp(false)}
            className="px-4 py-1.5 rounded bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold transition"
          >
            Close Guide
          </button>
        </div>
      </div>
    </div>
  );
};

export default HelpModal;
