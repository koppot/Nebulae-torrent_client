import React, { useState } from 'react';
import { useUiStore } from '../../stores/uiStore';
import { useTorrentStore } from '../../stores/torrentStore';
import { X, Sparkles, Folder, FileText, CheckCircle2, AlertCircle, Loader2 } from 'lucide-react';

const LAST_SAVED_TRACKERS_KEY = 'nebulae_last_created_trackers';

export const CreateTorrentModal: React.FC = () => {
  const { showCreateTorrent, setShowCreateTorrent } = useUiStore();
  const { createTorrent, selectSourceFolder, selectSourceFile, selectDownloadDirectory } =
    useTorrentStore();

  const [sourcePath, setSourcePath] = useState('');
  const [outputPath, setOutputPath] = useState('');
  const [pieceSizeKib, setPieceSizeKib] = useState(1024); // 1 MiB default
  const [trackers, setTrackers] = useState(() => {
    try {
      return localStorage.getItem(LAST_SAVED_TRACKERS_KEY) || '';
    } catch {
      return '';
    }
  });
  const [isPrivate, setIsPrivate] = useState(true); // Private by default
  const [comment, setComment] = useState('Created with Nebulae');
  const [createdBy, setCreatedBy] = useState('Nebulae Torrent Client 1.0');
  const [startSeeding, setStartSeeding] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [statusMessage, setStatusMessage] = useState<{ type: 'ok' | 'err'; text: string } | null>(
    null
  );

  if (!showCreateTorrent) return null;

  const handleBrowseFolder = async () => {
    try {
      const path = await selectSourceFolder();
      if (path) setSourcePath(path);
    } catch {
      //
    }
  };

  const handleBrowseFile = async () => {
    try {
      const path = await selectSourceFile();
      if (path) setSourcePath(path);
    } catch {
      //
    }
  };

  const handleBrowseOutput = async () => {
    try {
      const dir = await selectDownloadDirectory();
      if (dir) setOutputPath(dir);
    } catch {
      //
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!sourcePath.trim()) {
      setStatusMessage({ type: 'err', text: 'Please specify a source file or directory' });
      return;
    }

    setIsSubmitting(true);
    setStatusMessage(null);

    const trackerList = trackers
      .split('\n')
      .map((t) => t.trim())
      .filter((t) => t.length > 0);

    // Save tracker URLs so subsequent creations offer the same tracker URLs
    if (trackers.trim()) {
      try {
        localStorage.setItem(LAST_SAVED_TRACKERS_KEY, trackers.trim());
      } catch {
        // Ignore storage errors
      }
    }

    try {
      const createdFile = await createTorrent({
        source_path: sourcePath.trim(),
        output_path: outputPath.trim(),
        piece_size_kib: pieceSizeKib,
        trackers: trackerList,
        web_seeds: [],
        is_private: isPrivate,
        comment: comment.trim(),
        created_by: createdBy.trim(),
        start_seeding: startSeeding,
      });

      setStatusMessage({
        type: 'ok',
        text: `Torrent created successfully: ${createdFile}`,
      });
      setTimeout(() => {
        setIsSubmitting(false);
        setShowCreateTorrent(false);
      }, 1500);
    } catch (err: any) {
      setIsSubmitting(false);
      setStatusMessage({ type: 'err', text: `Failed to create torrent: ${err}` });
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm select-none font-sans">
      <div className="relative w-full max-w-2xl bg-[#141414] border border-[#333333] rounded-xl shadow-2xl overflow-hidden flex flex-col text-white">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-[#333333] bg-[#1f1f1f]">
          <div className="flex items-center gap-2">
            <Sparkles className="w-4 h-4 text-[#e50914]" />
            <h2 className="text-sm font-bold text-white">Torrent Creator (Create .torrent / .adc)</h2>
          </div>
          <button
            onClick={() => setShowCreateTorrent(false)}
            className="p-1 rounded text-[#a3a3a3] hover:text-white transition"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Form Body */}
        <form onSubmit={handleSubmit} className="p-5 space-y-4 text-xs text-[#a3a3a3]">
          {/* Source Selection */}
          <div>
            <label className="block text-xs font-medium text-white mb-1.5">
              Select Source Content (Folder or File)
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                value={sourcePath}
                onChange={(e) => setSourcePath(e.target.value)}
                placeholder="/path/to/folder or /path/to/movie.mkv"
                className="flex-1 px-3 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white font-mono text-xs focus:outline-none focus:border-[#e50914]"
              />
              <button
                type="button"
                onClick={handleBrowseFolder}
                className="px-3 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white flex items-center gap-1.5 transition text-xs whitespace-nowrap"
                title="Select a directory with multiple files or a complete release"
              >
                <Folder className="w-3.5 h-3.5 text-[#e50914]" />
                <span>Select Folder...</span>
              </button>
              <button
                type="button"
                onClick={handleBrowseFile}
                className="px-3 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white flex items-center gap-1.5 transition text-xs whitespace-nowrap"
                title="Select a single media or data file"
              >
                <FileText className="w-3.5 h-3.5 text-blue-400" />
                <span>Select File...</span>
              </button>
            </div>
          </div>

          {/* Piece Size & Destination Folder */}
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-medium text-white mb-1.5">Piece Size</label>
              <select
                value={pieceSizeKib}
                onChange={(e) => setPieceSizeKib(Number(e.target.value))}
                className="w-full px-2.5 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:border-[#e50914] focus:outline-none font-mono"
              >
                <option value={32}>32 KiB</option>
                <option value={64}>64 KiB</option>
                <option value={128}>128 KiB</option>
                <option value={256}>256 KiB</option>
                <option value={512}>512 KiB</option>
                <option value={1024}>1 MiB (Recommended)</option>
                <option value={2048}>2 MiB</option>
                <option value={4096}>4 MiB (4K Media)</option>
                <option value={8192}>8 MiB</option>
                <option value={16384}>16 MiB</option>
              </select>
            </div>

            <div>
              <label className="block text-xs font-medium text-white mb-1.5">Output Directory</label>
              <div className="flex gap-1.5">
                <input
                  type="text"
                  value={outputPath}
                  onChange={(e) => setOutputPath(e.target.value)}
                  placeholder="Defaults to Downloads"
                  className="flex-1 px-2.5 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white font-mono text-xs focus:outline-none focus:border-[#e50914]"
                />
                <button
                  type="button"
                  onClick={handleBrowseOutput}
                  className="px-2.5 py-1.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-white transition text-xs"
                >
                  Browse...
                </button>
              </div>
            </div>
          </div>

          {/* Trackers Textarea */}
          <div>
            <label className="block text-xs font-medium text-white mb-1">
              Tracker URLs (One per line, tiers separated by empty line)
            </label>
            <textarea
              rows={3}
              value={trackers}
              onChange={(e) => setTrackers(e.target.value)}
              className="w-full px-3 py-2 rounded border border-[#333333] bg-[#1f1f1f] text-white font-mono text-xs focus:outline-none focus:border-[#e50914]"
            />
          </div>

          {/* Comments & Flags */}
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-[11px] text-[#a3a3a3] mb-1">Comments</label>
              <input
                type="text"
                value={comment}
                onChange={(e) => setComment(e.target.value)}
                className="w-full px-2.5 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:outline-none focus:border-[#e50914]"
              />
            </div>
            <div>
              <label className="block text-[11px] text-[#a3a3a3] mb-1">Created By</label>
              <input
                type="text"
                value={createdBy}
                onChange={(e) => setCreatedBy(e.target.value)}
                className="w-full px-2.5 py-1.5 rounded border border-[#333333] bg-[#1f1f1f] text-white text-xs focus:outline-none focus:border-[#e50914]"
              />
            </div>
          </div>

          {/* Checkboxes: Private Torrent & Start Seeding */}
          <div className="pt-2 border-t border-[#333333] flex items-center justify-between">
            <div className="space-y-1.5">
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={isPrivate}
                  onChange={(e) => setIsPrivate(e.target.checked)}
                  className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                />
                <span className="text-white">
                  Private Torrent (Disables DHT, PeX, and LSD for private communities)
                </span>
              </label>
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={startSeeding}
                  onChange={(e) => setStartSeeding(e.target.checked)}
                  className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                />
                <span className="text-white">Start seeding immediately</span>
              </label>
            </div>
          </div>

          {statusMessage && (
            <div
              className={`p-2.5 rounded border flex items-center gap-2 text-xs font-mono ${
                statusMessage.type === 'ok'
                  ? 'bg-emerald-950/60 border-emerald-700/60 text-emerald-300'
                  : 'bg-red-950/60 border-red-800/60 text-red-300'
              }`}
            >
              {statusMessage.type === 'ok' ? (
                <CheckCircle2 className="w-4 h-4 text-emerald-400 flex-shrink-0" />
              ) : (
                <AlertCircle className="w-4 h-4 text-red-400 flex-shrink-0" />
              )}
              <span>{statusMessage.text}</span>
            </div>
          )}

          {/* Footer Actions */}
          <div className="flex justify-end gap-2 pt-3 border-t border-[#333333]">
            <button
              type="button"
              onClick={() => setShowCreateTorrent(false)}
              className="px-4 py-1.5 rounded-lg text-[#a3a3a3] hover:text-white transition"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSubmitting || !sourcePath.trim()}
              className="px-5 py-1.5 rounded-lg bg-[#e50914] hover:bg-[#f40612] disabled:opacity-40 text-white font-semibold transition flex items-center gap-1.5 shadow"
            >
              {isSubmitting ? (
                <>
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                  <span>Hashing & Creating...</span>
                </>
              ) : (
                <span>Create .torrent</span>
              )}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};

export default CreateTorrentModal;
