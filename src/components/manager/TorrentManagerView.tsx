import React, { useEffect, useState, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTorrentStore } from '../../stores/torrentStore';
import { useUiStore, ManagerTab } from '../../stores/uiStore';
import { usePeeringStore } from '../../stores/peeringStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { formatBytes, formatSpeed, formatEta, formatRatio } from '../../lib/formatters';
import { TorrentFileItem } from '../../types';
import {
  Play,
  Pause,
  Trash2,
  HardDrive,
  Users,
  Activity,
  Layers,
  FileText,
  RotateCw,
  Radio,
  Copy,
  Edit2,
  ShieldCheck,
  AlertCircle,
  X,
  Folder,
  FolderOpen,
  ChevronRight,
  ChevronDown,
  Film,
  Disc,
  Music,
  File,
  Search,
  List,
  FolderTree,
  LineChart,
  Sliders,
  Zap,
} from 'lucide-react';

interface ContextMenuState {
  visible: boolean;
  x: number;
  y: number;
  torrentId: string;
}

export interface FileTreeNode {
  name: string;
  path: string;
  isFolder: boolean;
  size_bytes: number;
  file?: TorrentFileItem;
  children: FileTreeNode[];
}

function buildFileTree(files: TorrentFileItem[]): FileTreeNode {
  const root: FileTreeNode = {
    name: 'root',
    path: '',
    isFolder: true,
    size_bytes: 0,
    children: [],
  };

  for (const file of files) {
    const parts = file.name.replace(/\\/g, '/').split('/').filter(Boolean);
    let current = root;

    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      const isLast = i === parts.length - 1;
      const currentPath = parts.slice(0, i + 1).join('/');

      if (isLast) {
        current.children.push({
          name: part,
          path: currentPath,
          isFolder: false,
          size_bytes: file.size_bytes,
          file,
          children: [],
        });
      } else {
        let folderNode = current.children.find((c) => c.isFolder && c.name === part);
        if (!folderNode) {
          folderNode = {
            name: part,
            path: currentPath,
            isFolder: true,
            size_bytes: 0,
            children: [],
          };
          current.children.push(folderNode);
        }
        current = folderNode;
      }
    }
  }

  function computeFolderSize(node: FileTreeNode): number {
    if (!node.isFolder) {
      return node.size_bytes;
    }
    node.size_bytes = node.children.reduce((acc, c) => acc + computeFolderSize(c), 0);
    node.children.sort((a, b) => {
      if (a.isFolder === b.isFolder) {
        return a.name.localeCompare(b.name);
      }
      return a.isFolder ? -1 : 1;
    });
    return node.size_bytes;
  }

  computeFolderSize(root);
  return root;
}

function countFilesInNode(node: FileTreeNode): number {
  if (!node.isFolder) return 1;
  return node.children.reduce((acc, c) => acc + countFilesInNode(c), 0);
}

function countFoldersInNode(node: FileTreeNode): number {
  if (!node.isFolder) return 0;
  return 1 + node.children.reduce((acc, c) => acc + countFoldersInNode(c), 0);
}

function nodeMatchesFilter(node: FileTreeNode, query: string): boolean {
  if (!query) return true;
  const q = query.toLowerCase();
  if (node.name.toLowerCase().includes(q)) return true;
  if (node.isFolder) {
    return node.children.some((c) => nodeMatchesFilter(c, query));
  }
  return false;
}

export const TorrentManagerView: React.FC = () => {
  const {
    torrents,
    selectedId,
    setSelectedId,
    pauseTorrent,
    resumeTorrent,
    forceRecheck,
    reannounce,
    renameTorrent,
    renameTorrentFile,
    renameTorrentFolder,
    setFilePriority,
    toggleSequential,
    toggleFirstLast,
    toggleSuperSeeding,
    setTorrentLimits,
  } = useTorrentStore();

  const {
    activeManagerTab,
    setActiveManagerTab,
    managerCategoryFilter,
    setManagerCategoryFilter,
    searchQuery,
    setDeleteModalTarget,
  } = useUiStore();

  const { peers, fetchPeers } = usePeeringStore();
  const { engineMode } = useSettingsStore();
  // In rtorrent-server mode the GUI hides options the rtorrent engine cannot
  // fulfill instead of offering actions that only produce errors.
  const isRemoteServer = engineMode === 'rtorrent_server';

  const [contextMenu, setContextMenu] = useState<ContextMenuState>({
    visible: false,
    x: 0,
    y: 0,
    torrentId: '',
  });

  const [filesViewMode, setFilesViewMode] = useState<'tree' | 'flat'>('tree');
  const [fileFilter, setFileFilter] = useState('');
  const [collapsedFolders, setCollapsedFolders] = useState<Record<string, boolean>>({});

  const [renameModal, setRenameModal] = useState<{
    isOpen: boolean;
    type: 'torrent' | 'folder' | 'file';
    id: string;
    fileIndex?: number;
    folderPath?: string;
    currentName: string;
    newName: string;
    savePath?: string;
  }>({
    isOpen: false,
    type: 'torrent',
    id: '',
    currentName: '',
    newName: '',
  });

  const [renameError, setRenameError] = useState<string | null>(null);

  const [speedHistory, setSpeedHistory] = useState<Array<{ dl: number; ul: number }>>([]);
  const [limitsModal, setLimitsModal] = useState<{
    isOpen: boolean;
    id: string;
    dlKib: number;
    ulKib: number;
    ratioLimit: number;
  }>({
    isOpen: false,
    id: '',
    dlKib: 0,
    ulKib: 0,
    ratioLimit: -1,
  });
  const [superSeedingNotice, setSuperSeedingNotice] = useState<string | null>(null);

  useEffect(() => {
    if (activeManagerTab === 'peers' && selectedId) {
      fetchPeers(selectedId);
      const interval = setInterval(() => {
        fetchPeers(selectedId);
      }, 2500);
      return () => clearInterval(interval);
    } else if (selectedId) {
      fetchPeers(selectedId);
    }
  }, [selectedId, activeManagerTab, fetchPeers]);

  // Filtered list — declared BEFORE the keyboard effect below, whose deps
  // reference it during render.
  const filteredTorrents = torrents.filter((t) => {
    if (searchQuery && !t.name.toLowerCase().includes(searchQuery.toLowerCase())) {
      return false;
    }
    if (managerCategoryFilter === 'downloading') return t.status === 'Downloading';
    if (managerCategoryFilter === 'seeding') return t.status === 'Seeding';
    if (managerCategoryFilter === 'completed') return t.status === 'Completed';
    if (managerCategoryFilter === 'paused') return t.status === 'Paused';
    if (managerCategoryFilter === 'media') return t.is_media;
    return true;
  });

  // Keyboard navigation & actions
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) {
        return;
      }

      if (!selectedId) return;
      const currentTorrent = torrents.find((t) => t.id === selectedId);
      if (!currentTorrent) return;

      if (e.key === 'Delete' || e.key === 'Backspace') {
        e.preventDefault();
        setDeleteModalTarget({ id: currentTorrent.id, name: currentTorrent.name });
      } else if (e.key === 'F2') {
        e.preventDefault();
        setRenameModal({
          isOpen: true,
          type: 'torrent',
          id: currentTorrent.id,
          currentName: currentTorrent.name,
          newName: currentTorrent.name,
          savePath: currentTorrent.save_path,
        });
      } else if (e.key === ' ') {
        e.preventDefault();
        if (currentTorrent.status === 'Downloading' || currentTorrent.status === 'Seeding') {
          pauseTorrent(selectedId);
        } else {
          resumeTorrent(selectedId);
        }
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        const currentIndex = filteredTorrents.findIndex((t) => t.id === selectedId);
        if (currentIndex < filteredTorrents.length - 1) {
          setSelectedId(filteredTorrents[currentIndex + 1].id);
        }
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        const currentIndex = filteredTorrents.findIndex((t) => t.id === selectedId);
        if (currentIndex > 0) {
          setSelectedId(filteredTorrents[currentIndex - 1].id);
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [selectedId, torrents, filteredTorrents, pauseTorrent, resumeTorrent, setDeleteModalTarget, setSelectedId]);

  // Close context menu on outside click
  useEffect(() => {
    const handleClickOutside = () => {
      if (contextMenu.visible) {
        setContextMenu((prev) => ({ ...prev, visible: false }));
      }
    };
    window.addEventListener('click', handleClickOutside);
    return () => window.removeEventListener('click', handleClickOutside);
  }, [contextMenu.visible]);

  const selectedTorrent = torrents.find((t) => t.id === selectedId) || filteredTorrents[0];

  useEffect(() => {
    if (selectedTorrent) {
      setSpeedHistory((prev) => {
        const next = [...prev, { dl: selectedTorrent.download_rate, ul: selectedTorrent.upload_rate }];
        return next.slice(-40);
      });
    }
    // Intentional: sample only when the rates themselves change, not on every
    // torrents refresh (identity churn would resample without new data).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedTorrent?.download_rate, selectedTorrent?.upload_rate]);

  const handleContextMenu = (e: React.MouseEvent, torrentId: string) => {
    e.preventDefault();
    setSelectedId(torrentId);
    setContextMenu({
      visible: true,
      x: e.clientX,
      y: e.clientY,
      torrentId,
    });
  };

  const handleCopyHash = () => {
    if (selectedTorrent?.info_hash) {
      navigator.clipboard.writeText(selectedTorrent.info_hash);
    }
  };

  const openRenameTorrentModal = () => {
    if (!selectedTorrent) return;
    setRenameError(null);
    setRenameModal({
      isOpen: true,
      type: 'torrent',
      id: selectedTorrent.id,
      currentName: selectedTorrent.name,
      newName: selectedTorrent.name,
      savePath: selectedTorrent.save_path,
    });
  };

  const openRenameFolderModal = (folderPath: string, folderName: string) => {
    if (!selectedTorrent) return;
    setRenameError(null);
    setRenameModal({
      isOpen: true,
      type: 'folder',
      id: selectedTorrent.id,
      folderPath,
      currentName: folderName,
      newName: folderName,
      savePath: selectedTorrent.save_path,
    });
  };

  const openRenameFileModal = (fileIndex: number, currentFilePath: string) => {
    if (!selectedTorrent) return;
    setRenameError(null);
    const parts = currentFilePath.replace(/\\/g, '/').split('/');
    const baseName = parts[parts.length - 1] || currentFilePath;
    setRenameModal({
      isOpen: true,
      type: 'file',
      id: selectedTorrent.id,
      fileIndex,
      currentName: baseName,
      newName: baseName,
      savePath: selectedTorrent.save_path,
    });
  };

  const handleConfirmRename = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = renameModal.newName.trim();
    if (!trimmed) {
      setRenameError('Name cannot be empty');
      return;
    }

    try {
      if (renameModal.type === 'torrent') {
        await renameTorrent(renameModal.id, trimmed);
      } else if (renameModal.type === 'folder' && renameModal.folderPath) {
        await renameTorrentFolder(renameModal.id, renameModal.folderPath, trimmed);
      } else if (renameModal.fileIndex !== undefined) {
        await renameTorrentFile(renameModal.id, renameModal.fileIndex, trimmed);
      }
      setRenameModal((prev) => ({ ...prev, isOpen: false }));
    } catch (err: unknown) {
      setRenameError(String(err));
    }
  };

  const fileTree = useMemo(() => {
    if (!selectedTorrent?.files || selectedTorrent.files.length === 0) {
      return null;
    }
    return buildFileTree(selectedTorrent.files);
  }, [selectedTorrent?.files]);

  const totalFoldersCount = useMemo(() => {
    if (!fileTree) return 0;
    return Math.max(0, countFoldersInNode(fileTree) - 1);
  }, [fileTree]);

  const filteredFlatFiles = useMemo(() => {
    if (!selectedTorrent?.files) return [];
    if (!fileFilter) return selectedTorrent.files;
    const q = fileFilter.toLowerCase();
    return selectedTorrent.files.filter((f) => f.name.toLowerCase().includes(q));
  }, [selectedTorrent?.files, fileFilter]);

  const toggleFolder = (folderPath: string) => {
    setCollapsedFolders((prev) => ({
      ...prev,
      [folderPath]: !prev[folderPath],
    }));
  };

  const getFileIcon = (fileName: string) => {
    const lower = fileName.toLowerCase();
    if (
      lower.endsWith('.mkv') ||
      lower.endsWith('.mp4') ||
      lower.endsWith('.avi') ||
      lower.endsWith('.m2ts') ||
      lower.endsWith('.ts')
    ) {
      return <Film className="w-3.5 h-3.5 text-blue-400 flex-shrink-0" />;
    }
    if (
      lower.endsWith('.iso') ||
      lower.endsWith('.img') ||
      lower.endsWith('.bdmv') ||
      lower.endsWith('.clpi') ||
      lower.endsWith('.mpls')
    ) {
      return <Disc className="w-3.5 h-3.5 text-purple-400 flex-shrink-0" />;
    }
    if (
      lower.endsWith('.srt') ||
      lower.endsWith('.sub') ||
      lower.endsWith('.idx') ||
      lower.endsWith('.vtt')
    ) {
      return <FileText className="w-3.5 h-3.5 text-yellow-400 flex-shrink-0" />;
    }
    if (
      lower.endsWith('.flac') ||
      lower.endsWith('.mp3') ||
      lower.endsWith('.wav') ||
      lower.endsWith('.ac3') ||
      lower.endsWith('.dts')
    ) {
      return <Music className="w-3.5 h-3.5 text-emerald-400 flex-shrink-0" />;
    }
    if (lower.endsWith('.nfo') || lower.endsWith('.txt') || lower.endsWith('.md')) {
      return <FileText className="w-3.5 h-3.5 text-[#a3a3a3] flex-shrink-0" />;
    }
    return <File className="w-3.5 h-3.5 text-[#e50914] flex-shrink-0" />;
  };

  const isMediaFileName = (name: string) =>
    /\.(mkv|mp4|avi|mov|wmv|flv|webm|m4v|ts|iso|mp3|flac|wav|m4a|aac)$/i.test(name);

  const renderTreeNode = (node: FileTreeNode, depth = 0): React.ReactNode => {
    if (node.isFolder) {
      const isCollapsed = !!collapsedFolders[node.path];
      const fileCount = countFilesInNode(node);

      if (fileFilter && !nodeMatchesFilter(node, fileFilter)) {
        return null;
      }

      return (
        <React.Fragment key={node.path || node.name}>
          <div
            className="group flex items-center justify-between p-1.5 rounded hover:bg-[#222222] border border-transparent hover:border-[#333333] transition text-xs"
            style={{ paddingLeft: `${depth * 18 + 8}px` }}
          >
            <div className="flex items-center gap-2 min-w-0">
              <button
                type="button"
                onClick={() => toggleFolder(node.path)}
                className="p-0.5 rounded hover:bg-[#333333] text-[#a3a3a3] hover:text-white"
                title={isCollapsed ? 'Expand folder' : 'Collapse folder'}
              >
                {isCollapsed ? (
                  <ChevronRight className="w-3.5 h-3.5" />
                ) : (
                  <ChevronDown className="w-3.5 h-3.5" />
                )}
              </button>
              {isCollapsed ? (
                <Folder className="w-4 h-4 text-[#e50914] flex-shrink-0" />
              ) : (
                <FolderOpen className="w-4 h-4 text-[#e50914] flex-shrink-0" />
              )}
              <span className="font-semibold text-white truncate max-w-sm font-sans">
                {node.name}
              </span>
              <span className="text-[10px] text-[#a3a3a3] font-mono">
                ({formatBytes(node.size_bytes)} · {fileCount} {fileCount === 1 ? 'file' : 'files'})
              </span>
            </div>

            <div className="flex items-center gap-2 flex-shrink-0 opacity-80 group-hover:opacity-100 transition">
              {!isRemoteServer && (
                <button
                  type="button"
                  onClick={() => openRenameFolderModal(node.path, node.name)}
                  className="px-2 py-0.5 rounded bg-[#141414] hover:bg-[#262626] border border-[#333333] text-[#a3a3a3] hover:text-white flex items-center gap-1 text-[10px] transition"
                  title={`Rename Folder "${node.name}" on disk`}
                >
                  <Edit2 className="w-3 h-3 text-[#e50914]" />
                  <span>Rename Folder</span>
                </button>
              )}
            </div>
          </div>

          {!isCollapsed && (
            <div className="space-y-0.5">
              {node.children.map((child) => renderTreeNode(child, depth + 1))}
            </div>
          )}
        </React.Fragment>
      );
    }

    if (fileFilter && !node.name.toLowerCase().includes(fileFilter.toLowerCase())) {
      return null;
    }

    const file = node.file!;
    return (
      <div
        key={file.index}
        className="group flex items-center justify-between p-1.5 rounded hover:bg-[#1f1f1f] border border-transparent hover:border-[#2a2a2a] transition text-xs font-mono"
        style={{ paddingLeft: `${depth * 18 + 14}px` }}
      >
        <div className="flex items-center gap-2 min-w-0">
          {getFileIcon(node.name)}
          <span className="text-white truncate max-w-md font-sans font-medium" title={file.name}>
            {node.name}
          </span>
          <span className="text-[10px] text-[#a3a3a3]">
            ({formatBytes(file.size_bytes)})
          </span>
        </div>

        <div className="flex items-center gap-2 flex-shrink-0 opacity-90 group-hover:opacity-100 transition">
          <select
            value={file.priority}
            onChange={(e) => setFilePriority(selectedTorrent!.id, file.index, Number(e.target.value))}
            className="px-2 py-0.5 rounded bg-[#141414] border border-[#333333] text-[10px] text-white"
          >
            <option value={0}>Do Not Download</option>
            <option value={1}>Normal Priority</option>
            <option value={2}>High Priority</option>
            <option value={3}>Maximum Priority</option>
          </select>
          {!isRemoteServer && isMediaFileName(file.name) && (
            <button
              type="button"
              onClick={() => {
                const fullPath = `${selectedTorrent!.save_path}/${file.name}`;
                invoke('launch_media_player', { filePath: fullPath }).catch(() => {});
              }}
              className="px-2 py-0.5 rounded bg-[#141414] hover:bg-[#262626] border border-[#333333] text-[#a3a3a3] hover:text-white flex items-center gap-1 text-[10px] transition"
              title={`Play "${node.name}" in media player`}
            >
              <Play className="w-3 h-3 text-[#e50914]" />
              <span>Play</span>
            </button>
          )}
          {!isRemoteServer && (
            <button
              type="button"
              onClick={() => openRenameFileModal(file.index, file.name)}
              className="px-2 py-0.5 rounded bg-[#141414] hover:bg-[#262626] border border-[#333333] text-[#a3a3a3] hover:text-white flex items-center gap-1 text-[10px] transition"
              title={`Rename File "${node.name}" on disk`}
            >
              <Edit2 className="w-3 h-3 text-[#e50914]" />
              <span>Rename File</span>
            </button>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="flex-1 flex overflow-hidden font-sans select-none bg-[#141414] text-white">
      {/* 1. Left Sidebar: Categories & Filters */}
      <aside className="w-48 bg-[#141414] border-r border-[#262626] flex flex-col p-2 space-y-4 text-xs">
        <div>
          <span className="text-[10px] font-bold uppercase tracking-wider text-[#a3a3a3] px-2 block mb-1">
            Transfers
          </span>
          <div className="space-y-0.5">
            {[
              { id: 'all', label: 'All Torrents', count: torrents.length },
              {
                id: 'downloading',
                label: 'Downloading',
                count: torrents.filter((t) => t.status === 'Downloading').length,
              },
              {
                id: 'seeding',
                label: 'Seeding',
                count: torrents.filter((t) => t.status === 'Seeding').length,
              },
              {
                id: 'completed',
                label: 'Completed',
                count: torrents.filter((t) => t.status === 'Completed').length,
              },
              {
                id: 'paused',
                label: 'Paused',
                count: torrents.filter((t) => t.status === 'Paused').length,
              },
            ].map((item) => (
              <button
                key={item.id}
                onClick={() => setManagerCategoryFilter(item.id)}
                className={`w-full flex items-center justify-between px-2.5 py-1.5 rounded text-left transition ${
                  managerCategoryFilter === item.id
                    ? 'bg-[#e50914] text-white font-semibold'
                    : 'text-[#a3a3a3] hover:text-white hover:bg-[#1f1f1f]'
                }`}
              >
                <span>{item.label}</span>
                <span className="text-[10px] font-mono opacity-80">{item.count}</span>
              </button>
            ))}
          </div>
        </div>

        <div>
          <span className="text-[10px] font-bold uppercase tracking-wider text-[#a3a3a3] px-2 block mb-1">
            Categories
          </span>
          <div className="space-y-0.5">
            {[
              {
                id: 'media',
                label: 'Video & Cinema',
                count: torrents.filter((t) => t.is_media).length,
              },
            ].map((item) => (
              <button
                key={item.id}
                onClick={() => setManagerCategoryFilter(item.id)}
                className={`w-full flex items-center justify-between px-2.5 py-1.5 rounded text-left transition ${
                  managerCategoryFilter === item.id
                    ? 'bg-[#e50914] text-white font-semibold'
                    : 'text-[#a3a3a3] hover:text-white hover:bg-[#1f1f1f]'
                }`}
              >
                <span>{item.label}</span>
                <span className="text-[10px] font-mono opacity-80">{item.count}</span>
              </button>
            ))}
          </div>
        </div>
      </aside>

      {/* 2. Main Right Area: Dense Transfer Grid + Inspector */}
      <div className="flex-1 flex flex-col overflow-hidden bg-[#141414]">
        {/* Data Table: Classic Grid */}
        <div className="flex-1 overflow-auto bg-[#141414]">
          <table className="w-full text-left border-collapse text-xs">
            <thead className="sticky top-0 bg-[#1f1f1f] border-b border-[#262626] text-[11px] font-semibold text-[#a3a3a3] uppercase font-mono tracking-wider z-10">
              <tr>
                <th className="py-2 px-3">Name</th>
                <th className="py-2 px-2.5 w-20">Size</th>
                <th className="py-2 px-2.5 w-36">Progress</th>
                <th className="py-2 px-2.5 w-24">Status</th>
                <th className="py-2 px-2.5 w-24">Down Speed</th>
                <th className="py-2 px-2.5 w-24">Up Speed</th>
                <th className="py-2 px-2.5 w-20">ETA</th>
                <th className="py-2 px-2.5 w-16">Ratio</th>
                <th className="py-2 px-2.5 w-14">Seeds</th>
                <th className="py-2 px-2.5 w-14">Peers</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[#262626] font-mono">
              {filteredTorrents.length === 0 && (
                <tr>
                  <td colSpan={10} className="py-24 text-center">
                    <div className="flex flex-col items-center justify-center space-y-2">
                      <HardDrive className="w-8 h-8 text-[#333333]" />
                      <p className="text-sm font-sans font-medium text-white">No torrents in list</p>
                      <p className="text-xs font-sans text-[#a3a3a3]">
                        Drop a <span className="text-white font-mono">.torrent</span> or{' '}
                        <span className="text-white font-mono">.adc</span> file, or click{' '}
                        <span className="text-[#e50914] font-semibold">+ Add</span> to begin
                      </p>
                    </div>
                  </td>
                </tr>
              )}
              {filteredTorrents.map((t) => {
                const isSelected = selectedId === t.id;
                const progressPct = Math.round(t.progress * 100);
                // While downloading the bar carries the downloaded volume;
                // once complete it switches to the uploaded volume (seeders
                // read contribution, not progress).
                const isComplete =
                  t.status === 'Seeding' || t.status === 'Completed' || t.progress >= 1;
                return (
                  <tr
                    key={t.id}
                    onClick={() => setSelectedId(t.id)}
                    onContextMenu={(e) => handleContextMenu(e, t.id)}
                    onDoubleClick={isRemoteServer ? undefined : openRenameTorrentModal}
                    className={`cursor-pointer transition select-none ${
                      isSelected
                        ? 'bg-[#1f1f1f] border-l-2 border-[#e50914] text-white'
                        : 'hover:bg-[#1a1a1a] text-[#a3a3a3] hover:text-white'
                    }`}
                  >
                    <td className="py-2 px-3 font-sans font-medium truncate max-w-xs text-white">
                      {t.name}
                    </td>
                    <td className="py-2 px-2.5">{formatBytes(t.total_bytes)}</td>
                    <td className="py-2 px-2.5">
                      <div className="flex flex-col gap-0.5 min-w-[150px]">
                        <div className="flex items-center gap-2">
                          <div className="flex-1 h-1.5 rounded-full bg-[#333333] overflow-hidden">
                            <div
                              className="h-full bg-[#e50914] rounded-full transition-all"
                              style={{ width: `${progressPct}%` }}
                            />
                          </div>
                          <span className="text-[10px] w-8 text-right font-mono text-white">
                            {progressPct}%
                          </span>
                        </div>
                        <span className="text-[9px] font-mono text-[#737373] text-right leading-tight">
                          {isComplete
                            ? `${formatBytes(t.uploaded_bytes)} uploaded`
                            : `${formatBytes(t.downloaded_bytes)} of ${formatBytes(t.total_bytes)}`}
                        </span>
                      </div>
                    </td>
                    <td className="py-2 px-2.5">
                      <span className="text-[11px] font-sans text-white">{t.status}</span>
                    </td>
                    <td className="py-2 px-2.5 text-white">{formatSpeed(t.download_rate)}</td>
                    <td className="py-2 px-2.5">{formatSpeed(t.upload_rate)}</td>
                    <td className="py-2 px-2.5">{formatEta(t.eta_seconds)}</td>
                    <td className="py-2 px-2.5 text-white">{formatRatio(t.ratio)}</td>
                    <td className="py-2 px-2.5 text-white">
                      {/* librqbit (local mode) cannot count connected seeds -
                          the peer API exposes no seed flag (Transmission shows
                          "?" for unknown; we show a dash). rtorrent/remote
                          report real values. */}
                      {isRemoteServer ? t.seeds : '—'}
                    </td>
                    <td className="py-2 px-2.5">{t.peers}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>

        {/* 3. Bottom Inspector: Tabs */}
        <div className="h-56 bg-[#1a1a1a] border-t border-[#262626] flex flex-col">
          {/* Tab Headers */}
          <div className="flex border-b border-[#262626] px-3 pt-1.5 gap-1 text-xs bg-[#1f1f1f]">
            {[
              { id: 'general', label: 'General', icon: Activity },
              { id: 'files', label: 'Files / Content', icon: FileText },
              { id: 'trackers', label: 'Trackers', icon: Layers },
              { id: 'peers', label: 'Peers', icon: Users },
              { id: 'speed', label: 'Speed Graph', icon: LineChart },
            ].map((tab) => {
              const Icon = tab.icon;
              const isActive = activeManagerTab === tab.id;
              return (
                <button
                  key={tab.id}
                  onClick={() => setActiveManagerTab(tab.id as ManagerTab)}
                  className={`flex items-center gap-1.5 px-3 py-1.5 rounded-t font-medium transition ${
                    isActive
                      ? 'border-b-2 border-[#e50914] text-white bg-[#141414]'
                      : 'text-[#a3a3a3] hover:text-white'
                  }`}
                >
                  <Icon className="w-3.5 h-3.5" />
                  <span>{tab.label}</span>
                </button>
              );
            })}
          </div>

          {/* Tab Panels */}
          <div className="flex-1 p-3 overflow-y-auto text-xs bg-[#141414]">
            {selectedTorrent ? (
              activeManagerTab === 'general' ? (
                <div className="space-y-3">
                  {superSeedingNotice && (
                    <div className="flex items-center justify-between p-2 rounded bg-amber-500/10 border border-amber-500/30 text-amber-200 text-xs">
                      <span>{superSeedingNotice}</span>
                      <button
                        type="button"
                        onClick={() => setSuperSeedingNotice(null)}
                        className="p-1 hover:text-white"
                      >
                        <X className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  )}

                  <div className="grid grid-cols-2 md:grid-cols-4 gap-3 font-mono text-xs">
                    <div className="space-y-1">
                      <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">Size</span>
                      <span className="text-white">{formatBytes(selectedTorrent.total_bytes)}</span>
                    </div>
                    <div className="space-y-1">
                      <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                        Downloaded
                      </span>
                      <span className="text-white">
                        {formatBytes(selectedTorrent.downloaded_bytes)}
                      </span>
                    </div>
                    <div className="space-y-1">
                      <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                        Uploaded
                      </span>
                      <span className="text-white">
                        {formatBytes(selectedTorrent.uploaded_bytes)}
                      </span>
                    </div>
                    <div className="space-y-1">
                      <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                        Share Ratio
                      </span>
                      <span className="text-white font-bold">{formatRatio(selectedTorrent.ratio)}</span>
                    </div>
                    <div className="space-y-1">
                      <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                        Save Path
                      </span>
                      <span className="text-white truncate block max-w-xs" title={selectedTorrent.save_path}>
                        {selectedTorrent.save_path}
                      </span>
                    </div>
                    <div className="space-y-1">
                      <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                        Info Hash
                      </span>
                      <span className="text-white truncate block max-w-xs font-mono text-[10px]">
                        {selectedTorrent.info_hash}
                      </span>
                    </div>
                    {!isRemoteServer && (
                      <>
                        <div className="space-y-1">
                          <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                            Speed Limits
                          </span>
                          <div className="flex items-center gap-2">
                            <span className="text-white text-[11px]">
                              {selectedTorrent.download_limit_kib > 0 ? `DL: ${selectedTorrent.download_limit_kib}K` : 'DL: Max'} / {selectedTorrent.upload_limit_kib > 0 ? `UL: ${selectedTorrent.upload_limit_kib}K` : 'UL: Max'}
                            </span>
                            <button
                              type="button"
                              onClick={() =>
                                setLimitsModal({
                                  isOpen: true,
                                  id: selectedTorrent.id,
                                  dlKib: selectedTorrent.download_limit_kib,
                                  ulKib: selectedTorrent.upload_limit_kib,
                                  ratioLimit: selectedTorrent.ratio_limit,
                                })
                              }
                              className="px-1.5 py-0.5 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-[10px] text-white flex items-center gap-1 transition"
                            >
                              <Sliders className="w-3 h-3 text-[#e50914]" />
                              <span>Edit</span>
                            </button>
                          </div>
                        </div>
                        <div className="space-y-1">
                          <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                            Swarm Controls
                          </span>
                          <div className="flex flex-wrap items-center gap-1.5">
                            <button
                              type="button"
                              onClick={() => toggleSequential(selectedTorrent.id)}
                              className={`px-2 py-0.5 rounded text-[10px] font-semibold border transition ${
                                selectedTorrent.sequential_download
                                  ? 'bg-emerald-500/20 text-emerald-400 border-emerald-500/40'
                                  : 'bg-[#1f1f1f] text-[#a3a3a3] border-[#333333] hover:text-white'
                              }`}
                              title="Sequential Downloading (Stream in order)"
                            >
                              Sequential: {selectedTorrent.sequential_download ? 'ON' : 'OFF'}
                            </button>

                            <button
                              type="button"
                              onClick={() => toggleFirstLast(selectedTorrent.id)}
                              className={`px-2 py-0.5 rounded text-[10px] font-semibold border transition ${
                                selectedTorrent.first_last_piece_prio
                                  ? 'bg-emerald-500/20 text-emerald-400 border-emerald-500/40'
                                  : 'bg-[#1f1f1f] text-[#a3a3a3] border-[#333333] hover:text-white'
                              }`}
                              title="Prioritize first and last piece for container headers"
                            >
                              1st/Last: {selectedTorrent.first_last_piece_prio ? 'ON' : 'OFF'}
                            </button>

                            <button
                              type="button"
                              onClick={() => {
                                toggleSuperSeeding(selectedTorrent.id);
                                setSuperSeedingNotice(
                                  'Observational: Super-Seeding (BEP 16) is not supported by upstream librqbit 9.0.1. Tracked in docs/upstream-issues/rqbit-super-seeding.md.'
                                );
                              }}
                              className={`px-2 py-0.5 rounded text-[10px] font-semibold border transition ${
                                selectedTorrent.super_seeding
                                  ? 'bg-pink-500/20 text-pink-400 border-pink-500/40'
                                  : 'bg-[#1f1f1f] text-[#a3a3a3] border-[#333333] hover:text-white'
                              }`}
                              title="Super-Seeding Mode (Initial Seeding / BEP 16)"
                            >
                              Super-Seed: {selectedTorrent.super_seeding ? 'ON' : 'OFF'}
                            </button>
                          </div>
                        </div>
                      </>
                    )}
                  </div>
                </div>
              ) : activeManagerTab === 'files' ? (
                /* Files & Folder Content Tab with Tree/Flat View & Folder/File Renaming */
                <div className="flex flex-col h-full space-y-2">
                  {/* Controls Bar */}
                  <div className="flex flex-wrap items-center justify-between gap-2 p-2 rounded bg-[#1f1f1f] border border-[#262626]">
                    <div className="flex items-center gap-2">
                      <div className="flex items-center bg-[#141414] rounded-lg p-0.5 border border-[#333333]">
                        <button
                          type="button"
                          onClick={() => setFilesViewMode('tree')}
                          className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-[11px] font-medium transition ${
                            filesViewMode === 'tree'
                              ? 'bg-[#e50914] text-white'
                              : 'text-[#a3a3a3] hover:text-white'
                          }`}
                        >
                          <FolderTree className="w-3.5 h-3.5" />
                          <span>Tree View</span>
                        </button>
                        <button
                          type="button"
                          onClick={() => setFilesViewMode('flat')}
                          className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-[11px] font-medium transition ${
                            filesViewMode === 'flat'
                              ? 'bg-[#e50914] text-white'
                              : 'text-[#a3a3a3] hover:text-white'
                          }`}
                        >
                          <List className="w-3.5 h-3.5" />
                          <span>Flat View</span>
                        </button>
                      </div>

                      {filesViewMode === 'tree' && fileTree && (
                        <div className="flex items-center gap-1">
                          <button
                            type="button"
                            onClick={() => setCollapsedFolders({})}
                            className="px-2 py-1 rounded bg-[#141414] hover:bg-[#262626] border border-[#333333] text-[10px] text-[#a3a3a3] hover:text-white transition"
                          >
                            Expand All
                          </button>
                          <button
                            type="button"
                            onClick={() => {
                              const map: Record<string, boolean> = {};
                              const collect = (node: FileTreeNode) => {
                                if (node.isFolder && node.path) map[node.path] = true;
                                node.children.forEach(collect);
                              };
                              if (fileTree) collect(fileTree);
                              setCollapsedFolders(map);
                            }}
                            className="px-2 py-1 rounded bg-[#141414] hover:bg-[#262626] border border-[#333333] text-[10px] text-[#a3a3a3] hover:text-white transition"
                          >
                            Collapse All
                          </button>
                        </div>
                      )}
                    </div>

                    <div className="flex items-center gap-2">
                      <div className="relative">
                        <Search className="w-3.5 h-3.5 absolute left-2 top-2 text-[#a3a3a3]" />
                        <input
                          type="text"
                          value={fileFilter}
                          onChange={(e) => setFileFilter(e.target.value)}
                          placeholder="Filter files or folders..."
                          className="pl-7 pr-6 py-1 rounded bg-[#141414] border border-[#333333] text-white placeholder-[#666666] text-xs font-sans focus:outline-none focus:border-[#e50914] w-48"
                        />
                        {fileFilter && (
                          <button
                            type="button"
                            onClick={() => setFileFilter('')}
                            className="absolute right-2 top-2 text-[#a3a3a3] hover:text-white"
                          >
                            <X className="w-3 h-3" />
                          </button>
                        )}
                      </div>

                      <div className="text-[11px] text-[#a3a3a3] font-mono whitespace-nowrap">
                        {selectedTorrent.files?.length || 0} files
                        {totalFoldersCount > 0 ? ` · ${totalFoldersCount} folders` : ''} ·{' '}
                        {formatBytes(selectedTorrent.total_bytes)}
                      </div>
                    </div>
                  </div>

                  {/* Content List / Tree */}
                  <div className="flex-1 overflow-y-auto space-y-1 font-mono text-xs pr-1">
                    {filesViewMode === 'tree' && fileTree ? (
                      fileTree.children.length > 0 ? (
                        fileTree.children.map((child) => renderTreeNode(child, 0))
                      ) : (
                        <div className="p-3 text-center text-[#a3a3a3]">
                          No files found in torrent.
                        </div>
                      )
                    ) : (
                      filteredFlatFiles.length > 0 ? (
                        filteredFlatFiles.map((file) => (
                          <div
                            key={file.index}
                            className="p-2 rounded bg-[#1f1f1f] border border-[#262626] flex justify-between items-center hover:border-[#333333]"
                          >
                            <div className="min-w-0 flex items-center gap-2">
                              {getFileIcon(file.name)}
                              <span className="text-white truncate max-w-md font-sans">
                                {file.name}
                              </span>
                              <span className="text-[10px] text-[#a3a3a3]">
                                ({formatBytes(file.size_bytes)})
                              </span>
                            </div>
                            <div className="flex items-center gap-3 flex-shrink-0">
                              <select
                                value={file.priority}
                                onChange={(e) =>
                                  setFilePriority(selectedTorrent.id, file.index, Number(e.target.value))
                                }
                                className="px-2 py-0.5 rounded bg-[#141414] border border-[#333333] text-[10px] text-white"
                              >
                                <option value={0}>Do Not Download</option>
                                <option value={1}>Normal Priority</option>
                                <option value={2}>High Priority</option>
                                <option value={3}>Maximum Priority</option>
                              </select>
                              {!isRemoteServer && isMediaFileName(file.name) && (
                                <button
                                  type="button"
                                  onClick={() => {
                                    const fullPath = `${selectedTorrent.save_path}/${file.name}`;
                                    invoke('launch_media_player', { filePath: fullPath }).catch(() => {});
                                  }}
                                  className="px-2 py-0.5 rounded bg-[#141414] hover:bg-[#262626] border border-[#333333] text-[#a3a3a3] hover:text-white flex items-center gap-1 text-[10px] transition"
                                  title="Play in media player"
                                >
                                  <Play className="w-3 h-3 text-[#e50914]" />
                                  <span>Play</span>
                                </button>
                              )}
                              {!isRemoteServer && (
                                <button
                                  type="button"
                                  onClick={() => openRenameFileModal(file.index, file.name)}
                                  className="px-2 py-0.5 rounded bg-[#141414] hover:bg-[#262626] border border-[#333333] text-[#a3a3a3] hover:text-white flex items-center gap-1 text-[10px] transition"
                                  title="Rename File on Disk"
                                >
                                  <Edit2 className="w-3 h-3 text-[#e50914]" />
                                  <span>Rename</span>
                                </button>
                              )}
                            </div>
                          </div>
                        ))
                      ) : (
                        <div className="p-3 text-center text-[#a3a3a3]">
                          {fileFilter ? 'No matching files found.' : 'No files found.'}
                        </div>
                      )
                    )}
                  </div>
                </div>
              ) : activeManagerTab === 'trackers' ? (
                <div className="font-mono text-xs space-y-1.5">
                  {selectedTorrent.trackers && selectedTorrent.trackers.length > 0 ? (
                    selectedTorrent.trackers.map((tr, idx) => (
                      <div
                        key={idx}
                        className="p-2 rounded bg-[#1f1f1f] border border-[#262626] flex items-center justify-between text-[11px]"
                      >
                        <div className="min-w-0 pr-3">
                          <span className="text-[#a3a3a3] truncate block max-w-lg">{tr.url}</span>
                          <span className="text-[10px] text-slate-500">Tier {tr.tier} • Seeds: {tr.seeds} • Peers: {tr.peers}</span>
                        </div>
                        <div className="flex items-center gap-2 flex-shrink-0">
                          <span className="text-white font-semibold px-1.5 py-0.5 rounded bg-[#141414] border border-[#262626]">
                            {tr.status}
                          </span>
                          <button
                            type="button"
                            onClick={() => reannounce(selectedTorrent.id)}
                            className="px-2 py-0.5 rounded bg-[#141414] hover:bg-[#262626] border border-[#333333] text-[10px] text-white flex items-center gap-1 transition"
                            title="Trigger immediate re-announce to this tracker"
                          >
                            <RotateCw className="w-3 h-3 text-[#e50914]" />
                            <span>Reannounce</span>
                          </button>
                        </div>
                      </div>
                    ))
                  ) : (
                    <div className="p-4 rounded bg-[#1f1f1f] border border-[#262626] text-center text-[#a3a3a3] text-xs font-sans">
                      No external announce trackers registered for this transfer (DHT / PEX only swarm).
                    </div>
                  )}
                </div>
              ) : activeManagerTab === 'peers' ? (
                /* Peers Tab */
                <div className="overflow-x-auto border border-[#262626] rounded">
                  <table className="w-full text-left border-collapse text-xs">
                    <thead className="bg-[#1f1f1f] border-b border-[#262626] text-[10px] uppercase font-mono text-[#a3a3a3]">
                      <tr>
                        <th className="py-1 px-2">Client</th>
                        <th className="py-1 px-2">IP & Port</th>
                        <th className="py-1 px-2">Down Speed</th>
                        <th className="py-1 px-2">Up Speed</th>
                        <th className="py-1 px-2">Score</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-[#262626] font-mono text-[11px]">
                      {peers.length === 0 ? (
                        <tr>
                          <td colSpan={5} className="py-4 text-center text-[#a3a3a3]">
                            No active peers connected yet
                          </td>
                        </tr>
                      ) : (
                        peers.map((p) => (
                          <tr key={p.peer_id} className="hover:bg-[#1f1f1f]">
                            <td className="py-1 px-2 text-white font-sans">{p.client_name}</td>
                            <td className="py-1 px-2 text-[#a3a3a3]">
                              {p.ip}:{p.port}
                            </td>
                            <td className="py-1 px-2 text-white">
                              {formatSpeed(p.sustained_dl_bps)}
                            </td>
                            <td className="py-1 px-2 text-[#a3a3a3]">
                              {formatSpeed(p.sustained_ul_bps)}
                            </td>
                            <td className="py-1 px-2 text-white font-semibold">
                              {(p.stability_score * 100).toFixed(0)}%
                            </td>
                          </tr>
                        ))
                      )}
                    </tbody>
                  </table>
                </div>
              ) : (
                /* Speed Graph Tab */
                <div className="flex flex-col h-full space-y-2">
                  <div className="flex items-center justify-between text-[11px] font-mono px-1 text-slate-400">
                    <div className="flex items-center gap-4">
                      <div className="flex items-center gap-1.5 text-adc-cyan">
                        <span className="w-2.5 h-2.5 rounded-full bg-adc-cyan" />
                        <span>Down: {formatSpeed(selectedTorrent.download_rate)}</span>
                      </div>
                      <div className="flex items-center gap-1.5 text-adc-emerald">
                        <span className="w-2.5 h-2.5 rounded-full bg-adc-emerald" />
                        <span>Up: {formatSpeed(selectedTorrent.upload_rate)}</span>
                      </div>
                    </div>
                    <span className="text-[10px]">Real-time Telemetry (500ms Push)</span>
                  </div>

                  <div className="flex-1 w-full rounded bg-[#0B0F19] border border-[#262626] p-2 relative overflow-hidden flex items-end">
                    {speedHistory.length < 2 ? (
                      <div className="w-full h-full flex items-center justify-center text-slate-500 font-sans text-xs">
                        Gathering throughput telemetry...
                      </div>
                    ) : (
                      <svg className="w-full h-full" viewBox="0 0 500 120" preserveAspectRatio="none">
                        <defs>
                          <linearGradient id="dlGrad" x1="0" y1="0" x2="0" y2="1">
                            <stop offset="0%" stopColor="#06B6D4" stopOpacity="0.4" />
                            <stop offset="100%" stopColor="#06B6D4" stopOpacity="0.0" />
                          </linearGradient>
                          <linearGradient id="ulGrad" x1="0" y1="0" x2="0" y2="1">
                            <stop offset="0%" stopColor="#10B981" stopOpacity="0.4" />
                            <stop offset="100%" stopColor="#10B981" stopOpacity="0.0" />
                          </linearGradient>
                        </defs>

                        {(() => {
                          const maxSpeed = Math.max(
                            ...speedHistory.map((s) => Math.max(s.dl, s.ul)),
                            1024 * 50
                          );
                          const w = 500;
                          const h = 120;
                          const step = w / Math.max(speedHistory.length - 1, 1);

                          const dlPoints = speedHistory.map((s, i) => [
                            i * step,
                            h - (s.dl / maxSpeed) * (h - 15),
                          ]);
                          const ulPoints = speedHistory.map((s, i) => [
                            i * step,
                            h - (s.ul / maxSpeed) * (h - 15),
                          ]);

                          const dlD = `M ${dlPoints.map((p) => `${p[0]},${p[1]}`).join(' L ')}`;
                          const dlArea = `${dlD} L ${w},${h} L 0,${h} Z`;

                          const ulD = `M ${ulPoints.map((p) => `${p[0]},${p[1]}`).join(' L ')}`;
                          const ulArea = `${ulD} L ${w},${h} L 0,${h} Z`;

                          return (
                            <>
                              <path d={dlArea} fill="url(#dlGrad)" />
                              <path d={dlD} fill="none" stroke="#06B6D4" strokeWidth="2" />
                              <path d={ulArea} fill="url(#ulGrad)" />
                              <path d={ulD} fill="none" stroke="#10B981" strokeWidth="2" />
                            </>
                          );
                        })()}
                      </svg>
                    )}
                  </div>
                </div>
              )
            ) : (
              <div className="text-[#a3a3a3] text-xs">No torrent selected</div>
            )}
          </div>
        </div>
      </div>

      {/* 4. Desktop Right-Click Context Menu */}
      {contextMenu.visible && (
        <div
          style={{ top: contextMenu.y, left: contextMenu.x }}
          className="fixed z-50 w-52 bg-[#1f1f1f] border border-[#333333] rounded shadow-2xl py-1 text-xs select-none"
          onClick={(e) => e.stopPropagation()}
        >
          <button
            onClick={() => {
              resumeTorrent(contextMenu.torrentId);
              setContextMenu((prev) => ({ ...prev, visible: false }));
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-[#e50914] text-white text-left transition"
          >
            <Play className="w-3.5 h-3.5 fill-current" />
            <span>Resume</span>
          </button>
          <button
            onClick={() => {
              pauseTorrent(contextMenu.torrentId);
              setContextMenu((prev) => ({ ...prev, visible: false }));
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-[#e50914] text-white text-left transition"
          >
            <Pause className="w-3.5 h-3.5 fill-current" />
            <span>Pause</span>
          </button>
          <div className="h-[1px] bg-[#333333] my-1" />
          {!isRemoteServer && (
            <button
              onClick={() => {
                openRenameTorrentModal();
                setContextMenu((prev) => ({ ...prev, visible: false }));
              }}
              className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-[#e50914] text-white text-left transition"
            >
              <Edit2 className="w-3.5 h-3.5" />
              <span>Rename...</span>
            </button>
          )}
          <button
            onClick={() => {
              forceRecheck(contextMenu.torrentId);
              setContextMenu((prev) => ({ ...prev, visible: false }));
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-[#e50914] text-white text-left transition"
          >
            <RotateCw className="w-3.5 h-3.5" />
            <span>Force Recheck</span>
          </button>
          <button
            onClick={() => {
              reannounce(contextMenu.torrentId);
              setContextMenu((prev) => ({ ...prev, visible: false }));
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-[#e50914] text-white text-left transition"
          >
            <Radio className="w-3.5 h-3.5" />
            <span>Reannounce</span>
          </button>
          <button
            onClick={() => {
              handleCopyHash();
              setContextMenu((prev) => ({ ...prev, visible: false }));
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-[#e50914] text-white text-left transition"
          >
            <Copy className="w-3.5 h-3.5" />
            <span>Copy Info Hash</span>
          </button>
          <div className="h-[1px] bg-[#333333] my-1" />
          <button
            onClick={() => {
              const t = torrents.find((item) => item.id === contextMenu.torrentId);
              if (t) {
                setLimitsModal({
                  isOpen: true,
                  id: t.id,
                  dlKib: t.download_limit_kib,
                  ulKib: t.upload_limit_kib,
                  ratioLimit: t.ratio_limit,
                });
              }
              setContextMenu((prev) => ({ ...prev, visible: false }));
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-[#e50914] text-white text-left transition"
          >
            <Sliders className="w-3.5 h-3.5" />
            <span>Set Speed Limits...</span>
          </button>
          <button
            onClick={() => {
              toggleSequential(contextMenu.torrentId);
              setContextMenu((prev) => ({ ...prev, visible: false }));
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-[#e50914] text-white text-left transition"
          >
            <Zap className="w-3.5 h-3.5" />
            <span>Toggle Sequential Download</span>
          </button>
          <div className="h-[1px] bg-[#333333] my-1" />
          <button
            onClick={() => {
              const t = torrents.find((item) => item.id === contextMenu.torrentId);
              if (t) {
                setDeleteModalTarget({ id: t.id, name: t.name });
              }
              setContextMenu((prev) => ({ ...prev, visible: false }));
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-[#e50914] text-white text-left transition"
          >
            <Trash2 className="w-3.5 h-3.5" />
            <span>Delete Torrent...</span>
          </button>
        </div>
      )}

      {/* 5. Rename Modal with Strict External qBittorrent Protection */}
      {renameModal.isOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm select-none font-sans">
          <div className="relative w-full max-w-md bg-[#141414] border border-[#333333] rounded-xl shadow-2xl overflow-hidden flex flex-col text-white">
            <div className="flex items-center justify-between px-5 py-3.5 border-b border-[#333333] bg-[#1f1f1f]">
              <div className="flex items-center gap-2">
                <Edit2 className="w-4 h-4 text-[#e50914]" />
                <h2 className="text-sm font-bold text-white">
                  {renameModal.type === 'torrent'
                    ? 'Rename Torrent / Root Directory'
                    : renameModal.type === 'folder'
                    ? 'Rename Folder'
                    : 'Rename File'}
                </h2>
              </div>
              <button
                onClick={() => setRenameModal((prev) => ({ ...prev, isOpen: false }))}
                className="p-1 rounded text-[#a3a3a3] hover:text-white transition"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            <form onSubmit={handleConfirmRename} className="p-5 space-y-4 text-xs">
              <div>
                <span className="text-[10px] uppercase text-[#a3a3a3] block font-mono mb-1">
                  Current {renameModal.type === 'torrent' ? 'Torrent' : renameModal.type === 'folder' ? 'Folder' : 'File'} Name:
                </span>
                <span className="text-xs text-white font-mono bg-[#1f1f1f] p-2 rounded block border border-[#262626] truncate">
                  {renameModal.currentName}
                </span>
              </div>

              <div>
                <label className="block text-xs font-medium text-white mb-1.5">
                  New {renameModal.type === 'torrent' ? 'Torrent' : renameModal.type === 'folder' ? 'Folder' : 'File'} Name:
                </label>
                <input
                  type="text"
                  autoFocus
                  value={renameModal.newName}
                  onChange={(e) => setRenameModal((prev) => ({ ...prev, newName: e.target.value }))}
                  className="w-full px-3 py-2 rounded-lg bg-[#1f1f1f] border border-[#333333] text-white font-mono text-xs focus:outline-none focus:border-[#e50914]"
                />
              </div>

              {renameModal.folderPath && renameModal.type === 'folder' && (
                <div>
                  <span className="text-[10px] uppercase text-[#a3a3a3] block font-mono">
                    Relative Folder Path:
                  </span>
                  <span className="text-xs text-[#a3a3a3] font-mono truncate block max-w-sm">
                    {renameModal.folderPath}
                  </span>
                </div>
              )}

              {renameModal.savePath && (
                <div>
                  <span className="text-[10px] uppercase text-[#a3a3a3] block font-mono">
                    Storage Location:
                  </span>
                  <span className="text-xs text-[#a3a3a3] font-mono truncate block max-w-sm">
                    {renameModal.savePath}
                  </span>
                </div>
              )}

              {/* Strict Isolation Notice */}
              <div className="p-2.5 rounded bg-[#1f1f1f] border border-[#262626] flex items-start gap-2 text-[11px] text-[#a3a3a3]">
                <ShieldCheck className="w-4 h-4 text-[#e50914] flex-shrink-0 mt-0.5" />
                <span>
                  <strong className="text-white">Safety Protection:</strong> Renames are applied strictly to Nebulae workspace files. Torrents or paths managed by external qBittorrent will never be altered or amended.
                </span>
              </div>

              {renameError && (
                <div className="p-2 rounded bg-[#1f1f1f] border border-[#e50914] text-white text-xs flex items-center gap-1.5 font-mono">
                  <AlertCircle className="w-4 h-4 text-[#e50914] flex-shrink-0" />
                  <span>{renameError}</span>
                </div>
              )}

              <div className="flex justify-end gap-2 pt-2 border-t border-[#333333]">
                <button
                  type="button"
                  onClick={() => setRenameModal((prev) => ({ ...prev, isOpen: false }))}
                  className="px-4 py-1.5 rounded-lg text-[#a3a3a3] hover:text-white transition"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="px-5 py-1.5 rounded-lg bg-[#e50914] hover:bg-[#f40612] text-white font-semibold transition shadow"
                >
                  Apply Rename
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* 6. Speed & Ratio Limits Modal */}
      {limitsModal.isOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm select-none font-sans">
          <div className="relative w-full max-w-md bg-[#141414] border border-[#333333] rounded-xl shadow-2xl overflow-hidden flex flex-col text-white">
            <div className="flex items-center justify-between px-5 py-3.5 border-b border-[#333333] bg-[#1f1f1f]">
              <div className="flex items-center gap-2">
                <Sliders className="w-4 h-4 text-[#e50914]" />
                <h2 className="text-sm font-bold text-white">Torrent Speed & Seeding Limits</h2>
              </div>
              <button
                type="button"
                onClick={() => setLimitsModal((prev) => ({ ...prev, isOpen: false }))}
                className="p-1 rounded text-[#a3a3a3] hover:text-white transition"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            <form
              onSubmit={(e) => {
                e.preventDefault();
                setTorrentLimits(
                  limitsModal.id,
                  Number(limitsModal.dlKib) || 0,
                  Number(limitsModal.ulKib) || 0,
                  Number(limitsModal.ratioLimit) ?? -1
                );
                setLimitsModal((prev) => ({ ...prev, isOpen: false }));
              }}
              className="p-5 space-y-4"
            >
              <div>
                <label className="text-xs text-[#a3a3a3] block font-mono mb-1">
                  Download Limit (KiB/s, 0 = unlimited):
                </label>
                <input
                  type="number"
                  min="0"
                  value={limitsModal.dlKib}
                  onChange={(e) =>
                    setLimitsModal((prev) => ({ ...prev, dlKib: Math.max(0, parseInt(e.target.value) || 0) }))
                  }
                  className="w-full px-3 py-2 bg-[#0B0F19] border border-[#333333] rounded-lg text-white font-mono text-sm focus:outline-none focus:border-[#06B6D4]"
                />
              </div>

              <div>
                <label className="text-xs text-[#a3a3a3] block font-mono mb-1">
                  Upload Limit (KiB/s, 0 = unlimited):
                </label>
                <input
                  type="number"
                  min="0"
                  value={limitsModal.ulKib}
                  onChange={(e) =>
                    setLimitsModal((prev) => ({ ...prev, ulKib: Math.max(0, parseInt(e.target.value) || 0) }))
                  }
                  className="w-full px-3 py-2 bg-[#0B0F19] border border-[#333333] rounded-lg text-white font-mono text-sm focus:outline-none focus:border-[#10B981]"
                />
              </div>

              <div>
                <label className="text-xs text-[#a3a3a3] block font-mono mb-1">
                  Stop Seeding at Ratio (-1 = no limit):
                </label>
                <input
                  type="number"
                  step="0.1"
                  min="-1"
                  value={limitsModal.ratioLimit}
                  onChange={(e) =>
                    setLimitsModal((prev) => ({ ...prev, ratioLimit: parseFloat(e.target.value) || -1 }))
                  }
                  className="w-full px-3 py-2 bg-[#0B0F19] border border-[#333333] rounded-lg text-white font-mono text-sm focus:outline-none focus:border-[#F59E0B]"
                />
              </div>

              <div className="flex justify-end gap-2 pt-2 border-t border-[#333333]">
                <button
                  type="button"
                  onClick={() => setLimitsModal((prev) => ({ ...prev, isOpen: false }))}
                  className="px-4 py-1.5 rounded-lg text-[#a3a3a3] hover:text-white transition"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="px-5 py-1.5 rounded-lg bg-[#e50914] hover:bg-[#f40612] text-white font-semibold transition shadow"
                >
                  Save Limits
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
};

export default TorrentManagerView;
