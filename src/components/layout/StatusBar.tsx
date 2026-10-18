import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { useTorrentStore } from '../../stores/torrentStore';
import { useConnectivityStore } from '../../stores/connectivityStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { VpnStatus } from '../../types/vpn';
import { formatBytes, formatSpeed } from '../../lib/formatters';

interface TailscaleStatus {
  connected: boolean;
  interface: string | null;
  tailnet_ip: string | null;
  adc_reachable?: boolean | null;
  adc_node?: string | null;
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
  const { settings } = useSettingsStore();

  const [lastDoctorRun, setLastDoctorRun] = useState<number>(0);
  const [tailscale, setTailscale] = useState<TailscaleStatus | null>(null);
  const [vpn, setVpn] = useState<VpnStatus | null>(null);
  const vpnConfigured = vpn ? vpn.mode !== 'NoVpn' : false;
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

  // Egress source IP — the address the OS actually uses for outbound
  // connections (owner request: hovering "Connected" shows the current
  // address through which the client accesses other servers, NOT the
  // Tailscale address). Follows default-route/VPN changes automatically.
  const [egressIp, setEgressIp] = useState<string | null>(null);
  useEffect(() => {
    const resolve = () =>
      invoke<string | null>('get_egress_source_ip')
        .then((ip) => setEgressIp(ip ?? null))
        .catch(() => {});
    resolve();
    const timer = setInterval(resolve, 60 * 1000);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    const poll = () => invoke<VolumeSpace>('get_volume_disk_space').then(setDisk).catch(() => {});
    poll();
    const timer = setInterval(poll, 60 * 1000);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    const poll = () => invoke<VpnStatus>('get_vpn_status').then(setVpn).catch(() => {});
    poll();
    const timer = setInterval(poll, 15 * 1000);
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

  return (
    <footer className="h-6 bg-[#141414] border-t border-[#262626] flex items-center justify-between px-3 text-[11px] font-mono text-[#a3a3a3] select-none z-30">
      {/* Left: Connectivity, Port, Tailscale, VPN */}
      <div className="flex items-center gap-3">
        <div
          className="flex items-center gap-1.5"
          title={
            connected
              ? egressIp
                ? `Connected — outbound via ${egressIp}`
                : 'Connected'
              : 'Disconnected'
          }
        >
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

        <button
          type="button"
          onClick={() => {
            // Owner: the Connectivity Doctor / port window opens ONLY from
            // this click — never automatically on a closed port.
            window.dispatchEvent(new CustomEvent('nebulae:port-closed-info'));
          }}
          className="flex items-center hover:text-white transition cursor-pointer"
          title={`Inbound Listening Port ${port} — ${
            reportFresh
              ? portState === 'Open'
                ? 'verified reachable'
                : 'verified closed — click for port help'
              : 'not verified yet (doctor runs every 10 min) — click for port help'
          }`}
        >
          <span
            className={`w-2 h-2 rounded-full inline-block mr-1.5 ${
              !reportFresh
                ? 'bg-[#333333]'
                : portState === 'Open'
                  ? 'bg-emerald-500 shadow-[0_0_8px_#10b981]'
                  : 'bg-red-500 shadow-[0_0_6px_rgba(239,68,68,0.6)]'
            }`}
          />
          Port: {port}
        </button>

        <span className="text-[#333333]">|</span>

        <div
          className="flex items-center gap-1.5"
          title={
            tailscale?.connected
              ? tailscale.adc_reachable === false
                ? 'ADC host offline'
                : `Tailscale connected — IP ${tailscale.tailnet_ip ?? ''}`
              : 'Tailscale is not connected'
          }
        >
          <span
            className={`w-2 h-2 rounded-full ${
              tailscale?.connected
                ? tailscale.adc_reachable === false
                  ? 'bg-amber-500'
                  : 'bg-emerald-500'
                : 'bg-[#333333]'
            }`}
          />
          <span className={tailscale?.connected ? 'text-white' : ''}>Tailscale</span>
          {tailscale?.connected && tailscale.adc_reachable === false && (
            <span className="text-amber-500">· ADC host offline</span>
          )}
        </div>

        <span className="text-[#333333]">|</span>

        <div
          className={`flex items-center gap-1.5 ${vpnConfigured ? '' : 'opacity-40'}`}
          title={
            !vpnConfigured
              ? 'No VPN profile configured'
              : vpn?.connected
                ? `VPN active on ${vpn.interface_name} (${vpn.assigned_ip})`
                : 'VPN profile configured, but not connected'
          }
        >
          <span
            className={`w-2 h-2 rounded-full ${
              !vpnConfigured
                ? 'bg-[#333333]'
                : vpn?.connected
                  ? 'bg-emerald-500'
                  : 'bg-red-500'
            }`}
          />
          <span className={vpnConfigured && vpn?.connected ? 'text-white' : ''}>
            VPN: {vpnConfigured ? (vpn?.connected ? vpn?.assigned_ip || 'connected' : 'disconnected') : 'not configured'}
          </span>
        </div>

        <span className="text-[#333333]">|</span>

        <div
          className="flex items-center gap-1.5"
          title={
            settings.racing_mode_enabled
              ? 'Racing mode active — aggressive peer limits, high bandwidth priority, large disk cache'
              : 'Racing mode disabled'
          }
        >
          <span
            className={`w-2 h-2 rounded-full transition-colors ${
              settings.racing_mode_enabled
                ? 'bg-emerald-500 shadow-[0_0_8px_#10b981]'
                : 'bg-red-500 shadow-[0_0_6px_rgba(239,68,68,0.6)]'
            }`}
          />
          <span className={settings.racing_mode_enabled ? 'text-white' : 'text-[#a3a3a3]'}>
            Racing mode
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
