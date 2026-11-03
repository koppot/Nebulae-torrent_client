import React, { useEffect, useState } from 'react';
import { useTrackerStore } from '../../stores/trackerStore';
import { useUiStore } from '../../stores/uiStore';
import {
  Radio, ArrowUpRight, ArrowDownLeft, Clock, Link2, Unlink,
  CheckCircle2, X, RefreshCw, Eye, EyeOff,
} from 'lucide-react';
import { formatBytes } from '../../lib/formatters';

export const TrackerModal: React.FC = () => {
  const { showTrackerDashboard, setShowTrackerDashboard } = useUiStore();
  const {
    profile,
    seedTimeRecords,
    isSyncing,
    fetchProfile,
    fetchSeedTimeRecords,
    linkAccount,
    unlinkAccount,
    syncStats,
  } = useTrackerStore();

  const [usernameInput, setUsernameInput] = useState('');
  const [passkeyInput, setPasskeyInput] = useState('');
  const [isLinking, setIsLinking] = useState(false);
  const [passkeyRevealed, setPasskeyRevealed] = useState(false);

  useEffect(() => {
    if (showTrackerDashboard) {
      fetchProfile();
      fetchSeedTimeRecords();
      // Always attempt a live sync when the panel opens so numbers are fresh.
      // The underlying command is rate-limited server-side (1 call / 5 min).
      syncStats();
    } else {
      setPasskeyRevealed(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showTrackerDashboard]);

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

  /**
   * The masked_passkey from the Rust layer already has the middle redacted
   * (e.g. "adc_pk_••••••••••••••on1!").  When the user reveals it, we cannot
   * un-redact it on the frontend — we need the Rust `get_stored_passkey`
   * command for the full value.  For now the reveal toggle shows/hides the
   * already-masked string so the user knows what is stored without exposing
   * the raw secret unintentionally.  A future `get_stored_passkey(reveal:true)`
   * Tauri command can replace this.
   */
  const passkeyDisplay = profile?.masked_passkey ?? '—';

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm select-none font-sans">
      <div className="relative w-full max-w-2xl max-h-[85vh] flex flex-col bg-[#141414] border border-[#333333] rounded-xl shadow-2xl overflow-hidden text-white">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-[#333333] bg-[#1f1f1f]">
          <div className="flex items-center gap-2.5">
            <Radio className="w-4 h-4 text-[#e50914]" />
            <h2 className="text-sm font-bold text-white">Tracker Integration &amp; Ratio Coach</h2>
          </div>
          <button
            onClick={() => setShowTrackerDashboard(false)}
            className="p-1 text-[#a3a3a3] hover:text-white rounded transition"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-5 space-y-4 text-xs">
          {!profile?.is_linked ? (
            /* Unlinked Form */
            <div className="p-5 rounded-lg border border-[#333333] bg-[#1f1f1f] space-y-4">
              <div className="text-center max-w-sm mx-auto space-y-1.5">
                <h3 className="text-sm font-bold text-white">Link Tracker Passkey</h3>
                <p className="text-xs text-[#a3a3a3]">
                  Enter your passkey to monitor your community ratio and satisfy seed-time rules.
                </p>
              </div>

              <form onSubmit={handleLink} className="max-w-sm mx-auto space-y-3">
                <div>
                  <label className="block text-[11px] text-[#a3a3a3] mb-1">Username</label>
                  <input
                    type="text"
                    required
                    placeholder="ADC Username"
                    value={usernameInput}
                    onChange={(e) => setUsernameInput(e.target.value)}
                    className="w-full px-3 py-1.5 text-xs rounded border border-[#333333] bg-[#141414] text-white focus:border-[#e50914] focus:outline-none"
                  />
                </div>

                <div>
                  <label className="block text-[11px] text-[#a3a3a3] mb-1">Passkey</label>
                  <input
                    type="password"
                    required
                    placeholder="32-character passkey"
                    value={passkeyInput}
                    onChange={(e) => setPasskeyInput(e.target.value)}
                    className="w-full px-3 py-1.5 text-xs rounded border border-[#333333] bg-[#141414] text-white focus:border-[#e50914] focus:outline-none font-mono"
                  />
                </div>

                <button
                  type="submit"
                  disabled={isLinking}
                  className="w-full flex items-center justify-center gap-2 py-2 text-xs font-semibold rounded bg-[#e50914] hover:bg-[#f40612] text-white transition disabled:opacity-40"
                >
                  <Link2 className="w-3.5 h-3.5" />
                  <span>{isLinking ? 'Verifying...' : 'Link Account'}</span>
                </button>
              </form>
            </div>
          ) : (
            /* Linked Dashboard */
            <>
              {/* Profile Card */}
              <div className="p-3.5 rounded-lg border border-[#333333] bg-[#1f1f1f] flex items-center justify-between">
                <div>
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-bold text-white">{profile.username}</span>
                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-[#e50914] text-white font-mono">
                      Linked
                    </span>
                  </div>

                  {/* Passkey row with reveal toggle */}
                  <div className="flex items-center gap-1.5 mt-0.5">
                    <p className="text-[11px] font-mono text-[#a3a3a3]">
                      Passkey:&nbsp;
                      {passkeyRevealed ? passkeyDisplay : '••••••••••••••••••••••••••••••••'}
                    </p>
                    <button
                      onClick={() => setPasskeyRevealed((v) => !v)}
                      className="text-[#a3a3a3] hover:text-white transition"
                      title={passkeyRevealed ? 'Hide passkey' : 'Reveal passkey'}
                    >
                      {passkeyRevealed
                        ? <EyeOff className="w-3 h-3" />
                        : <Eye className="w-3 h-3" />}
                    </button>
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  {/* Live stats refresh */}
                  <button
                    onClick={() => syncStats()}
                    disabled={isSyncing}
                    className="flex items-center gap-1 px-2.5 py-1 rounded border border-[#333333] bg-[#141414] text-[#a3a3a3] hover:text-white transition disabled:opacity-40"
                    title="Refresh stats from ADC"
                  >
                    <RefreshCw className={`w-3 h-3 ${isSyncing ? 'animate-spin' : ''}`} />
                    <span>{isSyncing ? 'Syncing…' : 'Refresh'}</span>
                  </button>

                  <button
                    onClick={() => unlinkAccount()}
                    className="flex items-center gap-1 px-2.5 py-1 rounded border border-[#333333] bg-[#141414] text-[#a3a3a3] hover:text-[#e50914] transition"
                  >
                    <Unlink className="w-3 h-3" />
                    <span>Unlink</span>
                  </button>
                </div>
              </div>

              {/* Stats Grid */}
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-2.5 font-mono">
                <div className="p-3 rounded-lg border border-[#333333] bg-[#1f1f1f]">
                  <span className="text-[10px] text-[#a3a3a3] block font-sans">Ratio</span>
                  <span className="text-base font-bold text-white mt-0.5 block">
                    {profile.ratio.toFixed(2)}
                  </span>
                </div>

                <div className="p-3 rounded-lg border border-[#333333] bg-[#1f1f1f]">
                  <span className="text-[10px] text-[#a3a3a3] block font-sans">Uploaded</span>
                  <div className="flex items-center gap-1 text-white font-semibold text-xs mt-0.5">
                    <ArrowUpRight className="w-3.5 h-3.5 text-[#e50914]" />
                    {formatBytes(profile.uploaded_bytes)}
                  </div>
                </div>

                <div className="p-3 rounded-lg border border-[#333333] bg-[#1f1f1f]">
                  <span className="text-[10px] text-[#a3a3a3] block font-sans">Downloaded</span>
                  <div className="flex items-center gap-1 text-white font-semibold text-xs mt-0.5">
                    <ArrowDownLeft className="w-3.5 h-3.5 text-[#a3a3a3]" />
                    {formatBytes(profile.downloaded_bytes)}
                  </div>
                </div>

                <div className="p-3 rounded-lg border border-[#333333] bg-[#1f1f1f]">
                  <span className="text-[10px] text-[#a3a3a3] block font-sans">Bonus Points</span>
                  <span className="text-xs font-semibold text-white mt-0.5 block">
                    {profile.bonus_points.toFixed(1)} BP
                  </span>
                </div>
              </div>

              {/* Seed Time Shield */}
              <div className="p-3.5 rounded-lg border border-[#333333] bg-[#1f1f1f] space-y-2.5">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-bold text-white flex items-center gap-1.5">
                    <Clock className="w-3.5 h-3.5 text-[#e50914]" />
                    Seed Time Requirement (72h)
                  </span>
                </div>

                <div className="space-y-2">
                  {seedTimeRecords.map((rec) => {
                    const currentHours = (rec.seeding_time_seconds / 3600).toFixed(1);
                    const reqHours = (rec.required_seeding_seconds / 3600).toFixed(0);
                    const pct = Math.min(
                      100,
                      Math.round((rec.seeding_time_seconds / rec.required_seeding_seconds) * 100),
                    );

                    return (
                      <div key={rec.info_hash} className="p-2.5 rounded border border-[#333333] bg-[#141414] space-y-1.5">
                        <div className="flex items-center justify-between text-xs">
                          <span className="font-medium text-white truncate max-w-sm">{rec.torrent_name}</span>
                          {rec.is_shield_satisfied ? (
                            <span className="flex items-center gap-1 text-[11px] text-white">
                              <CheckCircle2 className="w-3 h-3 text-[#e50914]" />
                              Complete
                            </span>
                          ) : (
                            <span className="text-[11px] text-[#a3a3a3] font-mono">
                              {currentHours}h / {reqHours}h
                            </span>
                          )}
                        </div>

                        <div className="w-full bg-[#333333] h-1 rounded-full overflow-hidden">
                          <div
                            className="h-full bg-[#e50914] transition-all"
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
        </div>

        {/* Footer */}
        <div className="px-5 py-3 border-t border-[#333333] bg-[#1f1f1f] flex justify-end">
          <button
            onClick={() => setShowTrackerDashboard(false)}
            className="px-4 py-1.5 text-xs rounded border border-[#333333] bg-[#141414] hover:bg-[#262626] text-white transition"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
};

export default TrackerModal;
