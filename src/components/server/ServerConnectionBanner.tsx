import React, { useState } from 'react';
import { CheckCircle2, Loader2, Lock, RefreshCw, WifiOff } from 'lucide-react';
import { useRemoteServerStore } from '../../stores/remoteServerStore';
import { useUiStore } from '../../stores/uiStore';

/** Connection state of the configured Nebulae Server, shown above the Server tab's Manager UI. */
export const ServerConnectionBanner: React.FC = () => {
  const { connState, errorMessage, serverStatus, lastSuccessAt } = useRemoteServerStore();
  const [isRefreshing, setIsRefreshing] = useState(false);
  const { setShowSettings, setSettingsTab } = useUiStore();
  const onRefresh = () => {
    setIsRefreshing(true);
    void useRemoteServerStore
      .getState()
      .refresh()
      .finally(() => setIsRefreshing(false));
  };
  const openSettings = () => {
    setSettingsTab('rtorrent');
    setShowSettings(true);
  };

  if (connState === 'connected' && serverStatus) {
    const ago = lastSuccessAt ? Math.round((Date.now() - lastSuccessAt) / 1000) : 0;
    return (
      <div className="flex items-center gap-2 px-3 py-1 bg-emerald-950/40 border-b border-emerald-800/30 text-[11px] flex-shrink-0 overflow-hidden">
        <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400 flex-shrink-0" />
        <span className="text-emerald-300 font-medium flex-shrink-0">Connected</span>
        <span className="text-[#a3a3a3] truncate">
          {serverStatus.name} {serverStatus.version} · engine: {serverStatus.engine}
        </span>
        {serverStatus.kill_switch_engaged && (
          <span className="flex-shrink-0 px-1.5 py-0.5 rounded bg-amber-900/60 border border-amber-700/50 text-amber-300 text-[10px] font-semibold uppercase">
            Kill switch active
          </span>
        )}
        <span className="ml-auto text-[#555] flex-shrink-0 tabular-nums">
          {ago < 10 ? 'just now' : `${ago}s ago`}
        </span>
        <button
          onClick={onRefresh}
          disabled={isRefreshing}
          className="p-0.5 rounded hover:bg-white/10 text-[#a3a3a3] hover:text-white transition disabled:opacity-40 flex-shrink-0"
          title="Refresh now"
        >
          <RefreshCw className={`w-3 h-3 ${isRefreshing ? 'animate-spin' : ''}`} />
        </button>
      </div>
    );
  }

  if (connState === 'connecting') {
    return (
      <div className="flex items-center gap-2 px-3 py-1 bg-[#1a1a1a] border-b border-[#262626] text-[11px] flex-shrink-0">
        <Loader2 className="w-3.5 h-3.5 text-[#a3a3a3] animate-spin flex-shrink-0" />
        <span className="text-[#a3a3a3]">Connecting to remote server…</span>
      </div>
    );
  }

  if (connState === 'auth_error') {
    return (
      <div className="flex items-center gap-2 px-3 py-1 bg-red-950/40 border-b border-red-900/40 text-[11px] flex-shrink-0">
        <Lock className="w-3.5 h-3.5 text-red-400 flex-shrink-0" />
        <span className="text-red-300 font-medium flex-shrink-0">Authentication failed</span>
        <span className="text-[#a3a3a3] truncate">{errorMessage}</span>
        <button
          onClick={openSettings}
          className="ml-auto px-2 py-0.5 rounded bg-[#2a2a2a] border border-[#333333] text-white text-[10px] hover:bg-[#333333] transition flex-shrink-0"
        >
          Open Settings
        </button>
      </div>
    );
  }

  if (connState === 'error') {
    return (
      <div className="flex items-center gap-2 px-3 py-1 bg-red-950/30 border-b border-red-900/30 text-[11px] flex-shrink-0">
        <WifiOff className="w-3.5 h-3.5 text-red-400 flex-shrink-0" />
        <span className="text-red-300 font-medium flex-shrink-0">Disconnected</span>
        <span className="text-[#a3a3a3] truncate">{errorMessage}</span>
        <button
          onClick={onRefresh}
          disabled={isRefreshing}
          className="ml-auto px-2 py-0.5 rounded bg-[#2a2a2a] border border-[#333333] text-white text-[10px] hover:bg-[#333333] transition disabled:opacity-40 flex-shrink-0"
        >
          Retry
        </button>
      </div>
    );
  }

  return null;
};
