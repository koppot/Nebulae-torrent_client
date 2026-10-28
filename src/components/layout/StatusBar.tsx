import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { useTorrentStore } from '../../stores/torrentStore';
import { useConnectivityStore } from '../../stores/connectivityStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { VpnStatus } from '../../types/vpn';
import { formatBytes, formatSpeed } from '../../lib/formatters';

// How far the last live probe of the ADC host got. No value means "offline":
// `unavailable` only says the probe got no connection and no tailnet answer
// just now, and `host_answered` says the host is up on the tailnet but does
// not accept :443 at the address checked. `reachable` is a TCP result on :443,
// not HTTPS or application health.
type AdcLink = 'reachable' | 'host_answered' | 'unavailable';

interface TailscaleStatus {
  connected: boolean;
  interface: string | null;
  tailnet_ip: string | null;
  adc_reachable?: boolean | null;
  adc_link?: AdcLink | null;
  adc_node?: string | null;
}

// The status comes from the embedded node unless it has not started, in which
// case the backend falls back to a system Tailscale interface.
const tailnetSource = (status: TailscaleStatus): string =>
  status.interface?.startsWith('nebulae-embedded')
    ? 'embedded node'
    : `system Tailscale${status.interface ? ` ${status.interface}` : ''}`;

const adcTooltip = (status: TailscaleStatus): string => {
  const head = `Tailnet connected (${tailnetSource(status)})`;
  switch (status.adc_link) {
    case 'reachable':
      return `${head} · ADC accepted a connection on port 443 (TCP check only, HTTPS not tested) · ${status.tailnet_ip ?? ''}`;
    case 'host_answered':
      return `${head} · the ADC host answered but does not accept connections on port 443 at the address checked. This is not an HTTPS test and does not mean ADC is down.`;
    case 'unavailable':
      return `${head} · the last check got no connection and no answer from ADC. That does not prove ADC is offline; it is checked again automatically.`;
    default:
      return status.adc_node
        ? `${head} · checking ADC…`
        : `${head} · no ADC node found in the tailnet map`;
  }
};

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
  const { settings, saveSettings } = useSettingsStore();

  const [isTogglingRacing, setIsTogglingRacing] = useState(false);

  const handleToggleRacingMode = async () => {
    if (isTogglingRacing) return;
    setIsTogglingRacing(true);
    try {
      await saveSettings({ racing_mode_enabled: !settings.racing_mode_enabled });
    } catch (err) {
      console.error('Failed to toggle racing mode:', err);
    } finally {
      setIsTogglingRacing(false);
    }
  };

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
    <footer className="min-h-[24px] h-auto py-1 px-3 bg-[#141414] border-t border-[#262626] flex flex-wrap items-center justify-between gap-x-3 gap-y-1 text-[11px] font-mono text-[#a3a3a3] select-none z-30">
      {/* Left: Connectivity, Port, Tailscale, VPN, Modes */}
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
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
          title={tailscale?.connected ? adcTooltip(tailscale) : 'Tailnet is not connected'}
        >
          <span
            className={`w-2 h-2 rounded-full ${
              !tailscale?.connected
                ? 'bg-[#333333]'
                : tailscale.adc_link === 'unavailable'
                  ? 'bg-amber-500 shadow-[0_0_6px_rgba(245,158,11,0.5)]'
                  : tailscale.adc_link === 'host_answered'
                    ? 'bg-amber-400 shadow-[0_0_6px_rgba(251,191,36,0.4)]'
                    : tailscale.adc_link === 'reachable'
                      ? 'bg-emerald-500 shadow-[0_0_8px_#10b981]'
                      : !tailscale.adc_node
                        ? 'bg-amber-500/60'
                        : 'bg-[#555555] animate-pulse'
            }`}
          />
          <span className={tailscale?.connected ? 'text-white' : ''}>Tailnet</span>
          {tailscale?.connected && tailscale.adc_link === 'unavailable' && (
            <span className="text-amber-500">· ADC unavailable (reachability unknown)</span>
          )}
          {tailscale?.connected && tailscale.adc_link === 'host_answered' && (
            <span className="text-amber-400">· ADC host up, port 443 not open</span>
          )}
          {tailscale?.connected && tailscale.adc_link == null && !tailscale.adc_node && (
            <span className="text-amber-500/80">· ADC unmapped</span>
          )}
          {tailscale?.connected && tailscale.adc_link == null && tailscale.adc_node && (
            <span className="text-[#a3a3a3]">· ADC verifying…</span>
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

        <button
          type="button"
          onClick={handleToggleRacingMode}
          disabled={isTogglingRacing}
          className="flex items-center gap-1.5 hover:text-white transition cursor-pointer disabled:opacity-50"
          title={
            settings.racing_mode_enabled
              ? 'Racing mode enabled (click to disable)'
              : 'Racing mode disabled (click to enable)'
          }
          aria-label={
            settings.racing_mode_enabled
              ? 'Disable racing mode'
              : 'Enable racing mode'
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
        </button>
      </div>

      {/* Right-aligned container: Global Throughput and Storage/System State */}
      <div className="ml-auto flex flex-wrap items-center justify-end gap-x-4 gap-y-1">
        {/* Center: Global Transfer Throughput */}
        <div className="flex items-center gap-x-3 gap-y-1">
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

        <span className="text-[#333333]">|</span>

        {/* Right: Storage & Application State */}
        <div className="flex items-center gap-x-3 gap-y-1">
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
      </div>
    </footer>
  );
};

export default StatusBar;
