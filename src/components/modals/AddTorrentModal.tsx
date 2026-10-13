import React, { useState, useRef, useEffect } from 'react';
import { useUiStore } from '../../stores/uiStore';
import { useTorrentStore } from '../../stores/torrentStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { X, Plus, FileUp, Folder, FileCheck, Layers, Play } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';

export const AddTorrentModal: React.FC = () => {
  const { showAddTorrent, setShowAddTorrent, initialAddTorrentSource, setInitialAddTorrentSource } =
    useUiStore();
  const { addTorrent, selectTorrentFile, selectDownloadDirectory } = useTorrentStore();
  const { settings, addRecentDirectory } = useSettingsStore();

  const [torrentSource, setTorrentSource] = useState('');
  const [downloadDir, setDownloadDir] = useState(settings.download_directory);
  const [selectedFileName, setSelectedFileName] = useState<string | null>(null);
  const [selectedFileSize, setSelectedFileSize] = useState<number | null>(null);
  const [category, setCategory] = useState('General');
  const [startImmediately, setStartImmediately] = useState(!settings.do_not_start_automatically);
  const [sequentialDownload, setSequentialDownload] = useState(false);
  const [firstLastPiecePrio, setFirstLastPiecePrio] = useState(true);
  const [isDragging, setIsDragging] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);

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
      const name =
        initialAddTorrentSource.split('/').pop()?.split('\\').pop() || initialAddTorrentSource;
      setSelectedFileName(name);
      setInitialAddTorrentSource(null);
    }
  }, [initialAddTorrentSource, setInitialAddTorrentSource]);

  useEffect(() => {
    if (!downloadDir && settings.download_directory) {
      setDownloadDir(settings.download_directory);
    }
  }, [settings.download_directory, downloadDir]);

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
    if (downloadDir) {
      await addRecentDirectory(downloadDir);
    }
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
            <h2 className="text-sm font-bold text-white">Add Torrent (Metainfo Ingestion)</h2>
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
              Torrent Metainfo File (.torrent / .adc)
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
                      {selectedFileSize ? formatFileSize(selectedFileSize) : 'Ready for swarm connection'}
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
                className={`border-2 border-dashed rounded-lg p-5 text-center transition cursor-pointer flex flex-col items-center justify-center gap-1.5 ${
                  isDragging
                    ? 'border-[#e50914] bg-[#e50914]/10'
                    : 'border-[#333333] hover:border-[#e50914] bg-[#1f1f1f]'
                }`}
              >
                <FileUp className="w-6 h-6 text-[#a3a3a3]" />
                <span className="text-xs text-white">
                  Click to select <span className="font-mono text-[#e50914]">.torrent</span> or{' '}
                  <span className="font-mono text-[#e50914]">.adc</span> file
                </span>
                <span className="text-[10px] text-[#a3a3a3]">
                  or drag and drop metainfo file here
                </span>
              </div>
            )}
          </div>

          {/* Category Assignment */}
          <div>
            <label className="block text-xs font-medium text-white mb-1.5 flex items-center gap-1.5">
              <Layers className="w-3.5 h-3.5 text-[#e50914]" />
              <span>Category</span>
            </label>
            <select
              value={category}
              onChange={(e) => setCategory(e.target.value)}
              className="w-full px-3 py-1.5 rounded-lg bg-[#1f1f1f] border border-[#333333] text-white text-xs focus:outline-none focus:border-[#e50914]"
            >
              <option value="General">General (Default)</option>
              <option value="Movies">Movies & Cinema</option>
              <option value="Anime">Anime & Animation</option>
              <option value="TV">TV Series</option>
              <option value="Music">Lossless Audio & Music</option>
              <option value="Software">Software & Utilities</option>
            </select>
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
                className="flex-1 px-3 py-1.5 rounded-lg bg-[#1f1f1f] border border-[#333333] text-white font-mono text-xs focus:outline-none focus:border-[#e50914]"
                placeholder="/path/to/download"
              />
              <button
                type="button"
                onClick={handleBrowseFolder}
                className="px-3 py-1.5 rounded-lg bg-[#1f1f1f] hover:bg-[#262626] text-white flex items-center gap-1.5 text-xs transition border border-[#333333]"
              >
                <Folder className="w-3.5 h-3.5 text-[#e50914]" />
                <span>Browse...</span>
              </button>
            </div>

            {/* Recent Destination Buttons */}
            {settings.recent_download_directories && settings.recent_download_directories.length > 0 && (
              <div className="flex items-center gap-1.5 mt-2 flex-wrap text-[11px] font-mono">
                <span className="text-[#a3a3a3]">Recent:</span>
                {settings.recent_download_directories.map((dirPath) => {
                  const label = dirPath.split('/').pop()?.split('\\').pop() || dirPath;
                  const isCurrent = downloadDir === dirPath;
                  return (
                    <button
                      key={dirPath}
                      type="button"
                      onClick={() => setDownloadDir(dirPath)}
                      title={dirPath}
                      className={`px-2 py-0.5 rounded transition border truncate max-w-[160px] ${
                        isCurrent
                          ? 'bg-[#e50914] text-white border-[#e50914]'
                          : 'bg-[#1f1f1f] text-[#a3a3a3] hover:text-white border-[#333333]'
                      }`}
                    >
                      {label}
                    </button>
                  );
                })}
              </div>
            )}
          </div>

          {/* Download Options Checkboxes */}
          <div className="pt-2 border-t border-[#333333] space-y-1.5">
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={startImmediately}
                onChange={(e) => setStartImmediately(e.target.checked)}
                className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
              />
              <span className="text-white">Start torrent immediately</span>
            </label>
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={sequentialDownload}
                onChange={(e) => setSequentialDownload(e.target.checked)}
                className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
              />
              <span className="text-white">Download in sequential order (media stream preview)</span>
            </label>
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={firstLastPiecePrio}
                onChange={(e) => setFirstLastPiecePrio(e.target.checked)}
                className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
              />
              <span className="text-white">Download first and last pieces first</span>
            </label>
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
              className="px-5 py-1.5 rounded-lg bg-[#e50914] hover:bg-[#f40612] disabled:opacity-40 text-white font-semibold transition flex items-center gap-1.5 shadow"
            >
              <Play className="w-3.5 h-3.5 fill-current" />
              <span>{isSubmitting ? 'Adding...' : 'Add Torrent'}</span>
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};

export default AddTorrentModal;
