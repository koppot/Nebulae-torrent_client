import React, { useState } from 'react';
import { useUiStore } from '../../stores/uiStore';
import {
  X,
  HelpCircle,
  Monitor,
  Apple,
  Cpu,
  ShieldCheck,
  Download,
  AlertTriangle,
  FolderCheck,
  Flame,
  LifeBuoy,
} from 'lucide-react';

export const HelpModal: React.FC = () => {
  const { showHelp, setShowHelp } = useUiStore();
  const [platformTab, setPlatformTab] = useState<'windows' | 'mac-arm' | 'mac-intel'>('windows');

  if (!showHelp) return null;

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
      <div className="relative w-full max-w-3xl bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl overflow-hidden flex flex-col max-h-[88vh]">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-slate-800 bg-slate-950/40">
          <div className="flex items-center gap-2">
            <HelpCircle className="w-5 h-5 text-pink-400" />
            <h2 className="text-base font-semibold text-white">Installation & User Guide — ADC</h2>
          </div>
          <button
            onClick={() => setShowHelp(false)}
            className="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Platform Selection Tabs */}
        <div className="flex border-b border-slate-800 bg-slate-950/80 px-6 pt-3 gap-2">
          <button
            onClick={() => setPlatformTab('windows')}
            className={`flex items-center gap-2 px-4 py-2 text-xs font-semibold rounded-t-lg transition border-b-2 ${
              platformTab === 'windows'
                ? 'border-pink-500 text-pink-400 bg-slate-900'
                : 'border-transparent text-slate-400 hover:text-slate-200'
            }`}
          >
            <Monitor className="w-4 h-4 text-cyan-400" />
            <span>Windows x64 (NSIS)</span>
          </button>
          <button
            onClick={() => setPlatformTab('mac-arm')}
            className={`flex items-center gap-2 px-4 py-2 text-xs font-semibold rounded-t-lg transition border-b-2 ${
              platformTab === 'mac-arm'
                ? 'border-pink-500 text-pink-400 bg-slate-900'
                : 'border-transparent text-slate-400 hover:text-slate-200'
            }`}
          >
            <Apple className="w-4 h-4 text-amber-400" />
            <span>macOS Apple Silicon (M1–M4)</span>
          </button>
          <button
            onClick={() => setPlatformTab('mac-intel')}
            className={`flex items-center gap-2 px-4 py-2 text-xs font-semibold rounded-t-lg transition border-b-2 ${
              platformTab === 'mac-intel'
                ? 'border-pink-500 text-pink-400 bg-slate-900'
                : 'border-transparent text-slate-400 hover:text-slate-200'
            }`}
          >
            <Cpu className="w-4 h-4 text-slate-400" />
            <span>macOS Intel (x86_64)</span>
          </button>
        </div>

        {/* Content Pane */}
        <div className="flex-1 p-6 overflow-y-auto space-y-6 text-xs text-slate-300 leading-relaxed">
          {/* Windows x64 Guide */}
          {platformTab === 'windows' && (
            <div className="space-y-5">
              <section className="bg-slate-950 p-4 rounded-xl border border-slate-800">
                <h3 className="font-semibold text-white text-sm mb-2 flex items-center gap-2">
                  <ShieldCheck className="w-4 h-4 text-cyan-400" />
                  1. System Requirements & Package Name
                </h3>
                <ul className="list-disc list-inside space-y-1 text-slate-400">
                  <li><strong>Operating System:</strong> Windows 10 or Windows 11 (64-bit only).</li>
                  <li><strong>Hardware:</strong> 2 GB RAM minimum (4 GB recommended), 100 MB disk space for app.</li>
                  <li><strong>Installer Artifact:</strong> <code className="text-pink-300">nebular_torrent-Setup-x64.exe</code> (produces installed binary <code className="text-pink-300">nebular_torrent.exe</code>).</li>
                </ul>
              </section>

              <section className="space-y-2">
                <h3 className="font-semibold text-white text-sm flex items-center gap-2">
                  <Download className="w-4 h-4 text-pink-400" />
                  2. Download & SmartScreen Notice
                </h3>
                <p>
                  Download <code className="text-cyan-300">nebular_torrent-Setup-x64.exe</code> exclusively from our official GitHub Releases page.
                </p>
                <div className="p-3 bg-amber-950/30 border border-amber-600/30 rounded-lg text-amber-200">
                  <div className="flex items-center gap-2 font-semibold mb-1">
                    <AlertTriangle className="w-4 h-4 text-amber-400" />
                    <span>Windows SmartScreen Warning Explanation</span>
                  </div>
                  <p>
                    Because community open-source releases do not bundle proprietary commercial EV certificates, Windows SmartScreen may display: <em>"Windows protected your PC"</em>.
                  </p>
                  <p className="mt-1">
                    To proceed safely: click <strong>"More info"</strong>, verify the publisher matches open-source checksums, and click <strong>"Run anyway"</strong>.
                  </p>
                </div>
              </section>

              <section className="space-y-2">
                <h3 className="font-semibold text-white text-sm flex items-center gap-2">
                  <FolderCheck className="w-4 h-4 text-emerald-400" />
                  3. Installer Walkthrough & Firewall Configuration
                </h3>
                <ol className="list-decimal list-inside space-y-1.5 text-slate-300">
                  <li>Run <code className="text-pink-300">nebular_torrent-Setup-x64.exe</code> and follow the standard NSIS setup wizard.</li>
                  <li>Select your destination folder and desktop shortcut preference.</li>
                  <li>Check "Launch ADC" and click Finish.</li>
                  <li>
                    <strong>Windows Defender Firewall Prompt:</strong> On first launch, Windows will ask to allow ADC network access. Click <strong>"Allow access"</strong> for Private networks. This permission is necessary for peer-to-peer BitTorrent piece exchange over TCP/UDP.
                  </li>
                </ol>
              </section>

              <section className="space-y-2">
                <h3 className="font-semibold text-white text-sm flex items-center gap-2">
                  <Flame className="w-4 h-4 text-pink-400" />
                  4. Adding Torrents & Startup Splash
                </h3>
                <p>
                  On launch, the cinematic <strong>ADC Splash Screen</strong> displays internal initialization status and an invitation to join our community (<code className="text-cyan-300">asiandvdclub.org</code>). You can skip it at any time or disable it under Settings.
                </p>
                <p>
                  Add torrents by clicking <strong>+ Add Torrent</strong>, dragging any <code className="text-slate-200">.torrent</code> file into the window, or pasting a magnet URI (<code className="text-slate-200">adc://</code> or <code className="text-slate-200">magnet:?xt=...</code>).
                </p>
              </section>

              <section className="space-y-2 bg-slate-950 p-4 rounded-xl border border-slate-800">
                <h3 className="font-semibold text-white text-sm flex items-center gap-2">
                  <LifeBuoy className="w-4 h-4 text-amber-400" />
                  5. Windows Troubleshooting & Technical File Location
                </h3>
                <ul className="space-y-1.5 text-slate-400">
                  <li><strong>App does not start:</strong> Ensure the Microsoft Visual C++ 2015–2022 Redistributable (x64) is installed if on a fresh Windows 10 installation.</li>
                  <li><strong>Internal Data & Logs:</strong> Technical files, SQLite database, and sanitized logs are stored in <code className="text-pink-300">%APPDATA%\ADC\internal\</code> and isolated from the main interface.</li>
                  <li><strong>Uninstalling:</strong> Windows Settings → Apps → Installed apps → Find <strong>ADC</strong> → Uninstall.</li>
                </ul>
              </section>
            </div>
          )}

          {/* macOS Apple Silicon Guide */}
          {platformTab === 'mac-arm' && (
            <div className="space-y-5">
              <section className="bg-slate-950 p-4 rounded-xl border border-slate-800">
                <h3 className="font-semibold text-white text-sm mb-2 flex items-center gap-2">
                  <Apple className="w-4 h-4 text-amber-400" />
                  1. Verify Apple Silicon Mac
                </h3>
                <p className="text-slate-400">
                  Click the <strong>Apple menu ()</strong> → <strong>About This Mac</strong>. Confirm the <strong>Chip</strong> line reads <strong>Apple M1, M2, M3, M4</strong> (or Pro/Max/Ultra).
                </p>
                <p className="text-slate-400 mt-1">
                  Download package: <code className="text-pink-300">nebular_torrent-macOS-arm64.dmg</code> (native ARM64 binary).
                </p>
              </section>

              <section className="space-y-2">
                <h3 className="font-semibold text-white text-sm flex items-center gap-2">
                  <Download className="w-4 h-4 text-pink-400" />
                  2. Installation Steps
                </h3>
                <ol className="list-decimal list-inside space-y-1.5 text-slate-300">
                  <li>Double-click <code className="text-pink-300">nebular_torrent-macOS-arm64.dmg</code> to mount the disk image.</li>
                  <li>Drag the <strong>nebular_torrent.app</strong> icon into your <strong>Applications</strong> shortcut.</li>
                  <li>Eject the mounted DMG in Finder.</li>
                  <li>Launch <strong>nebular_torrent</strong> from Applications or Launchpad.</li>
                </ol>
              </section>

              <section className="p-3 bg-amber-950/30 border border-amber-600/30 rounded-lg text-amber-200 space-y-1.5">
                <div className="flex items-center gap-2 font-semibold">
                  <AlertTriangle className="w-4 h-4 text-amber-400" />
                  <span>macOS Gatekeeper Verification</span>
                </div>
                <p>
                  If launching an unsigned community release, macOS may display: <em>"nebular_torrent cannot be opened because Apple cannot check it for malicious software"</em>.
                </p>
                <p>
                  To open: Right-click (or Control-click) <strong>nebular_torrent.app</strong> in Applications and choose <strong>Open</strong>, then click <strong>Open</strong> in the dialog. Alternatively, open <strong>System Settings → Privacy & Security</strong> and click <strong>Open Anyway</strong>.
                </p>
              </section>

              <section className="space-y-2 bg-slate-950 p-4 rounded-xl border border-slate-800">
                <h3 className="font-semibold text-white text-sm flex items-center gap-2">
                  <LifeBuoy className="w-4 h-4 text-cyan-400" />
                  3. Troubleshooting & Internal Files
                </h3>
                <ul className="space-y-1.5 text-slate-400">
                  <li><strong>"nebular_torrent is damaged" quarantine prompt:</strong> Open Terminal and execute: <code className="text-pink-300 select-all">xattr -cr /Applications/nebular_torrent.app</code></li>
                  <li><strong>Internal Files:</strong> Stored in <code className="text-pink-300">~/Library/Application Support/ADC/internal/</code>.</li>
                  <li><strong>Uninstall:</strong> Drag <code className="text-slate-200">nebular_torrent.app</code> to Trash and optionally delete <code className="text-slate-200">~/Library/Application Support/ADC/</code>.</li>
                </ul>
              </section>
            </div>
          )}

          {/* macOS Intel Guide */}
          {platformTab === 'mac-intel' && (
            <div className="space-y-5">
              <section className="bg-slate-950 p-4 rounded-xl border border-slate-800">
                <h3 className="font-semibold text-white text-sm mb-2 flex items-center gap-2">
                  <Cpu className="w-4 h-4 text-slate-400" />
                  1. Verify Intel Mac
                </h3>
                <p className="text-slate-400">
                  Click the <strong>Apple menu ()</strong> → <strong>About This Mac</strong>. Confirm the <strong>Processor</strong> line reads <strong>Intel Core i5, i7, i9, or Xeon</strong>.
                </p>
                <p className="text-slate-400 mt-1">
                  Download package: <code className="text-pink-300">nebular_torrent-macOS-x64.dmg</code> (native x86_64 binary).
                </p>
              </section>

              <section className="space-y-2">
                <h3 className="font-semibold text-white text-sm flex items-center gap-2">
                  <Download className="w-4 h-4 text-pink-400" />
                  2. Installation Steps
                </h3>
                <ol className="list-decimal list-inside space-y-1.5 text-slate-300">
                  <li>Double-click <code className="text-pink-300">nebular_torrent-macOS-x64.dmg</code> to mount.</li>
                  <li>Drag <strong>nebular_torrent.app</strong> to Applications.</li>
                  <li>Eject the disk image and launch nebular_torrent.</li>
                </ol>
              </section>

              <section className="p-3 bg-amber-950/30 border border-amber-600/30 rounded-lg text-amber-200 space-y-1.5">
                <div className="flex items-center gap-2 font-semibold">
                  <AlertTriangle className="w-4 h-4 text-amber-400" />
                  <span>macOS Gatekeeper Verification</span>
                </div>
                <p>
                  Right-click <strong>nebular_torrent.app</strong> → select <strong>Open</strong> → confirm <strong>Open</strong>.
                </p>
              </section>

              <section className="space-y-2 bg-slate-950 p-4 rounded-xl border border-slate-800">
                <h3 className="font-semibold text-white text-sm flex items-center gap-2">
                  <LifeBuoy className="w-4 h-4 text-cyan-400" />
                  3. Troubleshooting & Internal Files
                </h3>
                <ul className="space-y-1.5 text-slate-400">
                  <li><strong>Quarantine removal:</strong> <code className="text-pink-300 select-all">xattr -cr /Applications/nebular_torrent.app</code></li>
                  <li><strong>Internal Files:</strong> Stored in <code className="text-pink-300">~/Library/Application Support/ADC/internal/</code>.</li>
                </ul>
              </section>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex justify-end px-6 py-3 border-t border-slate-800 bg-slate-950/40">
          <button
            onClick={() => setShowHelp(false)}
            className="px-4 py-1.5 rounded-lg bg-pink-600 hover:bg-pink-500 text-white text-xs font-semibold transition"
          >
            Close Guide
          </button>
        </div>
      </div>
    </div>
  );
};
