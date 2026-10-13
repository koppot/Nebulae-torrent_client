import React, { useState, useRef, useEffect } from 'react';
import { useUiStore } from '../../stores/uiStore';
import { useTorrentStore, SystemPathPresets } from '../../stores/torrentStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { X, Plus, Magnet, FileUp, Folder, FileCheck, CheckCircle2, FolderSearch } from 'lucide-react';

export const AddTorrentModal: React.FC = () => {
  const { showAddTorrent, setShowAddTorrent } = useUiStore();
  const { addTorrent, selectTorrentFile, selectDownloadDirectory, getSystemPathPresets } = useTorrentStore();
  const { settings } = useSettingsStore();

  const [torrentSource, setTorrentSource] = useState('');
  const [downloadDir, setDownloadDir] = useState(settings.download_directory);
  const [selectedFileName, setSelectedFileName] = useState<string | null>(null);
  const [selectedFileSize, setSelectedFileSize] = useState<number | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [isManualPathEdit, setIsManualPathEdit] = useState(false);
  const [presets, setPresets] = useState<SystemPathPresets | null>(null);

  const fileInputRef = useRef<HTMLInputElement>(null);
  const folderInputRef = useRef<HTMLInputElement>(null);

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
      // 1. Attempt native OS file dialog via Tauri
      const nativePath = await selectTorrentFile();
      if (nativePath) {
        const name = nativePath.split('/').pop()?.split('\\').pop() || nativePath;
        setSelectedFileName(name);
        setSelectedFileSize(null);
        setTorrentSource(nativePath);
        return;
      }
    } catch {
      // Fall through to HTML5 file input
    }

    // 2. Fallback to HTML5 file input (macOS WebKit / WebView2 native sheet)
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
      if (file.name.endsWith('.torrent') || file.type.includes('bittorrent')) {
        setSelectedFileName(file.name);
        setSelectedFileSize(file.size);
        setTorrentSource(file.name);
      }
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

    // Fallback to webkitdirectory folder picker
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
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4 animate-in fade-in duration-150">
      <div className="relative w-full max-w-lg bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl overflow-hidden flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-slate-800 bg-slate-950/40">
          <div className="flex items-center gap-2">
            <Plus className="w-5 h-5 text-adc-gold" />
            <h2 className="text-base font-semibold text-white">Add New Torrent</h2>
          </div>
          <button
            onClick={() => setShowAddTorrent(false)}
            className="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Hidden HTML5 File & Folder Inputs */}
        <input
          ref={fileInputRef}
          type="file"
          accept=".torrent,application/x-bittorrent"
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
        <form onSubmit={handleSubmit} className="p-6 space-y-5 text-xs text-slate-300">
          {/* File Selection Dropzone */}
          <div>
            <label className="block text-xs font-medium text-slate-300 mb-1.5 flex items-center gap-1.5">
              <FileUp className="w-3.5 h-3.5 text-adc-gold" />
              <span>Torrent Metainfo File</span>
            </label>

            {selectedFileName ? (
              <div
                onClick={handleChooseFile}
                className="relative border border-emerald-500/40 bg-emerald-950/20 hover:bg-emerald-950/30 rounded-xl p-4 transition cursor-pointer flex items-center justify-between group"
              >
                <div className="flex items-center gap-3 min-w-0">
                  <div className="p-2.5 rounded-lg bg-emerald-500/20 text-emerald-400 flex-shrink-0">
                    <FileCheck className="w-5 h-5" />
                  </div>
                  <div className="min-w-0">
                    <div className="flex items-center gap-1.5">
                      <span className="text-xs font-semibold text-white truncate max-w-[280px]">
                        {selectedFileName}
                      </span>
                      <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400 flex-shrink-0" />
                    </div>
                    <span className="text-[11px] text-slate-400 font-mono">
                      {selectedFileSize ? formatFileSize(selectedFileSize) : 'Selected via File Dialog'}
                    </span>
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  <span className="text-[10px] text-slate-400 group-hover:text-slate-200 underline">
                    Change
                  </span>
                  <button
                    type="button"
                    onClick={handleClearSelectedFile}
                    className="p-1.5 rounded-lg text-slate-400 hover:text-rose-400 hover:bg-rose-950/40 transition"
                    title="Remove selected file"
                  >
                    <X className="w-4 h-4" />
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
                className={`border-2 border-dashed rounded-xl p-5 text-center transition cursor-pointer flex flex-col items-center justify-center gap-2 ${
                  isDragging
                    ? 'border-adc-crimson bg-adc-crimson/10 scale-[1.01]'
                    : 'border-slate-700 hover:border-adc-crimson/60 bg-slate-950/50 hover:bg-slate-950'
                }`}
              >
                <div className="p-2.5 rounded-full bg-slate-800 text-slate-300">
                  <FileUp className="w-5 h-5" />
                </div>
                <div>
                  <span className="text-xs text-white font-medium block">
                    Click to browse and select <code className="text-adc-gold">.torrent</code> file
                  </span>
                  <span className="text-[11px] text-slate-400 mt-0.5 block">
                    or drag and drop metainfo file into this window
                  </span>
                </div>
              </div>
            )}
          </div>

          <div className="flex items-center gap-2">
            <div className="flex-1 h-[1px] bg-slate-800"></div>
            <span className="text-[10px] uppercase font-mono text-slate-500">Or enter magnet link</span>
            <div className="flex-1 h-[1px] bg-slate-800"></div>
          </div>

          {/* Magnet / URL input */}
          <div>
            <label className="block text-xs font-medium text-slate-300 mb-1.5 flex items-center gap-1.5">
              <Magnet className="w-3.5 h-3.5 text-adc-crimson" />
              <span>Magnet Link or URL</span>
            </label>
            <textarea
              rows={2}
              placeholder="magnet:?xt=urn:btih:... or adc://..."
              value={selectedFileName ? '' : torrentSource}
              disabled={!!selectedFileName}
              onChange={(e) => setTorrentSource(e.target.value)}
              className="w-full px-3 py-2 rounded-lg bg-slate-950 border border-slate-700 text-slate-100 font-mono text-xs focus:outline-none focus:border-adc-crimson disabled:opacity-40"
            />
          </div>

          {/* Destination Directory (Interactive Folder Selection) */}
          <div>
            <div className="flex items-center justify-between mb-1.5">
              <label className="text-xs font-medium text-slate-300 flex items-center gap-1.5">
                <Folder className="w-3.5 h-3.5 text-cyan-400" />
                <span>Save Destination Directory</span>
              </label>
              <button
                type="button"
                onClick={() => setIsManualPathEdit(!isManualPathEdit)}
                className="text-[11px] text-slate-400 hover:text-white transition underline"
              >
                {isManualPathEdit ? 'Pick via Directory Dialog' : 'Edit path manually'}
              </button>
            </div>

            {!isManualPathEdit ? (
              <div
                onClick={handleBrowseFolder}
                className="border border-slate-700 hover:border-cyan-500/60 bg-slate-950/60 hover:bg-slate-950 rounded-xl p-3 transition cursor-pointer flex items-center justify-between group"
              >
                <div className="flex items-center gap-2.5 min-w-0">
                  <div className="p-2 rounded-lg bg-cyan-950/60 text-cyan-400 border border-cyan-800/40 flex-shrink-0">
                    <FolderSearch className="w-4 h-4" />
                  </div>
                  <div className="min-w-0">
                    <span className="text-[10px] text-slate-400 block font-mono uppercase tracking-wider">
                      Target Folder:
                    </span>
                    <span className="text-xs font-mono text-white font-medium truncate block max-w-[310px]">
                      {downloadDir || 'Click to select folder...'}
                    </span>
                  </div>
                </div>
                <div className="flex items-center gap-1 text-[11px] text-cyan-400 group-hover:text-cyan-300 px-2.5 py-1 rounded-md bg-cyan-950/40 border border-cyan-800/30 flex-shrink-0">
                  <span>Browse Folder...</span>
                </div>
              </div>
            ) : (
              <div className="flex gap-2">
                <input
                  type="text"
                  value={downloadDir}
                  onChange={(e) => setDownloadDir(e.target.value)}
                  className="flex-1 px-3 py-2 rounded-lg bg-slate-950 border border-slate-700 text-slate-100 font-mono text-xs focus:outline-none focus:border-adc-gold"
                  placeholder="e.g. /Users/username/Downloads"
                />
                <button
                  type="button"
                  onClick={handleBrowseFolder}
                  className="px-3 py-2 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 flex items-center gap-1.5 text-xs transition border border-slate-700"
                  title="Browse Directory"
                >
                  <FolderSearch className="w-3.5 h-3.5" />
                  <span>Browse...</span>
                </button>
              </div>
            )}

            {/* Quick Preset Location Pills */}
            <div className="flex items-center gap-1.5 mt-2 flex-wrap">
              <span className="text-[10px] text-slate-500 uppercase font-mono mr-0.5">Quick Pick:</span>
              {presets?.downloads && (
                <button
                  type="button"
                  onClick={() => setDownloadDir(presets.downloads!)}
                  className={`px-2 py-0.5 rounded text-[11px] font-mono transition border ${
                    downloadDir === presets.downloads
                      ? 'bg-cyan-950 text-cyan-300 border-cyan-700 font-semibold'
                      : 'bg-slate-950/80 text-slate-400 hover:text-white border-slate-800 hover:border-slate-700'
                  }`}
                >
                  Downloads
                </button>
              )}
              {presets?.movies && (
                <button
                  type="button"
                  onClick={() => setDownloadDir(presets.movies!)}
                  className={`px-2 py-0.5 rounded text-[11px] font-mono transition border ${
                    downloadDir === presets.movies
                      ? 'bg-cyan-950 text-cyan-300 border-cyan-700 font-semibold'
                      : 'bg-slate-950/80 text-slate-400 hover:text-white border-slate-800 hover:border-slate-700'
                  }`}
                >
                  Movies
                </button>
              )}
              {presets?.desktop && (
                <button
                  type="button"
                  onClick={() => setDownloadDir(presets.desktop!)}
                  className={`px-2 py-0.5 rounded text-[11px] font-mono transition border ${
                    downloadDir === presets.desktop
                      ? 'bg-cyan-950 text-cyan-300 border-cyan-700 font-semibold'
                      : 'bg-slate-950/80 text-slate-400 hover:text-white border-slate-800 hover:border-slate-700'
                  }`}
                >
                  Desktop
                </button>
              )}
              <button
                type="button"
                onClick={handleBrowseFolder}
                className="px-2 py-0.5 rounded text-[11px] font-mono text-adc-gold hover:text-amber-300 bg-amber-950/20 border border-amber-800/40 transition"
              >
                + Choose Other Folder...
              </button>
            </div>
          </div>

          {/* Footer Controls */}
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
              disabled={isSubmitting || (!torrentSource.trim() && !selectedFileName)}
              className="px-5 py-2 rounded-lg bg-adc-crimson hover:bg-red-700 disabled:opacity-50 text-white font-semibold transition flex items-center gap-1.5 shadow-lg shadow-adc-crimson/20"
            >
              {isSubmitting ? 'Adding...' : 'Start Download'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
