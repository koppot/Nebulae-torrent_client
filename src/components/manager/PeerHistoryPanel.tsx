import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-shell';
import { Loader2, Trash2, X } from 'lucide-react';
import type {
  PeerAddr,
  PeerHistoryEntry,
  PeerTelemetry,
  SwarmMember,
  TorrentTrackerItem,
} from '../../types';
import { formatBytes } from '../../lib/formatters';
import { getCountryName } from '../../utils/countryNames';
import {
  getCooldownRemaining,
  getResolvedSwarm,
  markSwarmLookupFailed,
  registerCooldownListener,
  storeSwarmResults,
} from '../../utils/swarmMembersCache';
import {
  isAdcTracker,
  isValidAdcProfileUrl,
  lookupSwarmMembers,
  normalizeProfileUrl,
} from '../../utils/adcSwarmMembersIpc';
import { useTrackerStore } from '../../stores/trackerStore';

interface BlockedPeerIp {
  ip: string;
  blocked_at_epoch_ms: number;
}

type PeerBlockCapability = 'Unsupported' | 'RepeatedDisconnect';

interface PeerHistoryPanelProps {
  torrentId: string;
  torrentHash: string;
  peers: PeerTelemetry[];
  trackers: TorrentTrackerItem[];
  isLoading: boolean;
}

interface HistoryRow {
  key: string;
  ip: string;
  port: number;
  history?: PeerHistoryEntry;
  live?: PeerTelemetry;
  member?: SwarmMember;
}

const endpointKey = (ip: string, port: number) => `${ip.toLowerCase()}|${port}`;

export const PeerHistoryPanel: React.FC<PeerHistoryPanelProps> = ({
  torrentId,
  torrentHash,
  peers,
  trackers,
  isLoading,
}) => {
  const [capability, setCapability] = useState<PeerBlockCapability | null>(null);
  const [blockedPeers, setBlockedPeers] = useState<BlockedPeerIp[]>([]);
  const [history, setHistory] = useState<PeerHistoryEntry[]>([]);
  const [liveMembers, setLiveMembers] = useState<Map<string, SwarmMember>>(new Map());
  const [busyIp, setBusyIp] = useState<string | null>(null);
  const [isClearingHistory, setIsClearingHistory] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [cooldownTrigger, setCooldownTrigger] = useState(0);
  const credentialRevision = useTrackerStore((state) => state.credentialRevision);
  const identityRef = useRef<string | null>(null);
  const isMounted = useRef(true);

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

  useEffect(() => {
    isMounted.current = true;
    return () => {
      isMounted.current = false;
    };
  }, []);

  useEffect(() => {
    return registerCooldownListener(() => {
      setCooldownTrigger((value) => value + 1);
    });
  }, []);

  const hasAdcTracker = trackers.some((tracker) => isAdcTracker(tracker.url));

  useEffect(() => {
    const identity = `${torrentHash}:${credentialRevision}:${hasAdcTracker}`;
    const identityChanged = identityRef.current !== identity;
    identityRef.current = identity;

    if (!hasAdcTracker || peers.length === 0) {
      if (identityChanged || !hasAdcTracker) setLiveMembers(new Map());
      return;
    }

    const peerAddrs: PeerAddr[] = peers.map(({ ip, port }) => ({ ip, port }));
    const resolved = getResolvedSwarm(torrentHash, credentialRevision, peerAddrs);
    setLiveMembers((previous) => {
      const next = identityChanged ? new Map<string, SwarmMember>() : new Map(previous);
      for (const [key, member] of resolved.cachedMap) next.set(key, member);
      return next;
    });

    if (resolved.unresolvedPeers.length === 0 || getCooldownRemaining() > 0) return;
    const batch = resolved.unresolvedPeers.slice(0, 100);
    void lookupSwarmMembers(
      torrentHash,
      batch,
      undefined,
      String(credentialRevision),
    ).then((lookup) => {
      if (!isMounted.current || identityRef.current !== identity) return;
      if (lookup.kind === 'ipc_error') {
        markSwarmLookupFailed(torrentHash, credentialRevision, batch);
        return;
      }
      storeSwarmResults(torrentHash, credentialRevision, batch, lookup.result);
      const current = getResolvedSwarm(torrentHash, credentialRevision, peerAddrs);
      if (current.cachedMap.size > 0) {
        setLiveMembers((previous) => new Map([...previous, ...current.cachedMap]));
      }
    });
  }, [torrentHash, peers, hasAdcTracker, credentialRevision, cooldownTrigger]);

  const rows = useMemo(() => {
    const merged = new Map<string, HistoryRow>();
    for (const entry of history) {
      const key = endpointKey(entry.ip, entry.port);
      merged.set(key, { key, ip: entry.ip, port: entry.port, history: entry });
    }
    for (const peer of peers) {
      const key = endpointKey(peer.ip, peer.port);
      const existing = merged.get(key);
      merged.set(key, {
        key,
        ip: peer.ip,
        port: peer.port,
        history: existing?.history,
        live: peer,
        member: liveMembers.get(`${peer.ip}:${peer.port}`),
      });
    }
    return [...merged.values()].sort((a, b) => {
      if (Boolean(a.live) !== Boolean(b.live)) return a.live ? -1 : 1;
      return (b.history?.last_seen_epoch_ms ?? 0) - (a.history?.last_seen_epoch_ms ?? 0);
    });
  }, [history, peers, liveMembers]);

  const blockedIps = useMemo(() => new Set(blockedPeers.map((peer) => peer.ip)), [blockedPeers]);

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

  const openProfile = async (member: SwarmMember | undefined, entry: PeerHistoryEntry | undefined) => {
    const userId = member?.user_id ?? entry?.adc_user_id ?? undefined;
    const url = normalizeProfileUrl(member?.profile_url ?? entry?.adc_profile_url, userId);
    if (!isValidAdcProfileUrl(url)) return;
    try {
      await open(url);
    } catch {
      setNotice('Could not open the ADC profile link.');
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col gap-3 font-sans">
      {notice && (
        <div className="rounded border border-[#333333] bg-[#1f1f1f] px-2.5 py-1.5 text-[11px] text-[#cccccc]">
          {notice}
        </div>
      )}

      <div className="min-h-0 flex-1 space-y-3 overflow-auto">
        <section>
          <div className="mb-1 flex items-center justify-between gap-2">
            <div>
              <h3 className="text-[11px] font-semibold text-white">Peer history</h3>
              <p className="text-[10px] text-[#777]">
                {peers.length} connected now · {history.length} saved endpoints
              </p>
            </div>
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

          <div className="mb-1 grid grid-cols-[minmax(110px,1.1fr)_minmax(120px,1fr)_minmax(90px,0.7fr)_minmax(90px,0.7fr)] gap-2 px-2 text-[9px] uppercase text-[#777]">
            <span>Peer / client</span>
            <span>ADC user</span>
            <span>Country</span>
            <span>Connection</span>
          </div>

          {isLoading && rows.length === 0 ? (
            <div className="flex items-center gap-2 py-3 text-[11px] text-[#a3a3a3]">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              Loading peers…
            </div>
          ) : rows.length === 0 ? (
            <p className="py-2 text-[11px] text-[#a3a3a3]">
              No connected peers or saved observations for this torrent.
            </p>
          ) : (
            <div className="space-y-1">
              {rows.map((row) => {
                const isBlocked = blockedIps.has(row.ip);
                const member = row.member;
                const savedEntry = row.history;
                const username = member?.username ?? savedEntry?.adc_username;
                const userId = member?.user_id ?? savedEntry?.adc_user_id ?? undefined;
                const profileUrl = normalizeProfileUrl(
                  member?.profile_url ?? savedEntry?.adc_profile_url,
                  userId,
                );
                const matchTime = member?.matched_at_epoch_ms ?? savedEntry?.adc_matched_at_epoch_ms;
                const matchAge = matchTime ? Date.now() - matchTime : Number.POSITIVE_INFINITY;
                const recentMatch = Boolean(
                  member &&
                  matchAge >= 0 &&
                  matchAge <= 30 * 60 * 1000,
                );
                const countryCode = row.live?.country_code ?? savedEntry?.country_code;

                return (
                  <div
                    key={row.key}
                    className="rounded border border-[#262626] bg-[#1a1a1a] px-2 py-1.5"
                  >
                    <div className="grid grid-cols-[minmax(110px,1.1fr)_minmax(120px,1fr)_minmax(90px,0.7fr)_minmax(90px,0.7fr)] items-start gap-2">
                      <div className="min-w-0">
                        <span className="block truncate font-mono text-[10px] text-white">
                          {row.ip}:{row.port}
                        </span>
                        <span className="block truncate text-[10px] text-[#888]">
                          {row.live?.client_name || savedEntry?.client_name || 'Unknown client'}
                        </span>
                      </div>

                      <div className="min-w-0">
                        {username ? (
                          <button
                            type="button"
                            onClick={() => void openProfile(member, savedEntry)}
                            className="block max-w-full truncate text-left text-[10px] font-semibold text-[#e50914] hover:underline"
                            title={profileUrl}
                          >
                            {username}
                          </button>
                        ) : (
                          <span className="text-[10px] text-[#555]">—</span>
                        )}
                        {username && (
                          <span className="block truncate text-[9px] text-[#777]">
                            {recentMatch ? 'Recent ADC match' : 'Saved identity · not re-verified'}
                            {userId ? ` · ID ${userId}` : ''}
                          </span>
                        )}
                      </div>

                      <span className="truncate text-[10px] text-[#cccccc]">
                        {countryCode ? getCountryName(countryCode) : '—'}
                      </span>

                      <div className="flex min-w-0 flex-col items-start gap-1">
                        <span className="truncate text-[10px] text-[#a3a3a3]">
                          {row.live ? (isBlocked ? 'Blocked now' : 'Connected now') : 'Disconnected'}
                        </span>
                        {row.live && !isBlocked && (
                          <button
                            type="button"
                            disabled={capability !== 'RepeatedDisconnect' || busyIp !== null}
                            onClick={() => void block(row.live!)}
                            className="rounded border border-[#444] px-1.5 py-0.5 text-[9px] text-[#cccccc] hover:text-white disabled:cursor-not-allowed disabled:opacity-40"
                          >
                            {busyIp === row.ip ? 'Blocking…' : 'Block IP'}
                          </button>
                        )}
                      </div>
                    </div>

                    <div className="mt-1 flex flex-wrap gap-x-3 text-[9px] text-[#888]">
                      {savedEntry ? (
                        <>
                          <span>First {new Date(savedEntry.first_seen_epoch_ms).toLocaleString()}</span>
                          <span>Last {new Date(savedEntry.last_seen_epoch_ms).toLocaleString()}</span>
                          <span>Observed ↓ {formatBytes(savedEntry.downloaded_delta_bytes)}</span>
                          <span>Observed ↑ {formatBytes(savedEntry.uploaded_delta_bytes)}</span>
                          {savedEntry.counter_resets > 0 && (
                            <span>
                              {savedEntry.counter_resets} counter reset{savedEntry.counter_resets === 1 ? '' : 's'}
                            </span>
                          )}
                        </>
                      ) : (
                        <span>History will appear after the next background sample.</span>
                      )}
                      {row.live && (
                        <>
                          <span>Live total ↓ {formatBytes(row.live.total_downloaded_bytes)}</span>
                          <span>Live total ↑ {formatBytes(row.live.total_uploaded_bytes)}</span>
                        </>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </section>
      </div>

      <div className="max-h-32 overflow-auto border-t border-[#262626] pt-2">
        <div className="mb-1 flex flex-wrap items-baseline justify-between gap-x-3">
          <h3 className="text-[11px] font-semibold text-white">Blocked IPs</h3>
          <p className="text-[9px] text-[#777]">
            Global across torrents, not a firewall rule.
          </p>
        </div>
        {capability === 'Unsupported' && (
          <p className="mb-1 text-[10px] text-[#a3a3a3]">
            The active engine cannot enforce peer blocks. Existing entries are retained but inactive.
          </p>
        )}
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
