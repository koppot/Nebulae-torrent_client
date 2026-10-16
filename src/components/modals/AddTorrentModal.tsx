import React, { useState, useRef, useEffect } from 'react';
import { useUiStore } from '../../stores/uiStore';
import { useTorrentStore } from '../../stores/torrentStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { InspectTorrentResult, TorrentContentMatch, FileWithPath } from '../../types';
import { errorMessage as toErrorMessage } from '../../utils/errors';
import {
  X,
  Plus,
  FileUp,
  Folder,
  FileCheck,
  Layers,
  Play,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  AlertCircle,
  Link as LinkIcon,
} from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';

export const AddTorrentModal: React.FC = () => {
  const { showAddTorrent, setShowAddTorrent, initialAddTorrentSource, setInitialAddTorrentSource, addTorrentRejection, setAddTorrentRejection } =
    useUiStore();
  const { addTorrent, inspectTorrent, matchTorrentContent, applyTorrentRenames, selectTorrentFile, selectDownloadDirectory } = useTorrentStore();
  const { settings, addRecentDirectory, engineMode } = useSettingsStore();

  const [inputMode, setInputMode] = useState<'file' | 'magnet'>('file');
  const [torrentSource, setTorrentSource] = useState('');
  const [downloadDir, setDownloadDir] = useState(settings.download_directory);
  const [selectedFileName, setSelectedFileName] = useState<string | null>(null);
  const [selectedFileSize, setSelectedFileSize] = useState<number | null>(null);
  const [selectedFileBytes, setSelectedFileBytes] = useState<number[] | null>(null);
  const [inspectPreview, setInspectPreview] = useState<InspectTorrentResult | null>(null);
  const [inspectLoading, setInspectLoading] = useState(false);
  const [showFilePreviewList, setShowFilePreviewList] = useState(false);
  const [category, setCategory] = useState('General');
  const [startImmediately, setStartImmediately] = useState(!settings.do_not_start_automatically);
  const [sequentialDownload, setSequentialDownload] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [contentMatch, setContentMatch] = useState<TorrentContentMatch | null>(null);
  const [matchLoading, setMatchLoading] = useState(false);
  const [renameInputs, setRenameInputs] = useState<Record<number, string>>({});

  // Only .torrent and .adc metainfo files may be added; everything else is
  // rejected instead of being accepted as if it were a torrent.
  const isAllowedMetainfoName = (name: string) => /\.(torrent|adc)$/i.test(name.trim());

  const fileInputRef = useRef<HTMLInputElement>(null);
  const folderInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!showAddTorrent) return;

    const unlistenList: Array<() => void> = [];

    // Surface rejections raised outside the modal (global drop, Finder open).
    if (addTorrentRejection) {
      setErrorMessage(addTorrentRejection);
      setAddTorrentRejection(null);
    }

    listen<{ paths?: string[] }>('tauri://drag-drop', (event) => {
      const paths = event.payload?.paths;
      if (paths && paths.length > 0) {
        const filePath = paths[0];
        const name = filePath.split('/').pop()?.split('\\').pop() || filePath;
        if (!isAllowedMetainfoName(name)) {
          setErrorMessage(`only .torrent or .adc files are accepted ('${name}' was ignored).`);
          return;
        }
        setSelectedFileName(name);
        setSelectedFileSize(null);
        setSelectedFileBytes(null);
        setTorrentSource(filePath);
        setInspectLoading(true);
        inspectTorrent(filePath)
          .then((preview) => setInspectPreview(preview))
          .catch(() => setInspectPreview(null))
          .finally(() => setInspectLoading(false));
      }
    })
      .then((un) => unlistenList.push(un))
      .catch(() => {});

    listen<string>('nebulae://file-dropped', (event) => {
      if (event.payload) {
        const filePath = event.payload;
        const name = filePath.split('/').pop()?.split('\\').pop() || filePath;
        if (!isAllowedMetainfoName(name)) {
          setErrorMessage(`only .torrent or .adc files are accepted ('${name}' was ignored).`);
          return;
        }
        setSelectedFileName(name);
        setSelectedFileSize(null);
        setSelectedFileBytes(null);
        setTorrentSource(filePath);
        setInspectLoading(true);
        inspectTorrent(filePath)
          .then((preview) => setInspectPreview(preview))
          .catch(() => setInspectPreview(null))
          .finally(() => setInspectLoading(false));
      }
    })
      .then((un) => unlistenList.push(un))
      .catch(() => {});

    return () => {
      for (const un of unlistenList) un();
    };
  }, [showAddTorrent, inspectTorrent, addTorrentRejection, setAddTorrentRejection]);

  useEffect(() => {
    if (initialAddTorrentSource) {
      setTorrentSource(initialAddTorrentSource);
      if (
        initialAddTorrentSource.startsWith('magnet:') ||
        initialAddTorrentSource.startsWith('http://') ||
        initialAddTorrentSource.startsWith('https://')
      ) {
        setInputMode('magnet');
      } else {
        setInputMode('file');
      }
      const name =
        initialAddTorrentSource.split('/').pop()?.split('\\').pop() || initialAddTorrentSource;
      setSelectedFileName(name);
      setInitialAddTorrentSource(null);
      setInspectLoading(true);
      inspectTorrent(initialAddTorrentSource)
        .then((preview) => setInspectPreview(preview))
        .catch(() => setInspectPreview(null))
        .finally(() => setInspectLoading(false));
    }
  }, [initialAddTorrentSource, setInitialAddTorrentSource, inspectTorrent]);

  // Shared submit path used by both the form button and the Enter key, so the
  // rename mapping is always applied before the torrent is added.
  const submitAdd = async () => {
    const source = (torrentSource || selectedFileName || '').trim();
    if (!source || isSubmitting) return;
    const targetDir = (downloadDir || settings.download_directory || '').trim();
    try {
      setIsSubmitting(true);
      setErrorMessage(null);
      // Apply any rename mapping first: move the user's existing files into
      // the torrent's expected layout so the initial recheck finds the data
      // and seeding keeps working with the original info-hash.
      if (contentMatch) {
        const renames = contentMatch.files
          .map((f) => ({ from: (renameInputs[f.index] ?? '').trim(), to: f.expected_rel_path }))
          .filter((r) => r.from && r.from !== r.to)
          .map((r) => ({ from_rel: r.from, to_rel: r.to }));
        if (renames.length > 0) {
          await applyTorrentRenames(targetDir, renames);
        }
      }
      await addTorrent(source, targetDir, selectedFileBytes || undefined);
      if (targetDir) {
        await addRecentDirectory(targetDir);
      }
      setTorrentSource('');
      setSelectedFileName(null);
      setSelectedFileSize(null);
      setSelectedFileBytes(null);
      setInspectPreview(null);
      setContentMatch(null);
      setRenameInputs({});
      setShowAddTorrent(false);
    } catch (err: unknown) {
      setErrorMessage(toErrorMessage(err, 'Failed to add torrent'));
    } finally {
      setIsSubmitting(false);
    }
  };

  useEffect(() => {
    if (!showAddTorrent) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Enter') {
        const source = (torrentSource || selectedFileName || '').trim();
        if (source && !isSubmitting) {
          e.preventDefault();
          submitAdd();
        }
      } else if (e.key === 'Escape') {
        e.preventDefault();
        setShowAddTorrent(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    showAddTorrent,
    torrentSource,
    selectedFileName,
    selectedFileBytes,
    isSubmitting,
    downloadDir,
    settings.download_directory,
    contentMatch,
    renameInputs,
    addTorrent,
    applyTorrentRenames,
    addRecentDirectory,
    setShowAddTorrent,
  ]);

  // A different torrent or save destination invalidates any previous match.
  // NOTE: must stay ABOVE the `if (!showAddTorrent) return null` early
  // return — hooks may never run conditionally, or React crashes the whole
  // tree ("Rendered more hooks than during the previous render"), leaving a
  // blank screen when the modal opens.
  useEffect(() => {
    setContentMatch(null);
    setRenameInputs({});
  }, [torrentSource, downloadDir]);

  // Native drag-and-drop with REAL absolute paths. The webview strips
  // File.path from HTML5 drop events, so drops handled only by the inline
  // onDrop produced a bare filename — "Find My Files" (Rename Existing
  // Content to Seed) then received a cwd-relative path and could never
  // match anything, which presented as a dead button. Tauri's webview-level
  // drag-drop event carries the actual paths.
  // Dedupe guard: the HTML5 drop fallback may also fire for the same drop.
  const lastNativeDropRef = useRef(0);
  useEffect(() => {
    if (!showAddTorrent) return;
    let alive = true;
    let unlisten: (() => void) | null = null;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type !== 'drop' || !alive) return;
        const now = Date.now();
        if (now - lastNativeDropRef.current < 800) return;
        const paths = (event.payload as { type: string; paths?: string[] }).paths || [];
        const path = paths.find((p) => isAllowedMetainfoName(p.split('/').pop() || p));
        if (!path) return;
        lastNativeDropRef.current = now;
        const name = path.split('/').pop()?.split('\\').pop() || path;
        setSelectedFileName(name);
        setSelectedFileSize(null);
        setSelectedFileBytes(null);
        setTorrentSource(path);
        setInspectLoading(true);
        inspectTorrent(path)
          .then((preview) => {
            if (alive) setInspectPreview(preview);
          })
          .catch(() => {
            if (alive) setInspectPreview(null);
          })
          .finally(() => {
            if (alive) setInspectLoading(false);
          });
      })
      .then((un) => {
        if (!alive) un();
        else unlisten = un;
      })
      .catch(() => {});
    return () => {
      alive = false;
      if (unlisten) unlisten();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showAddTorrent]);

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
        setSelectedFileBytes(null);
        setTorrentSource(nativePath);
        setInspectLoading(true);
        try {
          const preview = await inspectTorrent(nativePath);
          setInspectPreview(preview);
        } catch {
          setInspectPreview(null);
        } finally {
          setInspectLoading(false);
        }
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
      if (!isAllowedMetainfoName(file.name)) {
        setErrorMessage(
          `only .torrent or .adc files are accepted ('${file.name}' was ignored).`
        );
        e.target.value = '';
        return;
      }
      setSelectedFileName(file.name);
      setSelectedFileSize(file.size);
      // The HTML5 file-input fallback cannot know the absolute path (the
      // webview strips File.path). Keep the bare name for the byte-based
      // inspect flow; "Find My Files" explains itself when the path is
      // missing instead of failing silently.
      setTorrentSource(file.name);

      const reader = new FileReader();
      reader.onload = async () => {
        if (reader.result instanceof ArrayBuffer) {
          const bytes = Array.from(new Uint8Array(reader.result));
          setSelectedFileBytes(bytes);
          setInspectLoading(true);
          try {
            const preview = await inspectTorrent(undefined, bytes);
            setInspectPreview(preview);
          } catch {
            setInspectPreview(null);
          } finally {
            setInspectLoading(false);
          }
        }
      };
      reader.readAsArrayBuffer(file);
    }
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      const file = e.dataTransfer.files[0];
      if (!isAllowedMetainfoName(file.name)) {
        setErrorMessage(
          `only .torrent or .adc files are accepted ('${file.name}' was ignored).`
        );
        return;
      }
      // The webview strips File.path on drops; the native drag-drop event
      // (handled in the effect below) supplies the real absolute path. This
      // inline handler stays as a fallback with the bare name only.
      const source = (file as FileWithPath).path || file.name;
      setSelectedFileName(file.name);
      setSelectedFileSize(file.size);
      setTorrentSource(source);

      const reader = new FileReader();
      reader.onload = async () => {
        if (reader.result instanceof ArrayBuffer) {
          const bytes = Array.from(new Uint8Array(reader.result));
          setSelectedFileBytes(bytes);
          setInspectLoading(true);
          try {
            const preview = await inspectTorrent(undefined, bytes);
            setInspectPreview(preview);
          } catch {
            setInspectPreview(null);
          } finally {
            setInspectLoading(false);
          }
        }
      };
      reader.readAsArrayBuffer(file);
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
      const path = (files[0] as FileWithPath).path;
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
    setSelectedFileBytes(null);
    setInspectPreview(null);
    setTorrentSource('');
    setContentMatch(null);
    setRenameInputs({});
    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  const handleFindContent = async () => {
    const source = (torrentSource || selectedFileName || '').trim();
    const targetDir = (downloadDir || settings.download_directory || '').trim();
    if (!source || !targetDir || matchLoading) return;
    // A bare filename (no directory component) means the webview could not
    // give us the absolute path — explain that instead of failing with a
    // generic scan error.
    if (!source.includes('/') && !source.includes('\\')) {
      setErrorMessage(
        "The torrent file's location isn't available, so existing content can't be matched. Please pick the .torrent again using the file picker."
      );
      return;
    }
    setMatchLoading(true);
    try {
      const match = await matchTorrentContent(source, targetDir);
      setContentMatch(match);
      const inputs: Record<number, string> = {};
      for (const f of match.files) {
        inputs[f.index] = f.matched_rel_path ?? '';
      }
      setRenameInputs(inputs);
      setErrorMessage(null);
    } catch (err: unknown) {
      setContentMatch(null);
      setRenameInputs({});
      setErrorMessage(
        toErrorMessage(err, 'Could not scan the save destination')
      );
    } finally {
      setMatchLoading(false);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    await submitAdd();
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
          // @ts-expect-error -- non-standard webview attributes for folder picking
          webkitdirectory="true"
          directory=""
          onChange={handleFolderInputChange}
          className="hidden"
        />

        {/* Form Body */}
        <form onSubmit={handleSubmit} className="p-5 space-y-4 text-xs text-[#a3a3a3]">
          {/* Source Mode Switcher */}
          <div className="flex border-b border-[#262626] mb-3 gap-2">
            <button
              type="button"
              onClick={() => setInputMode('file')}
              className={`pb-2 px-3 text-xs font-medium border-b-2 flex items-center gap-1.5 transition ${
                inputMode === 'file'
                  ? 'border-[#e50914] text-white'
                  : 'border-transparent text-[#737373] hover:text-white'
              }`}
            >
              <FileUp className="w-3.5 h-3.5" />
              <span>Torrent Metainfo File</span>
            </button>
            <button
              type="button"
              onClick={() => setInputMode('magnet')}
              className={`pb-2 px-3 text-xs font-medium border-b-2 flex items-center gap-1.5 transition ${
                inputMode === 'magnet'
                  ? 'border-[#e50914] text-white'
                  : 'border-transparent text-[#737373] hover:text-white'
              }`}
            >
              <LinkIcon className="w-3.5 h-3.5" />
              <span>Magnet Link / URL</span>
            </button>
          </div>

          {/* File Selection Dropzone or Magnet Input */}
          <div>
            {inputMode === 'file' ? (
              <>
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
              </>
            ) : (
              <div>
                <label className="block text-xs font-medium text-white mb-1.5 flex items-center justify-between">
                  <span>Magnet URI or Direct Web Torrent URL</span>
                  <span className="text-[10px] text-[#737373] font-mono">BEP 9 Metadata Exchange</span>
                </label>
                <textarea
                  value={torrentSource}
                  onChange={(e) => {
                    const val = e.target.value;
                    setTorrentSource(val);
                    setSelectedFileName(null);
                    setSelectedFileSize(null);
                    setSelectedFileBytes(null);
                    if (val.trim().startsWith('magnet:') || val.trim().startsWith('http')) {
                      setInspectLoading(true);
                      inspectTorrent(val.trim())
                        .then((preview) => setInspectPreview(preview))
                        .catch(() => setInspectPreview(null))
                        .finally(() => setInspectLoading(false));
                    } else {
                      setInspectPreview(null);
                    }
                  }}
                  rows={3}
                  placeholder="magnet:?xt=urn:btih:...&dn=... or https://tracker.asiandvdclub.org/..."
                  className="w-full px-3 py-2 rounded-lg bg-[#1f1f1f] border border-[#333333] text-white font-mono text-xs focus:outline-none focus:border-[#e50914] resize-none"
                />
              </div>
            )}

            {inspectLoading && (
              <div className="mt-2.5 p-3 rounded-lg bg-[#1f1f1f] border border-[#262626] flex items-center justify-center gap-2 text-xs text-[#a3a3a3]">
                <div className="w-3.5 h-3.5 border-2 border-[#e50914] border-t-transparent rounded-full animate-spin" />
                <span>Parsing Bencoded Torrent Metainfo...</span>
              </div>
            )}

            {inspectPreview && !inspectLoading && (
              <div className="mt-2.5 p-3 rounded-lg bg-[#1f1f1f] border border-[#262626] space-y-2 text-xs">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2 min-w-0">
                    <CheckCircle2 className="w-4 h-4 text-emerald-500 flex-shrink-0" />
                    <span className="font-semibold text-white truncate max-w-[280px]">
                      {inspectPreview.name}
                    </span>
                  </div>
                  <span className="text-[11px] text-[#a3a3a3] font-mono flex-shrink-0">
                    {formatFileSize(inspectPreview.total_bytes)} · {inspectPreview.files.length}{' '}
                    {inspectPreview.files.length === 1 ? 'file' : 'files'}
                  </span>
                </div>

                <div className="flex items-center justify-between text-[11px] text-[#a3a3a3] font-mono border-t border-[#2a2a2a] pt-1.5">
                  <span className="truncate max-w-[200px]" title={inspectPreview.info_hash}>
                    Hash: {inspectPreview.info_hash.substring(0, 16)}...
                  </span>
                  {inspectPreview.files.length > 0 && (
                    <button
                      type="button"
                      onClick={() => setShowFilePreviewList(!showFilePreviewList)}
                      className="flex items-center gap-1 text-[#e50914] hover:underline"
                    >
                      <span>{showFilePreviewList ? 'Hide Files' : 'Inspect Content'}</span>
                      {showFilePreviewList ? (
                        <ChevronUp className="w-3 h-3" />
                      ) : (
                        <ChevronDown className="w-3 h-3" />
                      )}
                    </button>
                  )}
                </div>

                {showFilePreviewList && inspectPreview.files.length > 0 && (
                  <div className="max-h-36 overflow-y-auto space-y-1 font-mono text-[11px] border-t border-[#2a2a2a] pt-1.5 pr-1">
                    {inspectPreview.files.slice(0, 25).map((f) => (
                      <div
                        key={f.index}
                        className="flex justify-between items-center py-0.5 text-[#cccccc] hover:text-white"
                      >
                        <span className="truncate max-w-[280px]" title={f.name}>
                          {f.name}
                        </span>
                        <span className="text-[#a3a3a3] text-[10px] flex-shrink-0 ml-2">
                          {formatFileSize(f.size_bytes)}
                        </span>
                      </div>
                    ))}
                    {inspectPreview.files.length > 25 && (
                      <div className="text-[10px] text-[#a3a3a3] text-center pt-1">
                        + {inspectPreview.files.length - 25} more files
                      </div>
                    )}
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Rename Existing Content — match files on disk to the torrent's
              expected layout so seeding keeps working (names are part of the
              info-hash and cannot be edited in the metainfo itself). */}
          {inspectPreview &&
            !inspectLoading &&
            inputMode === 'file' &&
            inspectPreview.files.length > 0 && (
              <div className="p-3 rounded-lg bg-[#1a1a1a] border border-[#262626] space-y-2 text-xs">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-1.5 min-w-0">
                    <Folder className="w-3.5 h-3.5 text-[#e50914] flex-shrink-0" />
                    <span className="font-semibold text-white">Rename Existing Content to Seed</span>
                  </div>
                  <button
                    type="button"
                    onClick={handleFindContent}
                    disabled={matchLoading || !(downloadDir || settings.download_directory)}
                    className="flex items-center gap-1 px-2 py-1 rounded-md bg-[#262626] hover:bg-[#333333] text-[11px] text-white disabled:opacity-40 disabled:cursor-not-allowed transition"
                  >
                    {matchLoading ? (
                      <div className="w-3 h-3 border-2 border-[#e50914] border-t-transparent rounded-full animate-spin" />
                    ) : (
                      <span>{contentMatch ? 'Rescan' : 'Find My Files'}</span>
                    )}
                  </button>
                </div>
                {contentMatch && (
                  <>
                    <p className="text-[10px] text-[#a3a3a3]">
                      Matched {contentMatch.matched_count} of {contentMatch.files.length} files by
                      size and name. Right side = where the file is on disk now; Nebulae renames it
                      to the torrent's expected path (left) before adding, so the original swarm
                      and info-hash stay intact.
                    </p>
                    <div className="max-h-44 overflow-y-auto space-y-1.5 border-t border-[#2a2a2a] pt-1.5 pr-1">
                      {contentMatch.files.map((f) => (
                        <div key={f.index} className="flex items-center gap-1.5">
                          <span
                            className="flex-1 truncate text-[10px] font-mono text-[#cccccc]"
                            title={f.expected_rel_path}
                          >
                            {f.expected_rel_path}
                          </span>
                          <span className="text-[#737373] text-[10px] flex-shrink-0">←</span>
                          <input
                            type="text"
                            value={renameInputs[f.index] ?? ''}
                            onChange={(e) =>
                              setRenameInputs((prev) => ({ ...prev, [f.index]: e.target.value }))
                            }
                            placeholder="path on disk (relative)"
                            className="flex-1 min-w-0 px-2 py-1 rounded-md bg-[#141414] border border-[#333333] text-[10px] font-mono text-white focus:outline-none focus:border-[#e50914]"
                          />
                        </div>
                      ))}
                    </div>
                  </>
                )}
              </div>
            )}

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
            {engineMode !== 'rtorrent_server' && (
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={sequentialDownload}
                  onChange={(e) => setSequentialDownload(e.target.checked)}
                  className="rounded border-[#333333] bg-[#1f1f1f] text-[#e50914] focus:ring-0"
                />
                <span className="text-white">Download in sequential order (media stream preview)</span>
              </label>
            )}
          </div>

          {/* Error Message */}
          {errorMessage && (
            <div className="p-3 rounded-lg bg-red-950/40 border border-red-800/60 text-xs text-red-200 flex items-start gap-2">
              <AlertCircle className="w-4 h-4 text-red-400 shrink-0 mt-0.5" />
              <span>The torrent could not be added: {errorMessage}</span>
            </div>
          )}

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
