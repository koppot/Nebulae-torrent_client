import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { useTorrentStore } from '../../stores/torrentStore';
import { useConnectivityStore } from '../../stores/connectivityStore';
import { formatBytes, formatSpeed } from '../../lib/formatters';

interface TailscaleStatus {
  connected: boolean;
  interface: string | null;
  tailnet_ip: string | null;
}

interface VolumeSpace {
  mount_point: string;
  total_bytes: number;
  available_bytes: number;
  used_bytes: number;
  used_percent: number;
}

const REPORT_FRESH_MS = 15 * 60 * 1000;

export const StatusBar: React.FC = () => {
  const { torrents } = useTorrentStore();
  const { report, runDoctorTest } = useConnectivityStore();

  const [lastDoctorRun, setLastDoctorRun] = useState<number>(0);
  const [tailscale, setTailscale] = useState<TailscaleStatus | null>(null);
  const [disk, setDisk] = useState<VolumeSpace | null>(null);
  const [appVersion, setAppVersion] = useState<string>('');

  useEffect(() => {
    getVersion().then((v) => setAppVersion(v)).catch(() => {});
  }, []);

  const totalDlSpeed = torrents.reduce((acc, t) => acc + (t.download_rate || 0), 0);
  const totalUlSpeed = torrents.reduce((acc, t) => acc + (t.upload_rate || 0), 0);
  const totalDownloaded = torrents.reduce((acc, t) => acc + (t.downloaded_bytes || 0), 0);
  const totalUploaded = torrents.reduce((acc, t) => acc + (t.uploaded_bytes || 0), 0);

  // Doctor probes (port reachability, NAT mapping) are heavier, so run on
  // mount and refresh on a slow cadence; everything else polls faster.
  useEffect(() => {
    runDoctorTest()
      .then(() => setLastDoctorRun(Date.now()))
      .catch(() => {});
    const doctorTimer = setInterval(() => {
      runDoctorTest()
        .then(() => setLastDoctorRun(Date.now()))
        .catch(() => {});
    }, 10 * 60 * 1000);
    return () => clearInterval(doctorTimer);
  }, [runDoctorTest]);

  useEffect(() => {
    const poll = () => invoke<TailscaleStatus>('get_tailscale_status').then(setTailscale).catch(() => {});
    poll();
    const timer = setInterval(poll, 15 * 1000);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    const poll = () => invoke<VolumeSpace>('get_volume_disk_space').then(setDisk).catch(() => {});
    poll();
    const timer = setInterval(poll, 60 * 1000);
    return () => clearInterval(timer);
  }, []);

  const reportFresh = !!lastDoctorRun && Date.now() - lastDoctorRun < REPORT_FRESH_MS;
  const port = report?.listening_port || 41413;
  const portState = reportFresh ? (report?.port_status === 'Open' ? 'Open' : 'Closed') : '—';
  const isPortOpen = portState === 'Open';

  // Connectivity dot: an authoritative fresh doctor report wins; otherwise
  // fall back to live swarm activity so a healthy seeding torrent never
  // reads as "Disconnected".
  const swarmActive = torrents.some(
    (t) =>
      (t.status === 'Seeding' || t.status === 'Downloading') &&
      (t.peers > 0 || t.trackers?.some((tr) => tr.status === 'Working') || (t.seeds ?? 0) > 0)
  );
  const connected = isPortOpen || swarmActive;
  const connectionBasis = isPortOpen
    ? 'Inbound port verified reachable'
    : swarmActive
      ? 'Swarm activity live (port not re-verified yet — runs the doctor every 10 min)'
      : 'No port verification and no swarm activity';

  return (
    <footer className="h-6 bg-[#141414] border-t border-[#262626] flex items-center justify-between px-3 text-[11px] font-mono text-[#a3a3a3] select-none z-30">
      {/* Left: Swarm Connectivity, DHT, Port, Tailscale */}
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-1.5" title={connectionBasis}>
          <span
            className={`w-2 h-2 rounded-full transition-colors ${
              connected
                ? 'bg-emerald-500 shadow-[0_0_8px_#10b981]'
                : 'bg-red-500 shadow-[0_0_6px_rgba(239,68,68,0.6)] animate-pulse'
            }`}
          />
          <span className="text-white font-sans text-[11px]">
            {connected ? 'Connected' : 'Disconnected'}
          </span>
        </div>

        <span className="text-[#333333]">|</span>

        <span
          title="Private tracker swarm: DHT/PEX/LPD are disabled at the engine and can never be enabled for ADC transfers"
        >
          DHT: Off · private
        </span>

        <span className="text-[#333333]">|</span>

        <span title="Inbound Listening Port">
          Port: {port} <span className="text-white">[{portState}]</span>
        </span>

        <span className="text-[#333333]">|</span>

        <div
          className="flex items-center gap-1.5"
          title={
            tailscale?.connected
              ? `Secure Network active on ${tailscale.interface} (${tailscale.tailnet_ip})`
              : 'Secure Network (Tailscale) not detected'
          }
        >
          <span
            className={`w-2 h-2 rounded-full ${
              tailscale?.connected ? 'bg-emerald-500' : 'bg-[#333333]'
            }`}
          />
          <span className={tailscale?.connected ? 'text-white' : ''}>
            Tailscale: {tailscale?.connected ? tailscale.tailnet_ip : '—'}
          </span>
        </div>
      </div>

      {/* Center: Global Transfer Throughput */}
      <div className="flex items-center gap-4">
        <div
          className="flex items-center gap-1.5"
          title="Global Download Rate & Total Downloaded"
        >
          <span
            className={`font-bold transition-colors ${
              totalDlSpeed > 0 ? 'text-emerald-500' : 'text-[#e50914]'
            }`}
          >
            ▼
          </span>
          <span
            className={`transition-colors ${
              totalDlSpeed > 0 ? 'text-emerald-400 font-semibold' : 'text-white'
            }`}
          >
            {formatSpeed(totalDlSpeed)}
          </span>
          <span className="text-[#a3a3a3]">({formatBytes(totalDownloaded)})</span>
        </div>

        <span className="text-[#333333]">|</span>

        <div
          className="flex items-center gap-1.5"
          title="Global Upload Rate & Total Uploaded"
        >
          <span
            className={`font-bold transition-colors ${
              totalUlSpeed > 0 ? 'text-emerald-500' : 'text-[#e50914]'
            }`}
          >
            ▲
          </span>
          <span
            className={`transition-colors ${
              totalUlSpeed > 0 ? 'text-emerald-400 font-semibold' : 'text-white'
            }`}
          >
            {formatSpeed(totalUlSpeed)}
          </span>
          <span className="text-[#a3a3a3]">({formatBytes(totalUploaded)})</span>
        </div>
      </div>

      {/* Right: Storage & Application State */}
      <div className="flex items-center gap-3">
        <span title={disk ? `Volume ${disk.mount_point}` : 'Available storage'}>
          Disk: {disk ? formatBytes(disk.available_bytes) : '…'} free
        </span>

        <span className="text-[#333333]">|</span>

        <span className="text-[#a3a3a3]" title="Protocol: BitTorrent Dual-Stack uTP/TCP">
          uTP + TCP
        </span>

        <span className="text-[#333333]">|</span>

        <span className="text-[#a3a3a3]" title="Nebulae application version">
          Nebulae v{appVersion || '…'}
        </span>
      </div>
    </footer>
  );
};

export default StatusBar;
