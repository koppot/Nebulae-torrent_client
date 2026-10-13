import React, { useState, useRef, useEffect } from 'react';
import { useUiStore } from '../../stores/uiStore';
import { useTorrentStore, SystemPathPresets } from '../../stores/torrentStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { X, Plus, FileUp, Folder, FileCheck } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';

export const AddTorrentModal: React.FC = () => {
  const { showAddTorrent, setShowAddTorrent, initialAddTorrentSource, setInitialAddTorrentSource } = useUiStore();
  const { addTorrent, selectTorrentFile, selectDownloadDirectory, getSystemPathPresets } = useTorrentStore();
  const { settings } = useSettingsStore();

  const [torrentSource, setTorrentSource] = useState('');
  const [downloadDir, setDownloadDir] = useState(settings.download_directory);
  const [selectedFileName, setSelectedFileName] = useState<string | null>(null);
  const [selectedFileSize, setSelectedFileSize] = useState<number | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [presets, setPresets] = useState<SystemPathPresets | null>(null);

  const fileInputRef = useRef<HTMLInputElement>(null);
  const folderInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!showAddTorrent) return;

    const unlistenList: Array<() => void> = [];

    listen<{ paths?: string[] }>('tauri://drag-drop', (event) => {
      const paths = event.payload?.paths;
      if (paths && paths.length > 0) {
        const filePath = paths[0];
        const name = filePath.split('/').pop()?.split('\\').pop() || filePath;
        setSelectedFileName(name);
        setSelectedFileSize(null);
        setTorrentSource(filePath);
      }
    })
      .then((un) => unlistenList.push(un))
      .catch(() => {});

    listen<string>('nebulae://file-dropped', (event) => {
      if (event.payload) {
        const filePath = event.payload;
        const name = filePath.split('/').pop()?.split('\\').pop() || filePath;
        setSelectedFileName(name);
        setSelectedFileSize(null);
        setTorrentSource(filePath);
      }
    })
      .then((un) => unlistenList.push(un))
      .catch(() => {});

    return () => {
      for (const un of unlistenList) un();
    };
  }, [showAddTorrent]);

  useEffect(() => {
    if (initialAddTorrentSource) {
      setTorrentSource(initialAddTorrentSource);
      const name = initialAddTorrentSource.split('/').pop()?.split('\\').pop() || initialAddTorrentSource;
      setSelectedFileName(name);
      setInitialAddTorrentSource(null);
    }
  }, [initialAddTorrentSource, setInitialAddTorrentSource]);

  useEffect(() => {
    getSystemPathPresets().then((res) => {
      setPresets(res);
      if (!downloadDir && res.downloads) {
        setDownloadDir(res.downloads);
      }
    });
  }, [getSystemPathPresets]);

  if (!showAddTorrent) return null;

  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  const handleChooseFile = async () => {
    try {
      const nativePath = await selectTorrentFile();
      if (nativePath) {
        const name = nativePath.split('/').pop()?.split('\\').pop() || nativePath;
        setSelectedFileName(name);
        setSelectedFileSize(null);
        setTorrentSource(nativePath);
        return;
      }
    } catch {
      // Fall through
    }
    fileInputRef.current?.click();
  };

  const handleFileInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (files && files.length > 0) {
      const file = files[0];
      setSelectedFileName(file.name);
      setSelectedFileSize(file.size);
      setTorrentSource(file.name);
    }
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      const file = e.dataTransfer.files[0];
      const source = (file as any).path || file.name;
      setSelectedFileName(file.name);
      setSelectedFileSize(file.size);
      setTorrentSource(source);
    }
  };

  const handleBrowseFolder = async () => {
    try {
      const folder = await selectDownloadDirectory();
      if (folder) {
        setDownloadDir(folder);
        return;
      }
    } catch {
      // Fall through
    }
    folderInputRef.current?.click();
  };

  const handleFolderInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (files && files.length > 0) {
      const path = (files[0] as any).path;
      if (path) {
        const dir = path.substring(0, path.lastIndexOf('/'));
        if (dir) setDownloadDir(dir);
      }
    }
  };

  const handleClearSelectedFile = (e: React.MouseEvent) => {
    e.stopPropagation();
    setSelectedFileName(null);
    setSelectedFileSize(null);
    setTorrentSource('');
    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const source = (torrentSource || selectedFileName || '').trim();
    if (!source) return;

    setIsSubmitting(true);
    await addTorrent(source, downloadDir);
    setIsSubmitting(false);
    setTorrentSource('');
    setSelectedFileName(null);
    setSelectedFileSize(null);
    setShowAddTorrent(false);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm p-4 font-sans select-none">
      <div className="relative w-full max-w-lg bg-[#141414] border border-[#333333] rounded-xl shadow-2xl overflow-hidden flex flex-col text-white">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-[#333333] bg-[#1f1f1f]">
          <div className="flex items-center gap-2">
            <Plus className="w-4 h-4 text-[#e50914]" />
            <h2 className="text-sm font-bold text-white">Add Torrent</h2>
          </div>
          <button
            onClick={() => setShowAddTorrent(false)}
            className="p-1 rounded text-[#a3a3a3] hover:text-white transition"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Hidden File & Folder Inputs */}
        <input
          ref={fileInputRef}
          type="file"
          accept=".torrent,.adc,application/x-bittorrent,application/x-asiandvdclub-torrent"
          onChange={handleFileInputChange}
          className="hidden"
        />
        <input
          ref={folderInputRef}
          type="file"
          // @ts-ignore
          webkitdirectory="true"
          directory=""
          onChange={handleFolderInputChange}
          className="hidden"
        />

        {/* Form Body */}
        <form onSubmit={handleSubmit} className="p-5 space-y-4 text-xs text-[#a3a3a3]">
          {/* File Selection Dropzone */}
          <div>
            <label className="block text-xs font-medium text-white mb-1.5">
              Torrent File
            </label>

            {selectedFileName ? (
              <div
                onClick={handleChooseFile}
                className="border border-[#333333] bg-[#1f1f1f] rounded-lg p-3 transition cursor-pointer flex items-center justify-between hover:border-[#e50914]"
              >
                <div className="flex items-center gap-2.5 min-w-0">
                  <FileCheck className="w-4 h-4 text-[#e50914] flex-shrink-0" />
                  <div className="min-w-0">
                    <span className="text-xs font-medium text-white truncate block max-w-[300px]">
                      {selectedFileName}
                    </span>
                    <span className="text-[10px] text-[#a3a3a3] font-mono">
                      {selectedFileSize ? formatFileSize(selectedFileSize) : 'Ready'}
                    </span>
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={handleClearSelectedFile}
                    className="p-1 rounded text-[#a3a3a3] hover:text-[#e50914] transition"
                  >
                    <X className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
            ) : (
              <div
                onClick={handleChooseFile}
                onDragOver={(e) => {
                  e.preventDefault();
                  setIsDragging(true);
                }}
                onDragLeave={() => setIsDragging(false)}
                onDrop={handleDrop}
                className={`border-2 border-dashed rounded-lg p-4 text-center transition cursor-pointer flex flex-col items-center justify-center gap-1.5 ${
                  isDragging
                    ? 'border-[#e50914] bg-[#e50914]/10'
                    : 'border-[#333333] hover:border-[#e50914] bg-[#1f1f1f]'
                }`}
              >
                <FileUp className="w-5 h-5 text-[#a3a3a3]" />
                <span className="text-xs text-white">
                  Click to select <span className="font-mono text-[#e50914]">.torrent</span> or <span className="font-mono text-[#e50914]">.adc</span> file
                </span>
                <span className="text-[10px] text-[#a3a3a3]">or drag and drop file here</span>
              </div>
            )}
          </div>

          {/* Magnet / Protocol URL input */}
          <div>
            <label className="block text-xs font-medium text-white mb-1.5">
              Magnet Link or ADC URL
            </label>
            <input
              type="text"
              placeholder="magnet:?xt=urn:btih:... or adc:?..."
              value={selectedFileName ? '' : torrentSource}
              disabled={!!selectedFileName}
              onChange={(e) => setTorrentSource(e.target.value)}
              className="w-full px-3 py-2 rounded-lg bg-[#1f1f1f] border border-[#333333] text-white font-mono text-xs focus:outline-none focus:border-[#e50914] disabled:opacity-40"
            />
          </div>

          {/* Save Destination Directory */}
          <div>
            <label className="block text-xs font-medium text-white mb-1.5">
              Save Destination
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                value={downloadDir}
                onChange={(e) => setDownloadDir(e.target.value)}
                className="flex-1 px-3 py-2 rounded-lg bg-[#1f1f1f] border border-[#333333] text-white font-mono text-xs focus:outline-none focus:border-[#e50914]"
                placeholder="/path/to/download"
              />
              <button
                type="button"
                onClick={handleBrowseFolder}
                className="px-3 py-2 rounded-lg bg-[#1f1f1f] hover:bg-[#262626] text-white flex items-center gap-1.5 text-xs transition border border-[#333333]"
              >
                <Folder className="w-3.5 h-3.5 text-[#e50914]" />
                <span>Browse...</span>
              </button>
            </div>

            {/* Quick Preset Buttons */}
            <div className="flex items-center gap-1.5 mt-2 flex-wrap text-[11px] font-mono">
              <span className="text-[#a3a3a3]">Quick:</span>
              {presets?.downloads && (
                <button
                  type="button"
                  onClick={() => setDownloadDir(presets.downloads!)}
                  className={`px-2 py-0.5 rounded transition border ${
                    downloadDir === presets.downloads
                      ? 'bg-[#e50914] text-white border-[#e50914]'
                      : 'bg-[#1f1f1f] text-[#a3a3a3] hover:text-white border-[#333333]'
                  }`}
                >
                  Downloads
                </button>
              )}
              {presets?.movies && (
                <button
                  type="button"
                  onClick={() => setDownloadDir(presets.movies!)}
                  className={`px-2 py-0.5 rounded transition border ${
                    downloadDir === presets.movies
                      ? 'bg-[#e50914] text-white border-[#e50914]'
                      : 'bg-[#1f1f1f] text-[#a3a3a3] hover:text-white border-[#333333]'
                  }`}
                >
                  Movies
                </button>
              )}
              {presets?.desktop && (
                <button
                  type="button"
                  onClick={() => setDownloadDir(presets.desktop!)}
                  className={`px-2 py-0.5 rounded transition border ${
                    downloadDir === presets.desktop
                      ? 'bg-[#e50914] text-white border-[#e50914]'
                      : 'bg-[#1f1f1f] text-[#a3a3a3] hover:text-white border-[#333333]'
                  }`}
                >
                  Desktop
                </button>
              )}
            </div>
          </div>

          {/* Footer Actions */}
          <div className="flex justify-end gap-2 pt-3 border-t border-[#333333]">
            <button
              type="button"
              onClick={() => setShowAddTorrent(false)}
              className="px-4 py-1.5 rounded-lg text-[#a3a3a3] hover:text-white transition"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSubmitting || (!torrentSource.trim() && !selectedFileName)}
              className="px-4 py-1.5 rounded-lg bg-[#e50914] hover:bg-[#f40612] disabled:opacity-40 text-white font-semibold transition flex items-center gap-1.5 shadow"
            >
              {isSubmitting ? 'Adding...' : 'Start Download'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};

export default AddTorrentModal;
