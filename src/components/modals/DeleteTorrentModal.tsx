import React, { useState, useEffect, useCallback } from 'react';
import { useUiStore } from '../../stores/uiStore';
import { useTorrentStore } from '../../stores/torrentStore';
import { useSettingsStore } from '../../stores/settingsStore';
import {
  Trash2,
  X,
  AlertTriangle,
  ShieldCheck,
  FileText,
} from 'lucide-react';

export const DeleteTorrentModal: React.FC = () => {
  const { deleteModalTarget, setDeleteModalTarget } = useUiStore();
  const { removeTorrent, activeOperations } = useTorrentStore();
  const { engineMode } = useSettingsStore();

  const [deleteFiles, setDeleteFiles] = useState(false);

  const activeOp = deleteModalTarget
    ? activeOperations[deleteModalTarget.id] ||
      (deleteModalTarget.id ? activeOperations[deleteModalTarget.id.toLowerCase()] : undefined)
    : undefined;
  const isOperating = Boolean(activeOp && activeOp.phase !== 'error');

  // Reset state when opening a new target
  useEffect(() => {
    if (deleteModalTarget) {
      setDeleteFiles(false);
    }
  }, [deleteModalTarget]);

  // Immediate acknowledgement: closes the modal immediately upon confirmation
  // and hands execution to the background lifecycle so the Manager UI remains
  // completely responsive with zero navigation lockup.
  const handleConfirm = useCallback(() => {
    if (!deleteModalTarget || isOperating) return;
    const targetId = deleteModalTarget.id;
    const delFiles = deleteFiles;
    setDeleteModalTarget(null);
    void removeTorrent(targetId, delFiles);
  }, [deleteModalTarget, deleteFiles, removeTorrent, setDeleteModalTarget, isOperating]);

  // Handle keyboard events (Enter to confirm, Escape to cancel)
  useEffect(() => {
    if (!deleteModalTarget) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        setDeleteModalTarget(null);
      } else if (e.key === 'Enter') {
        e.preventDefault();
        handleConfirm();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [deleteModalTarget, deleteFiles, handleConfirm, setDeleteModalTarget]);

  if (!deleteModalTarget) return null;

  const handleClose = () => {
    setDeleteModalTarget(null);
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
            className="p-1 rounded text-[#a3a3a3] hover:text-white transition"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Body Content */}
        <div className="p-5 space-y-4 text-xs">
          <p className="text-[13px] text-[#cccccc] leading-relaxed whitespace-nowrap">
            Are you sure you want to remove this torrent from the transfer list?
          </p>

          {/* Torrent Item Preview Card */}
          <div className="p-3 rounded bg-[#1f1f1f] border border-[#262626] flex items-center gap-2.5">
            <FileText className="w-4 h-4 text-[#e50914] flex-shrink-0" />
            <span className="font-semibold text-white font-mono text-xs truncate max-w-sm">
              {deleteModalTarget.name}
            </span>
          </div>

          {/* qBittorrent Checkbox Option — rtorrent (server mode) erases the
              torrent only and refuses data deletion over RPC */}
          {engineMode !== 'rtorrent_server' && (
            <label className="flex items-center gap-2.5 p-2 rounded hover:bg-[#1a1a1a] cursor-pointer transition select-none">
              <input
                type="checkbox"
                checked={deleteFiles}
                onChange={(e) => setDeleteFiles(e.target.checked)}
                className="w-4 h-4 rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-[#e50914] focus:ring-offset-0 cursor-pointer accent-[#e50914]"
              />
              <span className="text-xs font-medium text-white">
                Also delete the files on the hard disk
              </span>
            </label>
          )}

          {/* Warning Banner when Delete Files is checked */}
          {deleteFiles && engineMode !== 'rtorrent_server' && (
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
              <strong className="text-white">Safety Protection:</strong> Torrent data directories and system directories are strictly safeguarded.
            </span>
          </div>

          {/* Action Buttons */}
          <div className="flex justify-end gap-2 pt-3 border-t border-[#333333]">
            <button
              type="button"
              onClick={handleClose}
              className="px-4 py-1.5 rounded-lg text-[#a3a3a3] hover:text-white transition"
            >
              Cancel
            </button>
            <button
              type="button"
              disabled={isOperating}
              onClick={handleConfirm}
              className={`px-4 py-1.5 rounded-lg text-white font-medium transition flex items-center gap-1.5 ${
                isOperating
                  ? 'opacity-40 cursor-not-allowed bg-neutral-700'
                  : deleteFiles
                    ? 'bg-[#e50914] hover:bg-[#b80710]'
                    : 'bg-[#e50914] hover:bg-[#f40612]'
              }`}
              title={isOperating ? 'An operation is already in progress for this torrent' : undefined}
            >
              {deleteFiles ? (
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
