import React, { useState } from 'react';
import { useUiStore } from '../../stores/uiStore';
import { useTorrentStore } from '../../stores/torrentStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { X, Plus, Magnet, FileUp, Folder } from 'lucide-react';

export const AddTorrentModal: React.FC = () => {
  const { showAddTorrent, setShowAddTorrent } = useUiStore();
  const { addTorrent } = useTorrentStore();
  const { settings } = useSettingsStore();

  const [torrentSource, setTorrentSource] = useState('');
  const [downloadDir, setDownloadDir] = useState(settings.download_directory);
  const [isSubmitting, setIsSubmitting] = useState(false);

  if (!showAddTorrent) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!torrentSource.trim()) return;

    setIsSubmitting(true);
    await addTorrent(torrentSource.trim());
    setIsSubmitting(false);
    setTorrentSource('');
    setShowAddTorrent(false);
  };

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
      <div className="relative w-full max-w-lg bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl overflow-hidden flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-slate-800 bg-slate-950/40">
          <div className="flex items-center gap-2">
            <Plus className="w-5 h-5 text-pink-400" />
            <h2 className="text-base font-semibold text-white">Add New Torrent</h2>
          </div>
          <button
            onClick={() => setShowAddTorrent(false)}
            className="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Form Body */}
        <form onSubmit={handleSubmit} className="p-6 space-y-5 text-xs text-slate-300">
          <div>
            <label className="block text-xs font-medium text-slate-300 mb-1.5 flex items-center gap-1.5">
              <Magnet className="w-3.5 h-3.5 text-pink-400" />
              <span>Magnet Link or Torrent URL</span>
            </label>
            <textarea
              rows={3}
              placeholder="magnet:?xt=urn:btih:... or adc://..."
              value={torrentSource}
              onChange={(e) => setTorrentSource(e.target.value)}
              className="w-full px-3 py-2 rounded-lg bg-slate-950 border border-slate-700 text-slate-100 font-mono text-xs focus:outline-none focus:border-pink-500"
            />
          </div>

          <div className="flex items-center gap-2">
            <div className="flex-1 h-[1px] bg-slate-800"></div>
            <span className="text-[10px] uppercase font-mono text-slate-500">Or drag and drop .torrent</span>
            <div className="flex-1 h-[1px] bg-slate-800"></div>
          </div>

          <div className="border border-dashed border-slate-700 hover:border-pink-500/50 rounded-xl p-4 text-center bg-slate-950/40 hover:bg-slate-950 transition cursor-pointer flex flex-col items-center justify-center gap-1.5">
            <FileUp className="w-6 h-6 text-slate-400" />
            <span className="text-xs text-slate-300 font-medium">Click to select .torrent file</span>
            <span className="text-[10px] text-slate-500">Supports all standard BEP metainfo files</span>
          </div>

          <div>
            <label className="block text-xs font-medium text-slate-300 mb-1.5 flex items-center gap-1.5">
              <Folder className="w-3.5 h-3.5 text-cyan-400" />
              <span>Save Destination Directory</span>
            </label>
            <input
              type="text"
              value={downloadDir}
              onChange={(e) => setDownloadDir(e.target.value)}
              className="w-full px-3 py-2 rounded-lg bg-slate-950 border border-slate-700 text-slate-100 font-mono text-xs focus:outline-none focus:border-pink-500"
            />
          </div>

          <div className="flex justify-end gap-3 pt-3 border-t border-slate-800">
            <button
              type="button"
              onClick={() => setShowAddTorrent(false)}
              className="px-4 py-2 rounded-lg text-slate-400 hover:text-white transition"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSubmitting || !torrentSource.trim()}
              className="px-5 py-2 rounded-lg bg-pink-600 hover:bg-pink-500 disabled:opacity-50 text-white font-semibold transition"
            >
              {isSubmitting ? 'Adding...' : 'Start Download'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
