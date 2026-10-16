import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useUiStore } from '../../stores/uiStore';
import { ShieldAlert, X } from 'lucide-react';

interface PortNegotiationResult {
  status: string;
  configured_port: number;
  tried_ports: number[];
  checked_at_epoch_ms: number;
}

/**
 * Owner directive: when the port negotiator finds no externally reachable
 * peer port, explain HOW to open a port in the router instead of failing
 * silently. Shown at most once per verdict (dismissal is remembered per
 * port + verdict timestamp, so a later failed negotiation shows again).
 */
export function PortForwardModal() {
  const showConnectivityDoctor = useUiStore((s) => s.showConnectivityDoctor);
  const setShowConnectivityDoctor = useUiStore((s) => s.setShowConnectivityDoctor);
  const [verdict, setVerdict] = useState<PortNegotiationResult | null>(null);
  const [dismissedKey, setDismissedKey] = useState<string>(
    () => localStorage.getItem('port_forward_dismissed') || ''
  );

  useEffect(() => {
    let alive = true;
    const check = async () => {
      try {
        const r = await invoke<PortNegotiationResult | null>('get_port_negotiation_result');
        if (alive && r && r.status === 'unreachable') setVerdict(r);
      } catch {
        // engine not ready yet — retried below
      }
    };
    // The negotiator probes one candidate every ~2-3 s at engine spawn;
    // give it time to finish before the first read.
    const t0 = window.setTimeout(check, 20_000);
    const t1 = window.setTimeout(check, 45_000);
    return () => {
      alive = false;
      window.clearTimeout(t0);
      window.clearTimeout(t1);
    };
  }, [showConnectivityDoctor]); // re-check after the doctor modal closes (user may have fixed the forward)

  if (!verdict) return null;
  const key = `${verdict.configured_port}:${verdict.checked_at_epoch_ms}`;
  if (dismissedKey === key) return null;

  const dismiss = () => {
    localStorage.setItem('port_forward_dismissed', key);
    setDismissedKey(key);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm p-4 font-sans select-none">
      <div className="relative w-full max-w-md bg-[#141414] border border-[#333333] rounded-xl shadow-2xl text-white">
        <div className="flex items-start gap-3 px-5 pt-4 pb-3 border-b border-[#333333] bg-[#1f1f1f] rounded-t-xl">
          <ShieldAlert className="w-5 h-5 text-[#e50914] flex-shrink-0 mt-0.5" />
          <div className="min-w-0 flex-1">
            <h2 className="text-sm font-bold text-white">Torrent port not reachable from the internet</h2>
            <p className="text-[11px] text-[#a3a3a3] mt-1">
              No router port mapping could be established automatically — other members cannot
              connect to you until a port is opened.
            </p>
          </div>
          <button onClick={dismiss} className="p-1 rounded text-[#a3a3a3] hover:text-white transition">
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="px-5 py-4 space-y-3 text-xs text-[#cccccc]">
          <p>
            Nebulae tried peer ports{' '}
            <span className="font-mono text-white">
              {verdict.tried_ports.join(', ') || String(verdict.configured_port)}
            </span>{' '}
            and every one is closed from the outside. Downloads can still work through outgoing
            connections, but seeding and many peer exchanges will be slow or fail.
          </p>
          <ol className="list-decimal ml-4 space-y-1.5 text-[#cccccc]">
            <li>
              Open your router's admin page (usually{' '}
              <span className="font-mono text-white">192.168.1.1</span> or{' '}
              <span className="font-mono text-white">192.168.0.1</span>) and sign in.
            </li>
            <li>
              Find the <span className="text-white font-semibold">Port Forwarding / Virtual Server</span>{' '}
              section.
            </li>
            <li>
              Create a rule for <span className="text-white font-semibold">TCP &amp; UDP</span> port{' '}
              <span className="font-mono text-white">{verdict.configured_port}</span> pointing at this
              Mac's local IP address.
            </li>
            <li>
              If the router offers <span className="text-white font-semibold">UPnP / NAT-PMP</span>,
              enabling it lets Nebulae open the port by itself next time.
            </li>
          </ol>
          <p className="text-[10px] text-[#737373]">
            Tip: make sure this Mac has a fixed local IP (DHCP reservation) so the forward keeps
            pointing at it.
          </p>
        </div>

        <div className="px-5 py-3 border-t border-[#333333] bg-[#1a1a1a] rounded-b-xl flex items-center justify-between gap-3">
          <button
            type="button"
            onClick={dismiss}
            className="px-3 py-1.5 rounded-md border border-[#333333] text-[11px] text-[#a3a3a3] hover:text-white hover:border-white transition"
          >
            Remind me next launch
          </button>
          <button
            type="button"
            onClick={() => {
              dismiss();
              setShowConnectivityDoctor(true);
            }}
            className="px-3 py-1.5 rounded-md bg-[#e50914] hover:bg-[#c4080f] text-[11px] font-semibold text-white transition"
          >
            Open Connectivity Doctor
          </button>
        </div>
      </div>
    </div>
  );
}
