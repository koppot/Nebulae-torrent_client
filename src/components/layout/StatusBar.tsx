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

  const port = report?.listening_port || 6881;
  const isPortOpen = report?.port_status === 'Open';

  return (
    <footer className="h-6 bg-[#141414] border-t border-[#262626] flex items-center justify-between px-3 text-[11px] font-mono text-[#a3a3a3] select-none z-30">
      {/* Left: Swarm Connectivity, Port & DHT */}
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-1.5" title="Swarm Network Status">
          <span
            className={`w-2 h-2 rounded-full ${
              isPortOpen ? 'bg-[#e50914]' : 'bg-[#e50914]/50 animate-pulse'
            }`}
          />
          <span className="text-white font-sans text-[11px]">
            {isPortOpen ? 'Connected' : 'Connecting'}
          </span>
        </div>

        <span className="text-[#333333]">|</span>

        <span title="Distributed Hash Table Nodes">DHT: 342 nodes</span>

        <span className="text-[#333333]">|</span>

        <span title="Inbound Listening Port">
          Port: {port} <span className="text-white">[{isPortOpen ? 'Open' : 'Testing'}]</span>
        </span>
      </div>

      {/* Center: Global Transfer Throughput */}
      <div className="flex items-center gap-4">
        <div className="flex items-center gap-1.5 text-white" title="Global Download Rate & Total Downloaded">
          <span className="text-[#e50914] font-bold">▼</span>
          <span>{formatSpeed(totalDlSpeed)}</span>
          <span className="text-[#a3a3a3]">({formatBytes(totalDownloaded)})</span>
        </div>

        <span className="text-[#333333]">|</span>

        <div className="flex items-center gap-1.5 text-white" title="Global Upload Rate & Total Uploaded">
          <span className="text-[#e50914] font-bold">▲</span>
          <span>{formatSpeed(totalUlSpeed)}</span>
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
