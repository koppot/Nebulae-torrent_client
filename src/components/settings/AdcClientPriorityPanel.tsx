import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { AdcClientTarget } from '../../types';
import { errorMessage as toErrorMessage } from '../../utils/errors';
import { Server, Laptop, ChevronUp, ChevronDown, Check, ShieldCheck } from 'lucide-react';

export const AdcClientPriorityPanel: React.FC = () => {
  const [targets, setTargets] = useState<AdcClientTarget[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);

  useEffect(() => {
    loadTargets();
  }, []);

  const loadTargets = async () => {
    try {
      setLoading(true);
      const res = await invoke<AdcClientTarget[]>('get_adc_client_targets');
      setTargets(res.sort((a, b) => a.priority - b.priority));
    } catch (err) {
      setStatusMessage(toErrorMessage(err, 'Failed to load client priority targets'));
    } finally {
      setLoading(false);
    }
  };

  const saveTargets = async (updated: AdcClientTarget[]) => {
    try {
      setSaving(true);
      setStatusMessage(null);
      await invoke('set_adc_client_targets', { targets: updated });
      setTargets(updated);
      setStatusMessage('Client priority configuration saved.');
      setTimeout(() => setStatusMessage(null), 3000);
    } catch (err) {
      setStatusMessage(toErrorMessage(err, 'Failed to save client targets'));
    } finally {
      setSaving(false);
    }
  };

  const toggleTarget = (id: string) => {
    const updated = targets.map((t) =>
      t.id === id ? { ...t, enabled: !t.enabled } : t,
    );
    saveTargets(updated);
  };

  const movePriority = (index: number, direction: 'up' | 'down') => {
    const targetIdx = direction === 'up' ? index - 1 : index + 1;
    if (targetIdx < 0 || targetIdx >= targets.length) return;

    const copy = [...targets];
    const temp = copy[index];
    copy[index] = copy[targetIdx];
    copy[targetIdx] = temp;

    // Re-index priorities 1..N
    const reindexed = copy.map((t, idx) => ({ ...t, priority: idx + 1 }));
    saveTargets(reindexed);
  };

  if (loading) {
    return (
      <div className="p-4 text-xs text-[#a3a3a3] flex items-center gap-2">
        <div className="w-3.5 h-3.5 border-2 border-[#e50914] border-t-transparent rounded-full animate-spin" />
        <span>Loading ADC client routing...</span>
      </div>
    );
  }

  return (
    <div className="space-y-3 text-xs">
      <div className="flex items-center justify-between">
        <div>
          <h4 className="text-sm font-semibold text-white flex items-center gap-2">
            <ShieldCheck className="w-4 h-4 text-[#e50914]" />
            <span>ADC One-Click Client Routing</span>
          </h4>
          <p className="text-[11px] text-[#a3a3a3] mt-0.5">
            Configure destination priority for torrents sent from AsianDVDClub one-click links.
            If the primary client is offline or unavailable, the request cascades to the next enabled client. The Nebulae Server entry uses the address and token configured in Settings → Server.
          </p>
        </div>
      </div>

      {statusMessage && (
        <div className="p-2 rounded bg-[#1e293b] border border-[#38bdf8] text-[11px] text-[#38bdf8] flex items-center gap-2">
          <Check className="w-3.5 h-3.5 flex-shrink-0" />
          <span>{statusMessage}</span>
        </div>
      )}

      <div className="space-y-2 mt-2">
        {targets.map((target, idx) => {
          const isLocal = target.client_type === 'local';
          const isFirst = idx === 0;
          const isLast = idx === targets.length - 1;

          return (
            <div
              key={target.id}
              className={`p-3 rounded-lg border transition flex items-center justify-between ${
                target.enabled
                  ? 'bg-[#1a1a1a] border-[#333333]'
                  : 'bg-[#141414] border-[#222222] opacity-60'
              }`}
            >
              <div className="flex items-center gap-3 min-w-0">
                <span className="w-5 h-5 rounded-full bg-[#262626] text-[#e50914] font-mono text-[11px] font-bold flex items-center justify-center flex-shrink-0">
                  {target.priority}
                </span>
                <div className="flex-shrink-0 text-[#a3a3a3]">
                  {isLocal ? <Laptop className="w-4 h-4 text-emerald-400" /> : <Server className="w-4 h-4 text-sky-400" />}
                </div>
                <div className="min-w-0">
                  <div className="font-medium text-white flex items-center gap-2">
                    <span className="truncate">{target.name}</span>
                    {isFirst && (
                      <span className="text-[9px] uppercase px-1.5 py-0.5 rounded bg-[#e50914]/20 text-[#e50914] font-semibold">
                        Primary
                      </span>
                    )}
                  </div>
                  <div className="text-[10px] text-[#737373] font-mono">
                    {isLocal
                      ? 'This computer'
                      : target.client_type === 'remote_server'
                        ? 'Server address and token come from Settings → Server'
                        : target.client_type}
                  </div>
                </div>
              </div>

              <div className="flex items-center gap-2 flex-shrink-0">
                <div className="flex flex-col">
                  <button
                    type="button"
                    disabled={isFirst || saving}
                    onClick={() => movePriority(idx, 'up')}
                    className="p-1 text-[#a3a3a3] hover:text-white disabled:opacity-20 disabled:hover:text-[#a3a3a3] transition"
                    title="Increase Priority"
                  >
                    <ChevronUp className="w-3.5 h-3.5" />
                  </button>
                  <button
                    type="button"
                    disabled={isLast || saving}
                    onClick={() => movePriority(idx, 'down')}
                    className="p-1 text-[#a3a3a3] hover:text-white disabled:opacity-20 disabled:hover:text-[#a3a3a3] transition"
                    title="Decrease Priority"
                  >
                    <ChevronDown className="w-3.5 h-3.5" />
                  </button>
                </div>

                <label className="flex items-center gap-1.5 ml-2 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={target.enabled}
                    disabled={saving}
                    onChange={() => toggleTarget(target.id)}
                    className="rounded bg-[#262626] border-[#404040] text-[#e50914] focus:ring-0"
                  />
                  <span className="text-[11px] text-[#cccccc]">Enabled</span>
                </label>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};
