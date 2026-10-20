import React, { useEffect, useRef, useState } from 'react';
import { resolvePoster, toDisplaySrc } from '../../lib/posters';

/**
 * Poster hover popup for the torrent listing, mirroring the ADC
 * torrents.php behavior (OpenTip image popup on hover over the name):
 * a floating card with ONLY the torrent's poster at 2:3 aspect, appearing
 * ~250 ms after the pointer settles on a row and disappearing on leave.
 * One shared instance; the parent positions it near the hovered row.
 */
export interface PosterPopupTarget {
  id: string;
  name: string;
  info_hash: string;
  save_path: string;
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
  // 70% of previous dimensions (was 220x330), 2:3 aspect ratio preserved.
  const CARD_W = 154;
  const CARD_H = 231;
  const left = x + 16 + CARD_W > window.innerWidth ? x - CARD_W - 16 : x + 16;
  const top = Math.max(8, Math.min(y - 24, window.innerHeight - CARD_H - 8));

  return (
    <div
      className="fixed z-[100] pointer-events-none rounded-lg border border-[#333333] bg-[#111111] shadow-2xl overflow-hidden"
      style={{ left, top, width: CARD_W, height: CARD_H }}
      data-testid="torrent-poster-popup"
    >
      {poster ? (
        <img src={toDisplaySrc(poster)} alt={target.name} className="w-full h-full object-cover" />
      ) : loading ? (
        <div className="w-full h-full flex items-center justify-center">
          <span className="text-[#555555] text-xs font-mono">…</span>
        </div>
      ) : (
        <div className="w-full h-full flex items-center justify-center">
          <span className="text-[#666666] text-4xl font-serif">
            {target.name.charAt(0).toUpperCase()}
          </span>
        </div>
      )}
    </div>
  );
};

export default PosterHoverPopup;
