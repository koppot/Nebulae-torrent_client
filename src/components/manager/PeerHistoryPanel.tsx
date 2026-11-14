import React, { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Loader2, ShieldCheck, Trash2, X } from 'lucide-react';
import type { PeerHistoryEntry, PeerTelemetry } from '../../types';
import { formatBytes } from '../../lib/formatters';

interface BlockedPeerIp {
  ip: string;
  blocked_at_epoch_ms: number;
}

type PeerBlockCapability = 'Unsupported' | 'RepeatedDisconnect';

interface PeerHistoryPanelProps {
  torrentId: string;
  torrentHash: string;
  peers: PeerTelemetry[];
  isLoading: boolean;
}

export const PeerHistoryPanel: React.FC<PeerHistoryPanelProps> = ({
  torrentId,
  torrentHash,
  peers,
  isLoading,
}) => {
  const [capability, setCapability] = useState<PeerBlockCapability | null>(null);
  const [blockedPeers, setBlockedPeers] = useState<BlockedPeerIp[]>([]);
  const [history, setHistory] = useState<PeerHistoryEntry[]>([]);
  const [busyIp, setBusyIp] = useState<string | null>(null);
  const [isClearingHistory, setIsClearingHistory] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [nextCapability, nextBlocks, nextHistory] = await Promise.all([
        invoke<PeerBlockCapability>('get_peer_block_capability'),
        invoke<BlockedPeerIp[]>('list_blocked_peer_ips'),
        invoke<PeerHistoryEntry[]>('get_peer_history', { infoHash: torrentHash }),
      ]);
      setCapability(nextCapability);
      setBlockedPeers(nextBlocks);
      setHistory(nextHistory);
    } catch (error) {
      setNotice(String(error));
    }
  }, [torrentHash]);

  useEffect(() => {
    void refresh();
    const interval = setInterval(() => void refresh(), 5000);
    return () => clearInterval(interval);
  }, [refresh]);

  const blockedIps = new Set(blockedPeers.map((peer) => peer.ip));

  const block = async (peer: PeerTelemetry) => {
    setBusyIp(peer.ip);
    setNotice(null);
    try {
      await invoke('block_peer_ip', {
        torrentId,
        ip: peer.ip,
        port: peer.port,
      });
      setNotice(`${peer.ip} blocked globally across torrents.`);
      await refresh();
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusyIp(null);
    }
  };

  const unblock = async (ip: string) => {
    setBusyIp(ip);
    setNotice(null);
    try {
      await invoke('unblock_peer_ip', { ip });
      setNotice(`${ip} removed from the global block list.`);
      await refresh();
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusyIp(null);
    }
  };

  const clearHistory = async () => {
    if (isClearingHistory) return;
    if (!window.confirm('Clear the saved peer history for this torrent?')) return;
    setIsClearingHistory(true);
    setNotice(null);
    try {
      await invoke<number>('clear_peer_history', { infoHash: torrentHash });
      setHistory([]);
      setNotice('Peer history cleared. New observations resume while Nebulae is running.');
      await refresh();
    } catch (error) {
      setNotice(String(error));
    } finally {
      setIsClearingHistory(false);
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col gap-3 font-sans">
      <div className="rounded border border-amber-500/30 bg-amber-500/5 px-3 py-2 text-[11px] text-amber-200">
        <div className="flex items-center gap-1.5 font-semibold">
          <ShieldCheck className="h-3.5 w-3.5" />
          Global IP blocks
        </div>
        <p className="mt-1 text-amber-100/80">
          Blocks apply to every torrent. Nebulae disconnects matching peers immediately and
          re-checks every 10 seconds. This is not a firewall rule, so a peer may briefly reconnect.
        </p>
        <p className="mt-1 text-amber-100/80">
          Local history is per torrent and stores only peer IP:port, observed times, byte deltas, and
          block state. It never stores ADC members/profiles, client names, or geo/ASN metadata.
          It records connected peers every 30 seconds while Nebulae is running; entries expire after
          90 days.
        </p>
        {capability === 'Unsupported' && (
          <p className="mt-1 font-semibold">
            The active engine cannot enforce peer blocks. Existing entries are retained but inactive.
          </p>
        )}
      </div>

      {notice && (
        <div className="rounded border border-[#333333] bg-[#1f1f1f] px-2.5 py-1.5 text-[11px] text-[#cccccc]">
          {notice}
        </div>
      )}

      <div className="min-h-0 flex-1 space-y-3 overflow-auto">
        <section>
          <h3 className="mb-1 text-[11px] font-semibold text-white">Connected peers for this torrent</h3>
          {isLoading && peers.length === 0 ? (
            <div className="flex items-center gap-2 py-3 text-[11px] text-[#a3a3a3]">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              Loading peers…
            </div>
          ) : peers.length === 0 ? (
            <p className="py-2 text-[11px] text-[#a3a3a3]">No connected peers to show.</p>
          ) : (
            <div className="space-y-1">
              {peers.map((peer) => {
                const isBlocked = blockedIps.has(peer.ip);
                return (
                  <div
                    key={`${peer.ip}:${peer.port}`}
                    className="flex items-center justify-between gap-3 rounded border border-[#262626] bg-[#1a1a1a] px-2 py-1.5"
                  >
                    <div className="min-w-0">
                      <span className="block truncate font-mono text-[11px] text-white">
                        {peer.ip}:{peer.port}
                      </span>
                      <span className="block truncate text-[10px] text-[#a3a3a3]">
                        {peer.client_name} · ↓ {formatBytes(peer.total_downloaded_bytes)} · ↑{' '}
                        {formatBytes(peer.total_uploaded_bytes)}
                      </span>
                    </div>
                    {isBlocked ? (
                      <span className="shrink-0 text-[10px] font-semibold text-amber-400">Blocked</span>
                    ) : (
                      <button
                        type="button"
                        disabled={capability !== 'RepeatedDisconnect' || busyIp !== null}
                        onClick={() => void block(peer)}
                        className="shrink-0 rounded border border-[#444] px-2 py-1 text-[10px] text-[#cccccc] hover:text-white disabled:cursor-not-allowed disabled:opacity-40"
                      >
                        {busyIp === peer.ip ? 'Blocking…' : 'Block IP'}
                      </button>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </section>

        <section className="border-t border-[#262626] pt-2">
          <div className="mb-1 flex items-center justify-between gap-2">
            <h3 className="text-[11px] font-semibold text-white">Peer history</h3>
            <button
              type="button"
              disabled={isClearingHistory}
              onClick={() => void clearHistory()}
              className="flex items-center gap-1 rounded border border-[#444] px-2 py-1 text-[10px] text-[#cccccc] hover:text-white disabled:opacity-40"
            >
              {isClearingHistory ? <Loader2 className="h-3 w-3 animate-spin" /> : <Trash2 className="h-3 w-3" />}
              Clear this torrent
            </button>
          </div>
          {history.length === 0 ? (
            <p className="py-2 text-[10px] text-[#a3a3a3]">
              No observations yet. Up to 500 endpoints are shown; the local database is capped at 100,000 rows.
            </p>
          ) : (
            <div className="space-y-1">
              {history.map((entry) => (
                <div
                  key={entry.peer_key}
                  className="rounded border border-[#262626] bg-[#1a1a1a] px-2 py-1.5"
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate font-mono text-[10px] text-white">
                      {entry.ip}:{entry.port}
                    </span>
                    <span className="shrink-0 text-[10px] text-[#a3a3a3]">
                      {entry.currently_blocked
                        ? 'Blocked now'
                        : entry.was_blocked
                          ? 'Blocked before'
                          : 'Not blocked'}
                    </span>
                  </div>
                  <div className="mt-0.5 flex flex-wrap gap-x-3 text-[10px] text-[#a3a3a3]">
                    <span>First {new Date(entry.first_seen_epoch_ms).toLocaleString()}</span>
                    <span>Last {new Date(entry.last_seen_epoch_ms).toLocaleString()}</span>
                    <span>↓ {formatBytes(entry.downloaded_delta_bytes)}</span>
                    <span>↑ {formatBytes(entry.uploaded_delta_bytes)}</span>
                    {entry.counter_resets > 0 && (
                      <span>{entry.counter_resets} counter reset{entry.counter_resets === 1 ? '' : 's'}</span>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>
      </div>

      <div className="max-h-24 overflow-auto border-t border-[#262626] pt-2">
        <h3 className="mb-1 text-[11px] font-semibold text-white">Blocked IPs</h3>
        {blockedPeers.length === 0 ? (
          <p className="text-[10px] text-[#a3a3a3]">No manually blocked IPs.</p>
        ) : (
          <div className="space-y-1">
            {blockedPeers.map((peer) => (
              <div key={peer.ip} className="flex items-center justify-between gap-2 text-[10px]">
                <span className="font-mono text-[#cccccc]">{peer.ip}</span>
                <span className="ml-auto text-[#737373]">
                  Since {new Date(peer.blocked_at_epoch_ms).toLocaleString()}
                </span>
                <button
                  type="button"
                  disabled={busyIp !== null}
                  onClick={() => void unblock(peer.ip)}
                  className="flex shrink-0 items-center gap-1 rounded border border-[#444] px-2 py-1 text-[#cccccc] hover:text-white disabled:opacity-40"
                >
                  {busyIp === peer.ip ? <Loader2 className="h-3 w-3 animate-spin" /> : <X className="h-3 w-3" />}
                  Unblock
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};
