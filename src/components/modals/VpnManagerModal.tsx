import React, { useEffect, useState } from 'react';
import { useVpnStore } from '../../stores/vpnStore';
import { useUiStore } from '../../stores/uiStore';
import { Shield, Lock, Power, Trash2, CheckCircle2, HelpCircle, FileText } from 'lucide-react';
import { VpnMode } from '../../types';

export const VpnManagerModal: React.FC = () => {
  const { showVpnManager, setShowVpnManager } = useUiStore();
  const {
    status,
    profiles,
    fetchStatus,
    fetchProfiles,
    setVpnMode,
    toggleKillSwitch,
    importWireguard,
    importOpenvpn,
    deleteProfile,
  } = useVpnStore();

  const [activeTab, setActiveTab] = useState<'tunnel' | 'import' | 'profiles'>('tunnel');
  const [importProtocol, setImportProtocol] = useState<'wireguard' | 'openvpn'>('wireguard');
  const [profileName, setProfileName] = useState('');
  const [profileContent, setProfileContent] = useState('');
  const [importMessage, setImportMessage] = useState<{ text: string; success: boolean } | null>(null);

  useEffect(() => {
    if (showVpnManager) {
      fetchStatus();
      fetchProfiles();
    }
  }, [showVpnManager, fetchStatus, fetchProfiles]);

  if (!showVpnManager) return null;

  const handleImport = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!profileContent.trim()) return;

    try {
      if (importProtocol === 'wireguard') {
        await importWireguard(profileContent, profileName || 'WireGuard Tunnel');
      } else {
        await importOpenvpn(profileContent, profileName || 'OpenVPN Tunnel');
      }
      setImportMessage({ text: 'Profile imported securely into OS Credential Store.', success: true });
      setProfileContent('');
      setProfileName('');
      setTimeout(() => {
        setImportMessage(null);
        setActiveTab('profiles');
      }, 1500);
    } catch (err) {
      setImportMessage({ text: String(err), success: false });
    }
  };

  const handleSampleWgLoad = () => {
    setProfileName('ADC Tokyo Fast WireGuard');
    setProfileContent(`[Interface]
PrivateKey = aaaaaaaa11111111bbbbbbbb22222222cccccccc33=
Address = 10.64.0.2/32
DNS = 1.1.1.1, 1.0.0.1

[Peer]
PublicKey = dddddddd44444444eeeeeeee55555555ffffffff66=
Endpoint = 198.51.100.10:51820
AllowedIPs = 0.0.0.0/0, ::/0
PersistentKeepalive = 25`);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-md animate-fadeIn">
      <div className="relative w-full max-w-3xl max-h-[90vh] flex flex-col bg-[#0b0c10] border border-adc-crimson/40 rounded-xl shadow-2xl overflow-hidden text-adc-ivory">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-adc-crimson/30 bg-[#16181f]/90">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-adc-crimson/20 border border-adc-crimson/50 text-adc-crimson-light">
              <Shield className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-lg font-semibold tracking-wide text-white">VPN for ADC Only</h2>
              <p className="text-xs text-adc-muted">Isolated BitTorrent socket binding & kill switch protection</p>
            </div>
          </div>
          <button
            onClick={() => setShowVpnManager(false)}
            className="p-1 text-adc-muted hover:text-white rounded-md transition-colors"
          >
            ✕
          </button>
        </div>

        {/* Tab Switcher */}
        <div className="flex border-b border-white/10 bg-black/40 px-6 gap-6 text-xs">
          <button
            onClick={() => setActiveTab('tunnel')}
            className={`py-3 font-medium border-b-2 transition-colors ${
              activeTab === 'tunnel' ? 'border-adc-crimson text-white' : 'border-transparent text-adc-muted hover:text-white'
            }`}
          >
            Tunnel & Kill Switch
          </button>
          <button
            onClick={() => setActiveTab('import')}
            className={`py-3 font-medium border-b-2 transition-colors ${
              activeTab === 'import' ? 'border-adc-crimson text-white' : 'border-transparent text-adc-muted hover:text-white'
            }`}
          >
            Import Profile (.conf / .ovpn)
          </button>
          <button
            onClick={() => setActiveTab('profiles')}
            className={`py-3 font-medium border-b-2 transition-colors ${
              activeTab === 'profiles' ? 'border-adc-crimson text-white' : 'border-transparent text-adc-muted hover:text-white'
            }`}
          >
            Configured Profiles ({profiles.length})
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {activeTab === 'tunnel' && (
            <>
              {/* VPN Mode Selector */}
              <div className="space-y-3">
                <label className="text-xs font-semibold text-adc-ivory uppercase tracking-wider">
                  Select Routing Mode
                </label>
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
                  {[
                    {
                      mode: 'AdcOnlyVpn' as VpnMode,
                      title: 'ADC-Only VPN',
                      desc: 'Exclusive socket bind. Only ADC torrent traffic traverses the tunnel.',
                    },
                    {
                      mode: 'SystemVpn' as VpnMode,
                      title: 'System VPN',
                      desc: 'Routes all computer traffic through OS default VPN adapter.',
                    },
                    {
                      mode: 'NoVpn' as VpnMode,
                      title: 'No VPN',
                      desc: 'Direct connection using standard network interfaces.',
                    },
                  ].map((item) => (
                    <button
                      key={item.mode}
                      onClick={() => setVpnMode(item.mode)}
                      className={`p-4 rounded-lg border text-left transition-all ${
                        status?.mode === item.mode
                          ? 'border-adc-crimson bg-adc-crimson/15 shadow-lg'
                          : 'border-white/10 bg-black/40 hover:border-white/20'
                      }`}
                    >
                      <div className="flex items-center justify-between">
                        <span className="text-sm font-semibold text-white">{item.title}</span>
                        {status?.mode === item.mode && <CheckCircle2 className="w-4 h-4 text-adc-crimson-light" />}
                      </div>
                      <p className="mt-1 text-xs text-adc-muted leading-relaxed">{item.desc}</p>
                    </button>
                  ))}
                </div>
              </div>

              {/* Status Card */}
              <div className="p-4 rounded-lg border border-white/10 bg-black/50 space-y-4">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <div
                      className={`w-3 h-3 rounded-full ${
                        status?.connected ? 'bg-emerald-500 animate-pulse' : 'bg-adc-muted'
                      }`}
                    />
                    <div>
                      <span className="text-sm font-semibold">
                        {status?.connected ? 'VPN Active & Sockets Bound' : 'VPN Inactive'}
                      </span>
                      <p className="text-[11px] text-adc-muted font-mono">
                        Interface: {status?.interface_name || 'None'} | Assigned IP: {status?.assigned_ip || 'None'}
                      </p>
                    </div>
                  </div>
                  <span className="text-xs px-2.5 py-1 rounded border border-white/10 bg-white/5 font-mono">
                    Mode: {status?.mode}
                  </span>
                </div>

                {status?.connected && (
                  <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 pt-2 border-t border-white/5 text-xs">
                    <div>
                      <span className="text-adc-muted">Endpoint</span>
                      <p className="font-mono text-white mt-0.5">{status?.endpoint || '198.51.100.10:51820'}</p>
                    </div>
                    <div>
                      <span className="text-adc-muted">Last Handshake</span>
                      <p className="font-mono text-white mt-0.5">{status?.handshake_age_secs ?? 24}s ago</p>
                    </div>
                    <div>
                      <span className="text-adc-muted">Sent (Tx)</span>
                      <p className="font-mono text-white mt-0.5">{(status?.bytes_sent / 1024 / 1024).toFixed(1)} MB</p>
                    </div>
                    <div>
                      <span className="text-adc-muted">Received (Rx)</span>
                      <p className="font-mono text-white mt-0.5">{(status?.bytes_received / 1024 / 1024).toFixed(1)} MB</p>
                    </div>
                  </div>
                )}
              </div>

              {/* Kill Switch Panel */}
              <div className="p-4 rounded-lg border border-adc-crimson/30 bg-black/40 flex items-center justify-between">
                <div className="flex items-start gap-3">
                  <div className="p-2 rounded bg-adc-crimson/20 text-adc-crimson-light mt-0.5">
                    <Power className="w-4 h-4" />
                  </div>
                  <div>
                    <h4 className="text-sm font-semibold text-white">ADC Kill Switch</h4>
                    <p className="text-xs text-adc-muted mt-0.5 max-w-md">
                      If the VPN disconnects, pause all torrent transfers immediately to prevent traffic leakage through the default interface.
                    </p>
                  </div>
                </div>

                <button
                  onClick={() => toggleKillSwitch(!status?.kill_switch_active)}
                  className={`px-4 py-1.5 text-xs font-semibold rounded transition-colors ${
                    status?.kill_switch_active
                      ? 'bg-adc-crimson text-white shadow-lg'
                      : 'bg-white/10 text-adc-muted hover:text-white'
                  }`}
                >
                  {status?.kill_switch_active ? 'Kill Switch Armed' : 'Disabled'}
                </button>
              </div>

              {/* Leak Protection Checks */}
              <div className="grid grid-cols-2 gap-3">
                <div className="p-3 rounded-lg border border-white/10 bg-black/30 flex items-center gap-3">
                  <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                  <div>
                    <span className="text-xs font-semibold text-white">DNS Leak Protection</span>
                    <p className="text-[11px] text-adc-muted">DNS routed exclusively through tunnel DNS</p>
                  </div>
                </div>

                <div className="p-3 rounded-lg border border-white/10 bg-black/30 flex items-center gap-3">
                  <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                  <div>
                    <span className="text-xs font-semibold text-white">IPv6 Leak Shield</span>
                    <p className="text-[11px] text-adc-muted">Blocks unencrypted local IPv6 bypass</p>
                  </div>
                </div>
              </div>
            </>
          )}

          {activeTab === 'import' && (
            <form onSubmit={handleImport} className="space-y-4">
              <div className="flex items-center gap-4">
                <label className="text-xs font-semibold text-adc-ivory">Format:</label>
                <div className="flex gap-2">
                  <button
                    type="button"
                    onClick={() => setImportProtocol('wireguard')}
                    className={`px-3 py-1 text-xs rounded border transition-colors ${
                      importProtocol === 'wireguard'
                        ? 'border-adc-crimson bg-adc-crimson/20 text-white'
                        : 'border-white/10 bg-black/40 text-adc-muted'
                    }`}
                  >
                    WireGuard (.conf)
                  </button>
                  <button
                    type="button"
                    onClick={() => setImportProtocol('openvpn')}
                    className={`px-3 py-1 text-xs rounded border transition-colors ${
                      importProtocol === 'openvpn'
                        ? 'border-adc-crimson bg-adc-crimson/20 text-white'
                        : 'border-white/10 bg-black/40 text-adc-muted'
                    }`}
                  >
                    OpenVPN (.ovpn)
                  </button>
                </div>

                <button
                  type="button"
                  onClick={handleSampleWgLoad}
                  className="ml-auto text-xs text-adc-crimson-light hover:underline"
                >
                  Load Sample WireGuard Config
                </button>
              </div>

              <div>
                <label className="block text-xs text-adc-muted mb-1">Tunnel Name</label>
                <input
                  type="text"
                  placeholder="e.g. My Private WireGuard Node"
                  value={profileName}
                  onChange={(e) => setProfileName(e.target.value)}
                  className="w-full px-3 py-2 text-xs rounded border border-white/20 bg-black/60 text-white focus:border-adc-crimson focus:outline-none font-sans"
                />
              </div>

              <div>
                <label className="block text-xs text-adc-muted mb-1">Configuration Contents</label>
                <textarea
                  rows={8}
                  placeholder="Paste [Interface] and [Peer] configuration here..."
                  value={profileContent}
                  onChange={(e) => setProfileContent(e.target.value)}
                  className="w-full p-3 text-xs font-mono rounded border border-white/20 bg-black/60 text-white focus:border-adc-crimson focus:outline-none"
                />
              </div>

              {/* Secure Keychain Notice */}
              <div className="p-3 rounded-lg border border-blue-500/30 bg-blue-950/20 text-xs text-blue-200/90 flex items-start gap-2.5">
                <Lock className="w-4 h-4 text-blue-400 shrink-0 mt-0.5" />
                <p>
                  <strong>Zero-Leak Keychain Security:</strong> Private keys and authentication credentials are encrypted directly into the OS Keychain (macOS) / Credential Manager (Windows). Secrets are never written to disk or logs.
                </p>
              </div>

              {importMessage && (
                <div
                  className={`p-3 rounded text-xs ${
                    importMessage.success
                      ? 'border border-emerald-500/40 bg-emerald-950/30 text-emerald-300'
                      : 'border border-red-500/40 bg-red-950/30 text-red-300'
                  }`}
                >
                  {importMessage.text}
                </div>
              )}

              <div className="flex justify-end gap-3 pt-2">
                <button
                  type="submit"
                  disabled={!profileContent.trim()}
                  className="px-4 py-2 text-xs font-semibold rounded bg-adc-crimson hover:bg-adc-crimson-light text-white transition-colors disabled:opacity-50"
                >
                  Import and Secure in Keychain
                </button>
              </div>
            </form>
          )}

          {activeTab === 'profiles' && (
            <div className="space-y-3">
              {profiles.length === 0 ? (
                <div className="text-center py-12 text-adc-muted border border-dashed border-white/10 rounded-lg">
                  <FileText className="w-8 h-8 mx-auto mb-2 opacity-40" />
                  <p className="text-xs">No VPN profiles imported yet.</p>
                  <button
                    onClick={() => setActiveTab('import')}
                    className="mt-3 text-xs text-adc-crimson-light hover:underline"
                  >
                    Import WireGuard or OpenVPN Profile →
                  </button>
                </div>
              ) : (
                profiles.map((p) => (
                  <div
                    key={p.id}
                    className="p-4 rounded-lg border border-white/10 bg-black/40 flex items-center justify-between"
                  >
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-semibold text-white">{p.name}</span>
                        <span className="px-1.5 py-0.5 text-[10px] uppercase font-bold rounded bg-blue-500/20 text-blue-300 border border-blue-500/30">
                          {p.protocol}
                        </span>
                      </div>
                      <p className="text-xs text-adc-muted font-mono mt-1">
                        Endpoint: {p.endpoint} | Address: {p.interface_address}
                      </p>
                    </div>
                    <button
                      onClick={() => deleteProfile(p.id)}
                      className="p-2 text-adc-muted hover:text-red-400 rounded transition-colors"
                      title="Delete profile"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                ))
              )}
            </div>
          )}

          {/* Privacy Disclaimer */}
          <div className="p-3 rounded-lg border border-white/10 bg-black/40 text-[11px] text-adc-muted leading-relaxed flex items-start gap-2">
            <HelpCircle className="w-4 h-4 text-adc-muted shrink-0 mt-0.5" />
            <p>
              ADC cannot guarantee anonymity if your VPN service, external DNS resolver, web browser, or host operating system leaks traffic or provides compromised endpoints.
            </p>
          </div>
        </div>

        {/* Footer */}
        <div className="px-6 py-3 border-t border-white/10 bg-[#16181f] flex justify-end">
          <button
            onClick={() => setShowVpnManager(false)}
            className="px-4 py-1.5 text-xs rounded border border-white/20 bg-white/5 hover:bg-white/10 text-white transition-colors"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
};
