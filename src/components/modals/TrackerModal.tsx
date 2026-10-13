import React, { useEffect, useState } from 'react';
import { useTrackerStore } from '../../stores/trackerStore';
import { useUiStore } from '../../stores/uiStore';
import { Shield, Award, ArrowUpRight, ArrowDownLeft, Clock, AlertCircle, Link2, Unlink, Calculator, CheckCircle2 } from 'lucide-react';
import { formatBytes } from '../../lib/formatters';

export const TrackerModal: React.FC = () => {
  const { showTrackerDashboard, setShowTrackerDashboard } = useUiStore();
  const {
    profile,
    seedTimeRecords,
    fetchProfile,
    fetchSeedTimeRecords,
    linkAccount,
    unlinkAccount,
    calculateProjection,
  } = useTrackerStore();

  const [usernameInput, setUsernameInput] = useState('');
  const [passkeyInput, setPasskeyInput] = useState('');
  const [isLinking, setIsLinking] = useState(false);
  const [simulatedDlGb, setSimulatedDlGb] = useState('25');
  const [projectedRatio, setProjectedRatio] = useState<number | null>(null);

  useEffect(() => {
    if (showTrackerDashboard) {
      fetchProfile();
      fetchSeedTimeRecords();
    }
  }, [showTrackerDashboard, fetchProfile, fetchSeedTimeRecords]);

  useEffect(() => {
    if (profile?.is_linked) {
      const gb = parseFloat(simulatedDlGb) || 0;
      const additionalBytes = gb * 1024 * 1024 * 1024;
      calculateProjection(profile.uploaded_bytes, profile.downloaded_bytes, additionalBytes)
        .then((ratio) => setProjectedRatio(ratio));
    }
  }, [simulatedDlGb, profile, calculateProjection]);

  if (!showTrackerDashboard) return null;

  const handleLink = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!usernameInput.trim() || !passkeyInput.trim()) return;

    setIsLinking(true);
    try {
      await linkAccount(usernameInput, passkeyInput);
      setUsernameInput('');
      setPasskeyInput('');
    } catch {
      //
    } finally {
      setIsLinking(false);
    }
  };

  const ratio = profile?.ratio ?? 1.0;
  const ratioColor = ratio >= 1.0 ? 'text-emerald-400' : ratio >= 0.8 ? 'text-amber-400' : 'text-red-400';

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
              <h2 className="text-lg font-semibold tracking-wide text-white">AsianDVDClub Tracker Integration</h2>
              <p className="text-xs text-adc-muted">Community passkey link, Ratio Coach & Seed Time Shield</p>
            </div>
          </div>
          <button
            onClick={() => setShowTrackerDashboard(false)}
            className="p-1 text-adc-muted hover:text-white rounded-md transition-colors"
          >
            ✕
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {!profile?.is_linked ? (
            /* Unlinked State */
            <div className="p-6 rounded-lg border border-white/10 bg-black/40 space-y-6">
              <div className="text-center max-w-md mx-auto space-y-2">
                <img
                  src="/src/assets/app-icon.png"
                  alt="ADC Crest"
                  className="w-16 h-16 mx-auto object-contain drop-shadow-[0_0_12px_rgba(132,0,1,0.5)]"
                />
                <h3 className="text-base font-semibold text-white">Link AsianDVDClub Account</h3>
                <p className="text-xs text-adc-muted">
                  Connect your account securely using your tracker passkey. Nebulae monitors your ratio and ensures you satisfy all required seed-time guidelines.
                </p>
              </div>

              <form onSubmit={handleLink} className="max-w-md mx-auto space-y-4">
                <div>
                  <label className="block text-xs text-adc-muted mb-1">ADC Username</label>
                  <input
                    type="text"
                    required
                    placeholder="Enter your ADC username"
                    value={usernameInput}
                    onChange={(e) => setUsernameInput(e.target.value)}
                    className="w-full px-3 py-2 text-xs rounded border border-white/20 bg-black/60 text-white focus:border-adc-crimson focus:outline-none"
                  />
                </div>

                <div>
                  <label className="block text-xs text-adc-muted mb-1">Personal Passkey (32 characters)</label>
                  <input
                    type="password"
                    required
                    placeholder="Paste tracker passkey"
                    value={passkeyInput}
                    onChange={(e) => setPasskeyInput(e.target.value)}
                    className="w-full px-3 py-2 text-xs rounded border border-white/20 bg-black/60 text-white focus:border-adc-crimson focus:outline-none font-mono"
                  />
                  <p className="mt-1 text-[11px] text-adc-muted">
                    Your passkey is stored securely in the OS Keychain and masked everywhere in the app.
                  </p>
                </div>

                <button
                  type="submit"
                  disabled={isLinking}
                  className="w-full flex items-center justify-center gap-2 py-2.5 text-xs font-semibold rounded bg-adc-crimson hover:bg-adc-crimson-light text-white shadow-lg transition-colors"
                >
                  <Link2 className="w-3.5 h-3.5" />
                  {isLinking ? 'Verifying...' : 'Link Account Securely'}
                </button>
              </form>
            </div>
          ) : (
            /* Linked State */
            <>
              {/* Linked Profile Card */}
              <div className="p-4 rounded-lg border border-white/10 bg-black/50 flex flex-wrap items-center justify-between gap-4">
                <div className="flex items-center gap-3">
                  <div className="w-10 h-10 rounded-full border border-adc-crimson/50 bg-adc-crimson/20 flex items-center justify-center text-white font-bold text-sm">
                    {profile.username.slice(0, 2).toUpperCase()}
                  </div>
                  <div>
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-semibold text-white">{profile.username}</span>
                      <span className="px-1.5 py-0.5 text-[10px] uppercase font-bold rounded bg-emerald-500/20 text-emerald-300 border border-emerald-500/30">
                        Linked Member
                      </span>
                    </div>
                    <p className="text-xs font-mono text-adc-muted mt-0.5">
                      Passkey: <span className="text-adc-ivory">{profile.masked_passkey}</span>
                    </p>
                  </div>
                </div>

                <div className="flex items-center gap-4 text-xs">
                  <div className="text-right">
                    <span className="text-adc-muted block">Announce Status</span>
                    <span className="text-emerald-400 font-mono font-medium">{profile.announce_status}</span>
                  </div>
                  <button
                    onClick={() => unlinkAccount()}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded border border-red-500/30 bg-red-950/20 text-red-300 hover:bg-red-950/40 transition-colors"
                  >
                    <Unlink className="w-3.5 h-3.5" />
                    Unlink
                  </button>
                </div>
              </div>

              {/* Stats Grid */}
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
                <div className="p-4 rounded-lg border border-white/10 bg-black/40">
                  <span className="text-xs text-adc-muted">Community Ratio</span>
                  <div className="mt-1 flex items-baseline gap-2">
                    <span className={`text-2xl font-bold font-mono ${ratioColor}`}>
                      {profile.ratio.toFixed(2)}
                    </span>
                    <span className="text-[11px] text-emerald-400">Healthy</span>
                  </div>
                </div>

                <div className="p-4 rounded-lg border border-white/10 bg-black/40">
                  <span className="text-xs text-adc-muted">Uploaded</span>
                  <div className="mt-1 flex items-center gap-1.5 text-white font-semibold font-mono text-base">
                    <ArrowUpRight className="w-4 h-4 text-emerald-400" />
                    {formatBytes(profile.uploaded_bytes)}
                  </div>
                </div>

                <div className="p-4 rounded-lg border border-white/10 bg-black/40">
                  <span className="text-xs text-adc-muted">Downloaded</span>
                  <div className="mt-1 flex items-center gap-1.5 text-white font-semibold font-mono text-base">
                    <ArrowDownLeft className="w-4 h-4 text-blue-400" />
                    {formatBytes(profile.downloaded_bytes)}
                  </div>
                </div>

                <div className="p-4 rounded-lg border border-white/10 bg-black/40">
                  <span className="text-xs text-adc-muted">Bonus Points</span>
                  <div className="mt-1 flex items-center gap-1.5 text-amber-300 font-semibold font-mono text-base">
                    <Award className="w-4 h-4 text-amber-400" />
                    {profile.bonus_points.toFixed(1)} BP
                  </div>
                </div>
              </div>

              {/* Ratio Coach & Calculator */}
              <div className="p-5 rounded-lg border border-white/10 bg-[#121319] space-y-4">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <Calculator className="w-4 h-4 text-adc-crimson-light" />
                    <h4 className="text-sm font-semibold text-white">Ratio Coach — Download Impact Simulator</h4>
                  </div>
                  <span className="text-xs text-adc-muted">Target threshold: &gt; 1.00</span>
                </div>

                <div className="flex flex-wrap items-center gap-4 text-xs">
                  <span className="text-adc-muted">Simulate new download:</span>
                  <div className="flex items-center gap-2">
                    <input
                      type="number"
                      value={simulatedDlGb}
                      onChange={(e) => setSimulatedDlGb(e.target.value)}
                      className="w-20 px-2 py-1 text-xs rounded border border-white/20 bg-black text-white font-mono focus:border-adc-crimson focus:outline-none"
                    />
                    <span className="text-adc-muted">GB</span>
                  </div>

                  {projectedRatio !== null && (
                    <div className="ml-auto flex items-center gap-2 bg-black/60 px-3 py-1.5 rounded border border-white/10">
                      <span className="text-adc-muted">Projected Ratio:</span>
                      <span className="font-mono font-bold text-sm text-emerald-400">
                        {projectedRatio.toFixed(2)}
                      </span>
                      <span className="text-[11px] text-emerald-300">
                        (Safe buffer: +{(projectedRatio - 1.0).toFixed(2)})
                      </span>
                    </div>
                  )}
                </div>
              </div>

              {/* Seed Time Shield */}
              <div className="p-5 rounded-lg border border-adc-crimson/30 bg-black/40 space-y-4">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <Clock className="w-4 h-4 text-adc-crimson-light" />
                    <h4 className="text-sm font-semibold text-white">Seed Time Shield (Hit & Run Guard)</h4>
                  </div>
                  <span className="text-[11px] text-adc-muted">Required: 72 hours per torrent</span>
                </div>

                <div className="space-y-3">
                  {seedTimeRecords.map((rec) => {
                    const currentHours = (rec.seeding_time_seconds / 3600).toFixed(1);
                    const reqHours = (rec.required_seeding_seconds / 3600).toFixed(0);
                    const pct = Math.min(100, Math.round((rec.seeding_time_seconds / rec.required_seeding_seconds) * 100));

                    return (
                      <div key={rec.info_hash} className="p-3 rounded-lg border border-white/5 bg-black/50 space-y-2">
                        <div className="flex items-center justify-between text-xs">
                          <span className="font-medium text-white truncate max-w-md">{rec.torrent_name}</span>
                          {rec.is_shield_satisfied ? (
                            <span className="flex items-center gap-1 text-[11px] text-emerald-400 font-medium">
                              <CheckCircle2 className="w-3.5 h-3.5" />
                              Shield Satisfied
                            </span>
                          ) : (
                            <span className="text-[11px] text-amber-300 font-mono">
                              {currentHours}h / {reqHours}h ({pct}%)
                            </span>
                          )}
                        </div>

                        <div className="w-full bg-white/10 h-1.5 rounded-full overflow-hidden">
                          <div
                            className={`h-full transition-all duration-300 ${
                              rec.is_shield_satisfied ? 'bg-emerald-500' : 'bg-adc-crimson'
                            }`}
                            style={{ width: `${pct}%` }}
                          />
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            </>
          )}

          {/* Guidelines Notice */}
          <div className="p-3.5 rounded-lg border border-white/10 bg-black/40 text-xs text-adc-muted leading-relaxed flex items-start gap-2.5">
            <AlertCircle className="w-4 h-4 text-adc-muted shrink-0 mt-0.5" />
            <p>
              AsianDVDClub tracker integration operates strictly through standard community BitTorrent tracker protocols. Private torrent flags are always respected, and passkeys are never transmitted anywhere other than the tracker announce endpoint.
            </p>
          </div>
        </div>

        {/* Footer */}
        <div className="px-6 py-3 border-t border-white/10 bg-[#16181f] flex justify-end">
          <button
            onClick={() => setShowTrackerDashboard(false)}
            className="px-4 py-1.5 text-xs rounded border border-white/20 bg-white/5 hover:bg-white/10 text-white transition-colors"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
};
