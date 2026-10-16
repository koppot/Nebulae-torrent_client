import React, { useEffect, useRef, useState } from 'react';
import { resolvePoster } from '../../lib/posters';
import { formatBytes } from '../../lib/formatters';

/**
 * Poster hover popup for the torrent listing, mirroring the ADC
 * torrents.php behavior (OpenTip image popup on hover over the name):
 * a floating card with the torrent's poster at 2:3 aspect, appearing
 * ~250 ms after the pointer settles on a row and disappearing on leave.
 * One shared instance; the parent positions it near the hovered row.
 */
export interface PosterPopupTarget {
  id: string;
  name: string;
  info_hash: string;
  save_path: string;
  size_bytes: number;
  progress: number;
  category: string;
}

const PosterHoverPopup: React.FC<{ target: PosterPopupTarget | null; x: number; y: number }> = ({
  target,
  x,
  y,
}) => {
  const [poster, setPoster] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const requestedRef = useRef<string | null>(null);

  useEffect(() => {
    if (!target) return;
    if (requestedRef.current !== target.id) {
      requestedRef.current = target.id;
      setPoster(null);
      setLoading(true);
      resolvePoster(target.info_hash, target.name, target.save_path)
        .then(setPoster)
        .catch(() => setPoster(null))
        .finally(() => setLoading(false));
    }
  }, [target]);

  if (!target) return null;

  // Flip left when the card would overflow the right edge; clamp top.
  const CARD_W = 220;
  const CARD_H = 360;
  const left = x + 16 + CARD_W > window.innerWidth ? x - CARD_W - 16 : x + 16;
  const top = Math.max(8, Math.min(y - 24, window.innerHeight - CARD_H - 8));
  const pct = Math.round(target.progress * 100);

  return (
    <div
      className="fixed z-[100] pointer-events-none rounded-lg border border-[#333333] bg-[#1f1f1f] shadow-2xl overflow-hidden"
      style={{ left, top, width: CARD_W }}
      data-testid="torrent-poster-popup"
    >
      <div className="w-full bg-[#111111] flex items-center justify-center" style={{ height: 300 }}>
        {poster ? (
          <img src={poster} alt={target.name} className="w-full h-full object-cover" />
        ) : loading ? (
          <span className="text-[#555555] text-xs font-mono">…</span>
        ) : (
          <span className="text-[#666666] text-4xl font-serif">
            {target.name.charAt(0).toUpperCase()}
          </span>
        )}
      </div>
      <div className="p-2 space-y-1">
        <div className="text-[11px] text-white font-sans font-medium leading-tight line-clamp-2" title={target.name}>
          {target.name}
        </div>
        <div className="text-[10px] text-[#a3a3a3] font-mono flex items-center justify-between">
          <span>{target.category || '—'}</span>
          <span>
            {formatBytes(target.size_bytes)} · {pct}%
          </span>
        </div>
      </div>
    </div>
  );
};

export default PosterHoverPopup;
