import React, { useState, useEffect } from 'react';
import { useUiStore } from '../../stores/uiStore';
import { useTorrentStore } from '../../stores/torrentStore';
import {
  Trash2,
  X,
  AlertTriangle,
  ShieldCheck,
  AlertCircle,
  FileText,
} from 'lucide-react';

export const DeleteTorrentModal: React.FC = () => {
  const { deleteModalTarget, setDeleteModalTarget } = useUiStore();
  const { removeTorrent } = useTorrentStore();

  const [deleteFiles, setDeleteFiles] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // Reset state when opening a new target
  useEffect(() => {
    if (deleteModalTarget) {
      setDeleteFiles(false);
      setIsDeleting(false);
      setErrorMessage(null);
    }
  }, [deleteModalTarget]);

  // Handle keyboard events (Enter to confirm, Escape to cancel)
  useEffect(() => {
    if (!deleteModalTarget) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        setDeleteModalTarget(null);
      } else if (e.key === 'Enter' && !isDeleting) {
        e.preventDefault();
        handleConfirm();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [deleteModalTarget, deleteFiles, isDeleting]);

  if (!deleteModalTarget) return null;

  const handleClose = () => {
    if (isDeleting) return;
    setDeleteModalTarget(null);
  };

  const handleConfirm = async () => {
    if (!deleteModalTarget || isDeleting) return;
    setIsDeleting(true);
    setErrorMessage(null);

    try {
      await removeTorrent(deleteModalTarget.id, deleteFiles);
      setDeleteModalTarget(null);
    } catch (err: any) {
      setErrorMessage(typeof err === 'string' ? err : err?.message || 'Failed to remove torrent');
      setIsDeleting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm select-none font-sans">
      <div className="relative w-full max-w-md bg-[#141414] border border-[#333333] rounded-xl shadow-2xl overflow-hidden flex flex-col text-white animate-in fade-in zoom-in-95 duration-100">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-[#333333] bg-[#1f1f1f]">
          <div className="flex items-center gap-2">
            <Trash2 className="w-4 h-4 text-[#e50914]" />
            <h2 className="text-sm font-bold text-white">Remove Torrent</h2>
          </div>
          <button
            type="button"
            onClick={handleClose}
            disabled={isDeleting}
            className="p-1 rounded text-[#a3a3a3] hover:text-white disabled:opacity-50 transition"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Body Content */}
        <div className="p-5 space-y-4 text-xs">
          <p className="text-sm text-[#cccccc] leading-relaxed">
            Are you sure you want to remove this torrent from the transfer list?
          </p>

          {/* Torrent Item Preview Card */}
          <div className="p-3 rounded bg-[#1f1f1f] border border-[#262626] flex items-center gap-2.5">
            <FileText className="w-4 h-4 text-[#e50914] flex-shrink-0" />
            <span className="font-semibold text-white font-mono text-xs truncate max-w-sm">
              {deleteModalTarget.name}
            </span>
          </div>

          {/* qBittorrent Checkbox Option */}
          <label className="flex items-center gap-2.5 p-2 rounded hover:bg-[#1a1a1a] cursor-pointer transition select-none">
            <input
              type="checkbox"
              checked={deleteFiles}
              onChange={(e) => setDeleteFiles(e.target.checked)}
              disabled={isDeleting}
              className="w-4 h-4 rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-[#e50914] focus:ring-offset-0 cursor-pointer accent-[#e50914]"
            />
            <span className="text-xs font-medium text-white">
              Also delete the files on the hard disk
            </span>
          </label>

          {/* Warning Banner when Delete Files is checked */}
          {deleteFiles && (
            <div className="p-3 rounded bg-[#261010] border border-[#e50914]/50 flex items-start gap-2.5 text-xs text-red-200">
              <AlertTriangle className="w-4 h-4 text-[#e50914] flex-shrink-0 mt-0.5" />
              <div className="leading-snug">
                <strong className="text-white block mb-0.5">Permanent Deletion Warning:</strong>
                All downloaded files and directories for this transfer will be permanently erased from your storage.
              </div>
            </div>
          )}

          {/* Safety Guard Shield */}
          <div className="p-2.5 rounded bg-[#1f1f1f] border border-[#262626] flex items-start gap-2 text-[11px] text-[#a3a3a3]">
            <ShieldCheck className="w-4 h-4 text-[#e50914] flex-shrink-0 mt-0.5" />
            <span>
              <strong className="text-white">Safety Protection:</strong> Directories or files managed by external qBittorrent installations and system directories are strictly safeguarded.
            </span>
          </div>

          {/* Error Banner */}
          {errorMessage && (
            <div className="p-2.5 rounded bg-[#261010] border border-[#e50914] flex items-start gap-2 text-xs text-red-200 font-mono">
              <AlertCircle className="w-4 h-4 text-[#e50914] flex-shrink-0 mt-0.5" />
              <span>{errorMessage}</span>
            </div>
          )}

          {/* Action Buttons */}
          <div className="flex justify-end gap-2 pt-3 border-t border-[#333333]">
            <button
              type="button"
              onClick={handleClose}
              disabled={isDeleting}
              className="px-4 py-1.5 rounded-lg text-[#a3a3a3] hover:text-white transition disabled:opacity-50"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={handleConfirm}
              disabled={isDeleting}
              className={`px-4 py-1.5 rounded-lg text-white font-medium transition flex items-center gap-1.5 disabled:opacity-50 ${
                deleteFiles
                  ? 'bg-[#e50914] hover:bg-[#b80710]'
                  : 'bg-[#e50914] hover:bg-[#f40612]'
              }`}
            >
              {isDeleting ? (
                <span>Removing...</span>
              ) : deleteFiles ? (
                <span>Delete Torrent & Files</span>
              ) : (
                <span>Remove</span>
              )}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
