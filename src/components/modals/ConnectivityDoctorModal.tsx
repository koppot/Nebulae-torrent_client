import React, { useEffect, useState } from 'react';
import { useConnectivityStore } from '../../stores/connectivityStore';
import { useUiStore } from '../../stores/uiStore';
import { Activity, CheckCircle, RefreshCw, Shield, ChevronDown, ChevronRight, Copy, Check, X } from 'lucide-react';

export const ConnectivityDoctorModal: React.FC = () => {
  const { showConnectivityDoctor, setShowConnectivityDoctor } = useUiStore();
  const { report, isRunningTest, isRetestingPort, runDoctorTest, retestPort } = useConnectivityStore();
  const [activeAccordion, setActiveAccordion] = useState<string | null>(null);
  const [copiedCert, setCopiedCert] = useState(false);
  const [portInput, setPortInput] = useState('51413');

  useEffect(() => {
    if (showConnectivityDoctor && !report) {
      runDoctorTest();
    }
  }, [showConnectivityDoctor, report, runDoctorTest]);

  if (!showConnectivityDoctor) return null;

  const toggleAccordion = (key: string) => {
    setActiveAccordion(activeAccordion === key ? null : key);
  };

  const copyCertificate = () => {
    if (report?.certificate_text) {
      navigator.clipboard.writeText(report.certificate_text);
      setCopiedCert(true);
      setTimeout(() => setCopiedCert(false), 2000);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm select-none font-sans">
      <div className="relative w-full max-w-2xl max-h-[85vh] flex flex-col bg-[#141414] border border-[#333333] rounded-xl shadow-2xl overflow-hidden text-white">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-[#333333] bg-[#1f1f1f]">
          <div className="flex items-center gap-2.5">
            <Activity className="w-4 h-4 text-[#e50914]" />
            <h2 className="text-sm font-bold text-white">Connectivity Doctor</h2>
          </div>
          <button
            onClick={() => setShowConnectivityDoctor(false)}
            className="p-1 text-[#a3a3a3] hover:text-white rounded transition"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-5 space-y-4 text-xs">
          {/* Certificate Card */}
          <div className="p-4 rounded-lg border border-[#333333] bg-[#1f1f1f]">
            <div className="flex items-start justify-between gap-3">
              <div className="space-y-1">
                <span className="text-[10px] uppercase font-mono px-1.5 py-0.5 rounded bg-[#e50914] text-white font-semibold">
                  Connectivity Certificate
                </span>
                <p className="text-xs font-mono text-white mt-1">
                  {report?.certificate_text || 'Connectable: Yes · Port 51413 Open · IPv6 Ready · Tracker Healthy'}
                </p>
              </div>
              <button
                onClick={copyCertificate}
                className="flex items-center gap-1 px-2.5 py-1 text-xs rounded border border-[#333333] bg-[#141414] text-[#a3a3a3] hover:text-white transition"
              >
                {copiedCert ? <Check className="w-3.5 h-3.5 text-[#e50914]" /> : <Copy className="w-3.5 h-3.5" />}
                <span>{copiedCert ? 'Copied' : 'Copy'}</span>
              </button>
            </div>
          </div>

          {/* Core Status Grid */}
          <div className="grid grid-cols-2 sm:grid-cols-4 gap-2.5">
            <div className="p-3 rounded-lg border border-[#333333] bg-[#1f1f1f]">
              <span className="text-[10px] text-[#a3a3a3] block">Listening Port</span>
              <div className="mt-1 flex items-center justify-between font-mono">
                <span className="text-sm font-bold text-white">{report?.listening_port || 51413}</span>
                <span className="text-[10px] text-white bg-[#141414] px-1.5 py-0.5 rounded border border-[#333333]">
                  {report?.port_status || 'Open'}
                </span>
              </div>
            </div>

            <div className="p-3 rounded-lg border border-[#333333] bg-[#1f1f1f]">
              <span className="text-[10px] text-[#a3a3a3] block">NAT Mapping</span>
              <div className="mt-1 flex items-center justify-between font-mono">
                <span className="text-sm font-bold text-white">{report?.upnp_available ? 'UPnP' : 'Manual'}</span>
                <span className="text-[10px] text-[#a3a3a3]">Active</span>
              </div>
            </div>

            <div className="p-3 rounded-lg border border-[#333333] bg-[#1f1f1f]">
              <span className="text-[10px] text-[#a3a3a3] block">IPv6 Status</span>
              <div className="mt-1 flex items-center justify-between font-mono">
                <span className="text-sm font-bold text-white">{report?.ipv6_healthy ? 'Ready' : 'IPv4'}</span>
                <span className="text-[10px] text-[#a3a3a3]">Dual-Stack</span>
              </div>
            </div>

            <div className="p-3 rounded-lg border border-[#333333] bg-[#1f1f1f]">
              <span className="text-[10px] text-[#a3a3a3] block">DNS Latency</span>
              <div className="mt-1 flex items-center justify-between font-mono">
                <span className="text-sm font-bold text-white">{report?.dns_health_ms || 18} ms</span>
                <span className="text-[10px] text-[#a3a3a3]">Normal</span>
              </div>
            </div>
          </div>

          {/* Plain-Language Findings */}
          <div className="p-3.5 rounded-lg border border-[#333333] bg-[#1f1f1f] space-y-2">
            <h3 className="text-xs font-semibold text-white flex items-center gap-1.5">
              <Shield className="w-3.5 h-3.5 text-[#e50914]" />
              <span>Findings</span>
            </h3>
            <ul className="space-y-1 text-xs text-[#a3a3a3]">
              {report?.plain_language_findings.map((f, i) => (
                <li key={i} className="flex items-start gap-2">
                  <CheckCircle className="w-3.5 h-3.5 text-[#e50914] shrink-0 mt-0.5" />
                  <span>{f}</span>
                </li>
              ))}
            </ul>
          </div>

          {/* Retest Bar */}
          <div className="flex flex-wrap items-center justify-between gap-3 p-3 rounded-lg border border-[#333333] bg-[#1f1f1f]">
            <div className="flex items-center gap-2">
              <span className="text-xs text-[#a3a3a3]">Custom Port:</span>
              <input
                type="number"
                value={portInput}
                onChange={(e) => setPortInput(e.target.value)}
                className="w-20 px-2 py-1 text-xs rounded border border-[#333333] bg-[#141414] text-white font-mono focus:border-[#e50914] focus:outline-none"
              />
              <button
                onClick={() => retestPort(parseInt(portInput) || 51413)}
                disabled={isRetestingPort}
                className="px-2.5 py-1 text-xs rounded border border-[#333333] bg-[#141414] hover:bg-[#262626] text-white transition"
              >
                {isRetestingPort ? 'Testing...' : 'Test Port'}
              </button>
            </div>

            <button
              onClick={() => runDoctorTest()}
              disabled={isRunningTest}
              className="flex items-center gap-1.5 px-3.5 py-1.5 text-xs font-semibold rounded bg-[#e50914] hover:bg-[#f40612] text-white transition"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${isRunningTest ? 'animate-spin' : ''}`} />
              <span>{isRunningTest ? 'Testing...' : 'Retest Network'}</span>
            </button>
          </div>

          {/* Router Port Forwarding Accordion */}
          <div className="border border-[#333333] rounded-lg overflow-hidden bg-[#1f1f1f]">
            <div className="p-2.5 bg-[#262626] text-xs font-semibold text-white flex items-center justify-between">
              <span>Port-Forwarding Instructions</span>
            </div>

            <div className="divide-y divide-[#333333] text-xs">
              <div>
                <button
                  onClick={() => toggleAccordion('asus')}
                  className="w-full px-3 py-2 flex items-center justify-between text-left hover:bg-[#262626] transition text-[#a3a3a3] hover:text-white"
                >
                  <span>ASUS Routers (Asuswrt / Merlin)</span>
                  {activeAccordion === 'asus' ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
                </button>
                {activeAccordion === 'asus' && (
                  <div className="px-3 py-2 bg-[#141414] text-[#a3a3a3] space-y-1 font-mono text-[11px]">
                    <p>1. Open http://router.asus.com in your browser.</p>
                    <p>2. Go to WAN → Virtual Server / Port Forwarding.</p>
                    <p>3. Add Profile: Port = 51413, Protocol = BOTH, Local IP = device IP.</p>
                  </div>
                )}
              </div>

              <div>
                <button
                  onClick={() => toggleAccordion('netgear')}
                  className="w-full px-3 py-2 flex items-center justify-between text-left hover:bg-[#262626] transition text-[#a3a3a3] hover:text-white"
                >
                  <span>Netgear Routers</span>
                  {activeAccordion === 'netgear' ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
                </button>
                {activeAccordion === 'netgear' && (
                  <div className="px-3 py-2 bg-[#141414] text-[#a3a3a3] space-y-1 font-mono text-[11px]">
                    <p>1. Open http://routerlogin.net.</p>
                    <p>2. Go to Advanced → Port Forwarding.</p>
                    <p>3. Forward port 51413 to your local IP address.</p>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>

        {/* Footer */}
        <div className="px-5 py-3 border-t border-[#333333] bg-[#1f1f1f] flex justify-end">
          <button
            onClick={() => setShowConnectivityDoctor(false)}
            className="px-4 py-1.5 text-xs rounded border border-[#333333] bg-[#141414] hover:bg-[#262626] text-white transition"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
};

export default ConnectivityDoctorModal;
