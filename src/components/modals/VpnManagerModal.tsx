import React, { useEffect, useState } from 'react';
import { useVpnStore } from '../../stores/vpnStore';
import { useUiStore } from '../../stores/uiStore';
import { Shield, Power, Trash2, CheckCircle2, FileText, X } from 'lucide-react';
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
    setProfileName('Fast WireGuard');
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
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm select-none font-sans">
      <div className="relative w-full max-w-2xl max-h-[85vh] flex flex-col bg-[#141414] border border-[#333333] rounded-xl shadow-2xl overflow-hidden text-white">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-[#333333] bg-[#1f1f1f]">
          <div className="flex items-center gap-2.5">
            <Shield className="w-4 h-4 text-[#e50914]" />
            <h2 className="text-sm font-bold text-white">VPN & Kill Switch Manager</h2>
          </div>
          <button
            onClick={() => setShowVpnManager(false)}
            className="p-1 text-[#a3a3a3] hover:text-white rounded transition"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Tab Switcher */}
        <div className="flex border-b border-[#333333] bg-[#1f1f1f] px-5 gap-4 text-xs">
          <button
            onClick={() => setActiveTab('tunnel')}
            className={`py-2.5 font-medium border-b-2 transition ${
              activeTab === 'tunnel' ? 'border-[#e50914] text-white' : 'border-transparent text-[#a3a3a3] hover:text-white'
            }`}
          >
            Routing & Kill Switch
          </button>
          <button
            onClick={() => setActiveTab('import')}
            className={`py-2.5 font-medium border-b-2 transition ${
              activeTab === 'import' ? 'border-[#e50914] text-white' : 'border-transparent text-[#a3a3a3] hover:text-white'
            }`}
          >
            Import Profile
          </button>
          <button
            onClick={() => setActiveTab('profiles')}
            className={`py-2.5 font-medium border-b-2 transition ${
              activeTab === 'profiles' ? 'border-[#e50914] text-white' : 'border-transparent text-[#a3a3a3] hover:text-white'
            }`}
          >
            Profiles ({profiles.length})
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-5 space-y-4 text-xs">
          {activeTab === 'tunnel' && (
            <>
              {/* Routing Modes */}
              <div className="space-y-2">
                <span className="text-[10px] font-mono text-[#a3a3a3] uppercase">Routing Mode</span>
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-2.5">
                  {[
                    {
                      mode: 'AdcOnlyVpn' as VpnMode,
                      title: 'Client-Only VPN',
                      desc: 'Binds torrent sockets exclusively to tunnel interface.',
                    },
                    {
                      mode: 'SystemVpn' as VpnMode,
                      title: 'System VPN',
                      desc: 'Routes all system traffic through default adapter.',
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
                      className={`p-3 rounded-lg border text-left transition ${
                        status?.mode === item.mode
                          ? 'border-[#e50914] bg-[#1f1f1f]'
                          : 'border-[#333333] bg-[#1f1f1f] hover:border-[#525252]'
                      }`}
                    >
                      <div className="flex items-center justify-between">
                        <span className="text-xs font-bold text-white">{item.title}</span>
                        {status?.mode === item.mode && <CheckCircle2 className="w-3.5 h-3.5 text-[#e50914]" />}
                      </div>
                      <p className="mt-1 text-[11px] text-[#a3a3a3] leading-relaxed">{item.desc}</p>
                    </button>
                  ))}
                </div>
              </div>

              {/* Status Card */}
              <div className="p-3.5 rounded-lg border border-[#333333] bg-[#1f1f1f] space-y-2 font-mono">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-sans text-white font-semibold">
                    {status?.connected ? 'VPN Active & Sockets Bound' : 'VPN Inactive'}
                  </span>
                  <span className="text-[10px] px-2 py-0.5 rounded bg-[#141414] border border-[#333333] text-white">
                    Mode: {status?.mode}
                  </span>
                </div>
                <div className="text-[11px] text-[#a3a3a3]">
                  Interface: {status?.interface_name || 'None'} · IP: {status?.assigned_ip || 'None'}
                </div>
              </div>

              {/* Kill Switch Panel */}
              <div className="p-3.5 rounded-lg border border-[#333333] bg-[#1f1f1f] flex items-center justify-between">
                <div className="flex items-start gap-3">
                  <Power className="w-4 h-4 text-[#e50914] mt-0.5 shrink-0" />
                  <div>
                    <h4 className="text-xs font-semibold text-white">Kill Switch Protection</h4>
                    <p className="text-[11px] text-[#a3a3a3] mt-0.5 max-w-sm">
                      Pauses all transfers immediately if VPN connection drops to prevent traffic leakage.
                    </p>
                  </div>
                </div>

                <button
                  onClick={() => toggleKillSwitch(!status?.kill_switch_active)}
                  className={`px-3 py-1.5 text-xs font-semibold rounded transition ${
                    status?.kill_switch_active
                      ? 'bg-[#e50914] text-white'
                      : 'bg-[#141414] border border-[#333333] text-[#a3a3a3] hover:text-white'
                  }`}
                >
                  {status?.kill_switch_active ? 'Armed' : 'Disabled'}
                </button>
              </div>
            </>
          )}

          {activeTab === 'import' && (
            <form onSubmit={handleImport} className="space-y-3">
              <div className="flex items-center gap-3">
                <span className="text-xs text-[#a3a3a3]">Format:</span>
                <div className="flex gap-2">
                  <button
                    type="button"
                    onClick={() => setImportProtocol('wireguard')}
                    className={`px-2.5 py-1 text-xs rounded border transition ${
                      importProtocol === 'wireguard'
                        ? 'border-[#e50914] bg-[#e50914] text-white'
                        : 'border-[#333333] bg-[#1f1f1f] text-[#a3a3a3]'
                    }`}
                  >
                    WireGuard
                  </button>
                  <button
                    type="button"
                    onClick={() => setImportProtocol('openvpn')}
                    className={`px-2.5 py-1 text-xs rounded border transition ${
                      importProtocol === 'openvpn'
                        ? 'border-[#e50914] bg-[#e50914] text-white'
                        : 'border-[#333333] bg-[#1f1f1f] text-[#a3a3a3]'
                    }`}
                  >
                    OpenVPN
                  </button>
                </div>

                <button
                  type="button"
                  onClick={handleSampleWgLoad}
                  className="ml-auto text-xs text-[#e50914] hover:underline"
                >
                  Load Sample
                </button>
              </div>

              <div>
                <label className="block text-[11px] text-[#a3a3a3] mb-1">Tunnel Name</label>
                <input
                  type="text"
                  placeholder="e.g. My VPN Tunnel"
                  value={profileName}
                  onChange={(e) => setProfileName(e.target.value)}
                  className="w-full px-3 py-1.5 text-xs rounded border border-[#333333] bg-[#1f1f1f] text-white focus:border-[#e50914] focus:outline-none"
                />
              </div>

              <div>
                <label className="block text-[11px] text-[#a3a3a3] mb-1">Configuration</label>
                <textarea
                  rows={6}
                  placeholder="Paste configuration here..."
                  value={profileContent}
                  onChange={(e) => setProfileContent(e.target.value)}
                  className="w-full p-2.5 text-xs font-mono rounded border border-[#333333] bg-[#1f1f1f] text-white focus:border-[#e50914] focus:outline-none"
                />
              </div>

              {importMessage && (
                <div
                  className={`p-2.5 rounded text-xs ${
                    importMessage.success
                      ? 'border border-[#333333] bg-[#1f1f1f] text-white'
                      : 'border border-[#e50914] bg-[#e50914]/20 text-white'
                  }`}
                >
                  {importMessage.text}
                </div>
              )}

              <div className="flex justify-end pt-1">
                <button
                  type="submit"
                  disabled={!profileContent.trim()}
                  className="px-4 py-1.5 text-xs font-semibold rounded bg-[#e50914] hover:bg-[#f40612] text-white transition disabled:opacity-40"
                >
                  Import Profile
                </button>
              </div>
            </form>
          )}

          {activeTab === 'profiles' && (
            <div className="space-y-2">
              {profiles.length === 0 ? (
                <div className="text-center py-10 text-[#a3a3a3] border border-dashed border-[#333333] rounded-lg">
                  <FileText className="w-8 h-8 mx-auto mb-2 opacity-40" />
                  <p className="text-xs">No VPN profiles imported yet.</p>
                </div>
              ) : (
                profiles.map((p) => (
                  <div
                    key={p.id}
                    className="p-3 rounded-lg border border-[#333333] bg-[#1f1f1f] flex items-center justify-between"
                  >
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-bold text-white">{p.name}</span>
                        <span className="px-1.5 py-0.5 text-[10px] uppercase font-mono rounded bg-[#141414] border border-[#333333] text-[#a3a3a3]">
                          {p.protocol}
                        </span>
                      </div>
                      <p className="text-[11px] text-[#a3a3a3] font-mono mt-0.5">
                        {p.endpoint}
                      </p>
                    </div>
                    <button
                      onClick={() => deleteProfile(p.id)}
                      className="p-1.5 text-[#a3a3a3] hover:text-[#e50914] transition"
                      title="Delete profile"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                ))
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="px-5 py-3 border-t border-[#333333] bg-[#1f1f1f] flex justify-end">
          <button
            onClick={() => setShowVpnManager(false)}
            className="px-4 py-1.5 text-xs rounded border border-[#333333] bg-[#141414] hover:bg-[#262626] text-white transition"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
};

export default VpnManagerModal;
