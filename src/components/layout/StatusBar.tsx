import React from 'react';
import { useTorrentStore } from '../../stores/torrentStore';
import { useConnectivityStore } from '../../stores/connectivityStore';
import { formatBytes, formatSpeed } from '../../lib/formatters';

export const StatusBar: React.FC = () => {
  const { torrents } = useTorrentStore();
  const { report } = useConnectivityStore();

  const totalDlSpeed = torrents.reduce((acc, t) => acc + (t.download_rate || 0), 0);
  const totalUlSpeed = torrents.reduce((acc, t) => acc + (t.upload_rate || 0), 0);
  const totalDownloaded = torrents.reduce((acc, t) => acc + (t.downloaded_bytes || 0), 0);
  const totalUploaded = torrents.reduce((acc, t) => acc + (t.uploaded_bytes || 0), 0);

  const port = report?.listening_port || 51413;
  const isPortOpen = report?.port_status === 'Open';

  return (
    <footer className="h-6 bg-[#141414] border-t border-[#262626] flex items-center justify-between px-3 text-[11px] font-mono text-[#a3a3a3] select-none z-30">
      {/* Left: Swarm Connectivity, Port & DHT */}
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-1.5" title="Swarm Network Status">
          <span
            className={`w-2 h-2 rounded-full transition-colors ${
              isPortOpen
                ? 'bg-emerald-500 shadow-[0_0_8px_#10b981]'
                : 'bg-red-500 shadow-[0_0_6px_rgba(239,68,68,0.6)] animate-pulse'
            }`}
          />
          <span className="text-white font-sans text-[11px]">
            {isPortOpen ? 'Connected' : 'Disconnected'}
          </span>
        </div>

        <span className="text-[#333333]">|</span>

        <span title="Distributed Hash Table Nodes">DHT: 342 nodes</span>

        <span className="text-[#333333]">|</span>

        <span title="Inbound Listening Port">
          Port: {port} <span className="text-white">[{isPortOpen ? 'Open' : 'Closed'}]</span>
        </span>
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
        <span title="Available Storage Capacity">Disk: 142.8 GB free</span>

        <span className="text-[#333333]">|</span>

        <span className="text-[#a3a3a3]" title="Protocol Protocol: BitTorrent Dual-Stack uTP/TCP">
          uTP + TCP
        </span>
      </div>
    </footer>
  );
};

export default StatusBar;
