import React, { useEffect, useState, useMemo, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { openInPlayer } from '../../lib/playerStore';
import { useTorrentStore } from '../../stores/torrentStore';
import { useUiStore, ManagerTab } from '../../stores/uiStore';
import { usePeeringStore } from '../../stores/peeringStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { formatBytes, formatSpeed, formatEta, formatRatio } from '../../lib/formatters';
import { TorrentFileItem, TorrentItem } from '../../types';
import PosterHoverPopup, { PosterPopupTarget } from './PosterHoverPopup';
import { PeersTabPanel } from './PeersTabPanel';
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
  Check,
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
  FolderInput,
  Save,
} from 'lucide-react';

// Human delta for announce timestamps (unix seconds): "4m 12s", "2h 5m", "3d 4h".
export const formatAnnounceDelta = (ts: number): string => {
  const d = Math.max(0, Math.floor(Date.now() / 1000) - ts);
  if (d < 60) return `${d}s`;
  if (d < 3600) return `${Math.floor(d / 60)}m ${d % 60}s`;
  if (d < 86400) return `${Math.floor(d / 3600)}h ${Math.floor((d % 3600) / 60)}m`;
  return `${Math.floor(d / 86400)}d ${Math.floor((d % 86400) / 3600)}h`;
};

// Next announce is a FUTURE timestamp; a plain now-minus-ts delta clamps to
// "0s". Compute the countdown instead.
export const formatAnnounceUntil = (ts: number): string => {
  const d = Math.max(0, Math.floor(ts - Date.now() / 1000));
  if (d < 60) return `${d}s`;
  if (d < 3600) return `${Math.floor(d / 60)}m ${d % 60}s`;
  if (d < 86400) return `${Math.floor(d / 3600)}h ${Math.floor((d % 3600) / 60)}m`;
  return `${Math.floor(d / 86400)}d ${Math.floor((d % 86400) / 3600)}h`;
};

// A torrent's data usually lands inside a top-level folder named by the
// torrent itself; Finder should open THAT folder, not the download root.
export const resolveTorrentDataDir = (t: {
  save_path: string;
  files?: { name: string }[];
}): string => {
  const roots = (t.files ?? []).map((f) => f.name.split('/')[0]).filter(Boolean);
  if (roots.length > 0 && roots.every((r) => r === roots[0])) {
    return `${t.save_path}/${roots[0]}`;
  }
  return t.save_path;
};

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

function nodeMatchesFilter(node: FileTreeNode, query: string): boolean {
  if (!query) return true;
  const q = query.toLowerCase();
  if (node.name.toLowerCase().includes(q)) return true;
  if (node.isFolder) {
    return node.children.some((c) => nodeMatchesFilter(c, query));
  }
  return false;
}

// qBittorrent-style tracker grouping: collapse each announce URL to its
// hostname so the sidebar lists trackers the way qBittorrent does
// ("tracker.example.org" with a torrent count and health dot).
const trackerHost = (url: string): string => {
  try {
    return new URL(url).hostname || url;
  } catch {
    return url;
  }
};

/** True when every one of the torrent's trackers reports failure — the
 * swarm counts (seeds/peers) on display are then a stale snapshot from the
 * last successful announce, not tracker reality. */
const trackersAllFailingCheck = (t: TorrentItem): boolean =>
  (t.trackers?.length ?? 0) > 0 &&
  t.trackers.every((tr) =>
    ['fail', 'error', 'not working', 'unreachable'].some((k) =>
      tr.status.toLowerCase().includes(k),
    ),
  );

// qBittorrent-style per-row download progress: a slim monochrome bar
// (gray fill on a near-black track) with GB and percent next to it.
// Folders aggregate their subtree by byte weight.
const nodeProgress = (node: FileTreeNode): number => {
  if (!node.isFolder) return node.file?.progress ?? 0;
  let done = 0;
  let total = 0;
  for (const child of node.children) {
    if (child.isFolder) {
      done += nodeProgress(child) * child.size_bytes;
      total += child.size_bytes;
    } else if (child.file) {
      done += child.file.progress * child.file.size_bytes;
      total += child.file.size_bytes;
    }
  }
  return total > 0 ? done / total : 0;
};

const ProgressBars = { track: '#111111', fill: '#9ca3af', fillDone: '#d4d4d4' };

const FileProgressBar: React.FC<{ progress: number; sizeBytes: number }> = ({
  progress,
  sizeBytes,
}) => {
  const pct = Math.round(progress * 100);
  const done = pct >= 100;
  return (
    <div className="flex items-center gap-2 flex-shrink-0" style={{ width: 180 }}>
      <div
        className="flex-1 h-2 rounded-sm overflow-hidden"
        style={{ backgroundColor: ProgressBars.track }}
      >
        <div
          className="h-full rounded-sm transition-[width] duration-500"
          style={{
            width: `${pct}%`,
            backgroundColor: done ? ProgressBars.fillDone : ProgressBars.fill,
          }}
        />
      </div>
      <span className={`text-[10px] font-mono whitespace-nowrap ${done ? 'text-white' : 'text-[#a3a3a3]'}`}>
        {formatBytes(sizeBytes)} · {pct}%
      </span>
    </div>
  );
};

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
    setTorrentLimits,
    setLocation,
    selectDownloadDirectory,
  } = useTorrentStore();

  const {
    activeManagerTab,
    setActiveManagerTab,
    managerCategoryFilter,
    setManagerCategoryFilter,
    managerTrackerFilter,
    setManagerTrackerFilter,
    searchQuery,
    setDeleteModalTarget,
  } = useUiStore();

  const { peers, fetchPeers, settings: peeringSettings, fetchSettings, saveSettings } = usePeeringStore();
  const { engineMode, settings } = useSettingsStore();
  // In rtorrent-server mode the GUI hides options the rtorrent engine cannot
  // fulfill instead of offering actions that only produce errors.
  const isRemoteServer = engineMode === 'rtorrent_server';
  const isTransmission = !isRemoteServer && (settings.engine || 'transmission') === 'transmission';

  // Bottom inspector height, user-resizable by dragging the divider.
  const [inspectorHeight, setInspectorHeight] = useState<number>(() => {
    const stored = Number(localStorage.getItem('nebulae.inspectorHeight'));
    return stored >= 120 && stored <= 640 ? stored : 224;
  });
  const startInspectorDrag = (e: React.MouseEvent) => {
    e.preventDefault();
    const startY = e.clientY;
    const startH = inspectorHeight;
    const onMove = (ev: MouseEvent) => {
      // Dragging up grows the inspector.
      const next = Math.min(640, Math.max(120, startH - (ev.clientY - startY)));
      setInspectorHeight(next);
    };
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      setInspectorHeight((h) => {
        localStorage.setItem('nebulae.inspectorHeight', String(h));
        return h;
      });
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  };
  const [contextMenu, setContextMenu] = useState<ContextMenuState>({
    visible: false,
    x: 0,
    y: 0,
    torrentId: '',
  });

  const [filesViewMode, setFilesViewMode] = useState<'tree' | 'flat'>('tree');

  // Poster hover popup state (ADC torrents.php behavior): ~250 ms settle
  // delay on the name cell, dismiss on leave. One shared card.
  const [posterPopup, setPosterPopup] = useState<{
    target: PosterPopupTarget;
    x: number;
    y: number;
  } | null>(null);
  const posterTimerRef = useRef<number | null>(null);
  const openPosterPopup = (t: TorrentItem, e: React.MouseEvent) => {
    const { clientX, clientY } = e;
    if (posterTimerRef.current) window.clearTimeout(posterTimerRef.current);
    posterTimerRef.current = window.setTimeout(() => {
      setPosterPopup({
        target: {
          id: t.id,
          name: t.name,
          info_hash: t.info_hash,
          save_path: t.save_path,
        },
        x: clientX,
        y: clientY,
      });
    }, 250);
  };
  const closePosterPopup = () => {
    if (posterTimerRef.current) {
      window.clearTimeout(posterTimerRef.current);
      posterTimerRef.current = null;
    }
    setPosterPopup(null);
  };
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
    if (managerTrackerFilter) {
      const hosts = (t.trackers ?? []).map((tr) => trackerHost(tr.url));
      return hosts.includes(managerTrackerFilter);
    }
    return true;
  });

  // qBittorrent-style tracker list for the left sidebar: unique tracker
  // hosts across all torrents, with torrent counts and aggregate health.
  const trackerEntries = (() => {
    const map = new Map<string, { count: number; healthy: number }>();
    for (const t of torrents) {
      const hosts = new Set((t.trackers ?? []).map((tr) => trackerHost(tr.url)).filter(Boolean));
      for (const host of hosts) {
        const entry = map.get(host) ?? { count: 0, healthy: 0 };
        entry.count += 1;
        const allWorking = (t.trackers ?? []).every((tr) => tr.status === 'Working');
        if (allWorking) entry.healthy += 1;
        map.set(host, entry);
      }
    }
    return Array.from(map.entries())
      .map(([host, { count, healthy }]) => ({ host, count, healthy }))
      .sort((a, b) => b.count - a.count);
  })();

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

  // Announce/scrape status for the General tab: prefer a tracker that is not
  // working (its failure message is what the user needs to see), otherwise
  // the first tracker's live message from the engine's scrape loop.
  const announceStatus = (() => {
    const trackers = selectedTorrent?.trackers ?? [];
    if (trackers.length === 0) return 'No trackers';
    const failing = trackers.find((tr) => tr.status !== 'Working');
    const pick = failing ?? trackers[0];
    return pick.message || 'No announce info';
  })();
  // Announce telemetry aggregated across the selected torrent's trackers:
  // most recent successful announce + soonest scheduled one. Recomputed on
  // each render; the manager re-renders on every telemetry poll.
  const announceTimes = (() => {
    const nowSec = Math.floor(Date.now() / 1000);
    const trackers = selectedTorrent?.trackers ?? [];
    const lastTimes = trackers.map((tr) => tr.last_announce).filter((v): v is number => v != null && v > 0);
    const nextTimes = trackers.map((tr) => tr.next_announce).filter((v): v is number => v != null && v > 0);
    const last = lastTimes.length ? Math.max(...lastTimes) : null;
    const next = nextTimes.length ? Math.min(...nextTimes) : null;
    const fmtRel = (ts: number, suffix: 'ago' | 'left') => {
      const d = Math.abs(nowSec - ts);
      const unit = d < 60 ? `${d}s` : d < 3600 ? `${Math.floor(d / 60)}m ${d % 60}s` : d < 86400 ? `${Math.floor(d / 3600)}h ${Math.floor((d % 3600) / 60)}m` : `${Math.floor(d / 86400)}d ${Math.floor((d % 86400) / 3600)}h`;
      return suffix === 'ago' ? `${unit} ago` : `in ${unit}`;
    };
    return {
      lastRel: last ? fmtRel(last, 'ago') : 'No successful announce yet',
      lastAbs: last ? new Date(last * 1000).toLocaleString() : '',
      nextRel: next ? fmtRel(next, 'left') : 'Not scheduled',
      nextAbs: next ? new Date(next * 1000).toLocaleString() : '',
    };
  })();

  // Sample the selected torrent's average rates on a fixed 1s cadence.
  // Triggering on value CHANGE froze the timeline during quiet periods
  // (rates sit at exactly 0 when no transfer is active, so deps never
  // changed and the graph stopped advancing) — a graph must tick even
  // when speeds are zero. The traced values are since-transfer-began
  // averages, so the line moves slowly and honestly.
  const selectedIdRef = useRef(selectedId);
  selectedIdRef.current = selectedId;
  useEffect(() => {
    const interval = setInterval(() => {
      const { torrents: current } = useTorrentStore.getState();
      const sel =
        current.find((t) => t.id === selectedIdRef.current) || current[0];
      if (sel) {
        setSpeedHistory((prev) =>
          [...prev, { dl: sel.download_rate, ul: sel.upload_rate }].slice(-40)
        );
      }
    }, 1000);
    return () => clearInterval(interval);
  }, []);

  // One-shot re-announce when a torrent's trackers recover after a full
  // outage: the seeds/peers columns otherwise keep showing the frozen
  // pre-outage counts until the next scheduled announce. Cooldown per
  // torrent (10 min) so a flapping tracker is not flooded.
  const wasFailingRef = useRef<Map<string, boolean>>(new Map());
  const reannounceAtRef = useRef<Map<string, number>>(new Map());
  useEffect(() => {
    torrents.forEach((t) => {
      const failing = trackersAllFailingCheck(t);
      const was = wasFailingRef.current.get(t.id) ?? false;
      wasFailingRef.current.set(t.id, failing);
      if (was && !failing) {
        const last = reannounceAtRef.current.get(t.id) ?? 0;
        if (Date.now() - last > 10 * 60 * 1000) {
          reannounceAtRef.current.set(t.id, Date.now());
          invoke('reannounce_torrent', { id: t.id }).catch(() => {});
          invoke('log_frontend', {
            message: `reannounce: triggered after tracker recovery for "${t.name}"`,
          }).catch(() => {});
        }
      }
    });
  }, [torrents]);

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

  const [copiedField, setCopiedField] = useState<string | null>(null);

  const handleCopy = (text: string, fieldName: string) => {
    if (!text) return;
    navigator.clipboard.writeText(text);
    setCopiedField(fieldName);
    setTimeout(() => setCopiedField((prev) => (prev === fieldName ? null : prev)), 2000);
  };

  const handleCopyHash = () => {
    if (selectedTorrent?.info_hash) {
      handleCopy(selectedTorrent.info_hash, 'info_hash');
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

  const openRenameFileModal = (fileIndex: number, fileName: string) => {
    if (!selectedTorrent) return;
    setRenameError(null);
    setRenameModal({
      isOpen: true,
      type: 'file',
      id: selectedTorrent.id,
      fileIndex,
      currentName: fileName,
      newName: fileName,
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
            onContextMenu={(e) => {
              e.preventDefault();
              if (isTransmission && !isRemoteServer && node.path) {
                openRenameFolderModal(node.path, node.name);
              }
            }}
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

            <FileProgressBar progress={nodeProgress(node)} sizeBytes={node.size_bytes} />

            <div className="flex items-center gap-2 flex-shrink-0 opacity-80 group-hover:opacity-100 transition">
              {!isRemoteServer && (
                isTransmission || !node.path.includes('/') ? (
                  <button
                    type="button"
                    onClick={() => openRenameFolderModal(node.path, node.name)}
                    className="p-1.5 rounded bg-[#141414] hover:bg-[#262626] border border-[#333333] text-[#a3a3a3] hover:text-white flex items-center transition"
                    title={`Rename the folder "${node.name}"`}
                  >
                    <Edit2 className="w-3 h-3 text-[#e50914]" />
                  </button>
                ) : (
                  <span
                    className="p-1.5 rounded bg-[#141414] border border-[#1f1f1f] text-[#525252] flex items-center cursor-not-allowed"
                    title="Nested folders cannot be renamed with the active engine"
                  >
                    <Edit2 className="w-3 h-3 text-[#525252]" />
                  </span>
                )
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
        onContextMenu={(e) => {
          e.preventDefault();
          if (node.file && isTransmission && !isRemoteServer) {
            openRenameFileModal(node.file.index, node.name);
          }
        }}
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

        <FileProgressBar progress={file.progress} sizeBytes={file.size_bytes} />

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
                const t = selectedTorrent!;
                openInPlayer(fullPath, {
                  displayTitle: t.media_meta?.clean_title || t.name,
                  year: t.media_meta?.year,
                }).catch(() => {});
              }}
              className="px-2 py-0.5 rounded bg-[#141414] hover:bg-[#262626] border border-[#333333] text-[#a3a3a3] hover:text-white flex items-center gap-1 text-[10px] transition"
              title={`Play "${node.name}" in media player`}
            >
              <Play className="w-3 h-3 text-[#e50914]" />
              <span>Play</span>
            </button>
          )}
          {!isRemoteServer && (
            isTransmission ? (
              <button
                type="button"
                onClick={() => node.file && openRenameFileModal(node.file.index, node.name)}
                className="p-1.5 rounded bg-[#141414] hover:bg-[#262626] border border-[#333333] text-[#a3a3a3] hover:text-white flex items-center transition"
                title={`Rename file "${node.name}"`}
              >
                <Edit2 className="w-3 h-3 text-[#e50914]" />
              </button>
            ) : (
              <span
                className="p-1.5 rounded bg-[#141414] border border-[#1f1f1f] text-[#525252] flex items-center cursor-not-allowed"
                title="Individual files cannot be renamed with the active engine"
              >
                <Edit2 className="w-3 h-3 text-[#525252]" />
              </span>
            )
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="flex-1 flex overflow-hidden font-sans select-none bg-[#141414] text-white">
      <PosterHoverPopup
        target={posterPopup?.target ?? null}
        x={posterPopup?.x ?? 0}
        y={posterPopup?.y ?? 0}
      />
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
                onClick={() => {
                  setManagerCategoryFilter(item.id);
                  setManagerTrackerFilter(null);
                }}
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
                onClick={() => {
                  setManagerCategoryFilter(item.id);
                  setManagerTrackerFilter(null);
                }}
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

        {/* qBittorrent-style Trackers list: unique tracker hosts across all
            torrents; click filters the grid to that tracker, click again to
            clear. Health dot aggregates per-torrent tracker status. */}
        {trackerEntries.length > 0 && (
          <div>
            <span className="text-[10px] font-bold uppercase tracking-wider text-[#a3a3a3] px-2 block mb-1">
              Trackers
            </span>
            <div className="space-y-0.5">
              {trackerEntries.map(({ host, count, healthy }) => (
                <button
                  key={host}
                  onClick={() =>
                    setManagerTrackerFilter(managerTrackerFilter === host ? null : host)
                  }
                  title={host}
                  className={`w-full flex items-center justify-between gap-1 px-2.5 py-1.5 rounded text-left transition ${
                    managerTrackerFilter === host
                      ? 'bg-[#e50914] text-white font-semibold'
                      : 'text-[#a3a3a3] hover:text-white hover:bg-[#1f1f1f]'
                  }`}
                >
                  <span className="flex items-center gap-1.5 min-w-0">
                    <span
                      className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${
                        healthy === count ? 'bg-emerald-500' : 'bg-amber-500'
                      }`}
                    />
                    <span className="truncate">{host}</span>
                  </span>
                  <span className="text-[10px] font-mono opacity-80">{count}</span>
                </button>
              ))}
            </div>
          </div>
        )}
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
                // Not connected: the torrent itself errored, or every one of
                // its trackers reports failure/error. The whole row turns
                // the app red and reverts once connectivity returns.
                const trackersAllFailing = trackersAllFailingCheck(t);
                const rowError = t.status === 'Error' || trackersAllFailing;
                const rowErrorClass = rowError ? 'text-[#e50914] hover:text-[#e50914]' : '';
                // While downloading the bar carries the downloaded volume;
                // once complete it switches to the uploaded volume (seeders
                // read contribution, not progress). Checking states display
                // verification progress.
                const isChecking = t.status === 'Checking' || t.status === 'Queued for checking';
                const isComplete =
                  !isChecking && (t.status === 'Seeding' || t.status === 'Completed' || t.progress >= 1);
                return (
                  <tr
                    key={t.id}
                    onClick={() => setSelectedId(t.id)}
                    onContextMenu={(e) => handleContextMenu(e, t.id)}
                    onDoubleClick={isRemoteServer ? undefined : openRenameTorrentModal}
                    className={`cursor-pointer transition select-none ${
                      isSelected
                        ? `bg-[#1f1f1f] border-l-2 border-[#e50914] text-white ${rowErrorClass}`
                        : rowError
                          ? 'hover:bg-[#1a1a1a] text-[#e50914]'
                          : 'hover:bg-[#1a1a1a] text-[#a3a3a3] hover:text-white'
                    }`}
                  >
                    <td className={`py-2 px-3 font-sans font-medium truncate max-w-xs ${rowError ? 'text-[#e50914]' : 'text-white'}`}>
                      <span
                        onMouseEnter={(e) => openPosterPopup(t, e)}
                        onMouseLeave={closePosterPopup}
                      >
                        {t.name}
                      </span>
                    </td>
                    <td className="py-2 px-2.5">{formatBytes(t.total_bytes)}</td>
                    <td className="py-2 px-2.5">
                      <div className="flex flex-col gap-0.5 min-w-[150px]">
                        <div className="flex items-center gap-2">
                          <div className="flex-1 h-1.5 rounded-full bg-[#333333] overflow-hidden">
                            <div
                              className="h-full bg-white rounded-full transition-all"
                              style={{ width: `${progressPct}%` }}
                            />
                          </div>
                          <span className={`text-[10px] w-8 text-right font-mono ${rowError ? 'text-[#e50914]' : 'text-white'}`}>
                            {progressPct}%
                          </span>
                        </div>
                        <span className="text-[9px] font-mono text-[#737373] text-center leading-tight">
                          {isChecking
                            ? t.status === 'Queued for checking'
                              ? 'Queued for checking'
                              : `Verifying… ${progressPct}%`
                            : isComplete
                              ? `${formatBytes(t.uploaded_bytes)} uploaded`
                              : `${formatBytes(t.downloaded_bytes)} of ${formatBytes(t.total_bytes)}`}
                        </span>
                      </div>
                    </td>
                    <td className="py-2 px-2.5">
                      {t.status === 'Downloading' && t.racingMode ? (
                        <span className="text-[11px] font-sans text-white inline-flex items-center gap-1">
                          <Zap className="w-3 h-3 text-[#e50914] flex-shrink-0" fill="currentColor" />
                          <span>Racing mode</span>
                        </span>
                      ) : t.status === 'Seeding' && t.super_seeding ? (
                        <span className="text-[11px] font-sans text-white inline-flex items-center gap-1">
                          <Zap className="w-3 h-3 text-emerald-400 flex-shrink-0" fill="currentColor" />
                          <span>Super seeding</span>
                        </span>
                      ) : (
                        <span className="text-[11px] font-sans text-white">{t.status}</span>
                      )}
                    </td>
                    <td
                      className="py-2 px-2.5 text-white whitespace-nowrap"
                      title={t.current_download_rate !== undefined ? `Since-transfer average shown. Current rate: ${formatSpeed(t.current_download_rate)}` : undefined}
                    >
                      {formatSpeed(t.download_rate)}
                      <span className="ml-1 text-[9px] uppercase text-[#737373]">avg</span>
                    </td>
                    <td
                      className="py-2 px-2.5 whitespace-nowrap"
                      title={t.current_upload_rate !== undefined ? `Since-transfer average shown. Current rate: ${formatSpeed(t.current_upload_rate)}` : undefined}
                    >
                      {formatSpeed(t.upload_rate)}
                      <span className="ml-1 text-[9px] uppercase text-[#737373]">avg</span>
                    </td>
                    <td
                      className="py-2 px-2.5"
                      title={t.eta_seconds !== undefined ? 'Estimated from the average download rate since the transfer began' : undefined}
                    >
                      {formatEta(t.eta_seconds)}
                    </td>
                    <td className="py-2 px-2.5 text-white">{formatRatio(t.ratio)}</td>
                    <td className="py-2 px-2.5 text-white">
                      {/* Swarm seed count from the engine's tracker scrape.
                          A dash means no successful scrape yet - never fake
                          a zero. */}
                      {trackersAllFailing ? '—' : (t.seeds ?? '—')}
                    </td>
                    <td className="py-2 px-2.5">{trackersAllFailing ? '—' : t.peers}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>

        {/* Divider: drag up/down to resize the bottom inspector */}
        <div
          role="separator"
          aria-orientation="horizontal"
          onMouseDown={startInspectorDrag}
          className="h-1.5 cursor-row-resize hover:bg-[#e50914]/30 transition-colors flex items-center justify-center flex-shrink-0 bg-[#141414]"
          title="Drag to resize the details panel"
        >
          <div className="w-16 h-0.5 rounded-full bg-[#333333]" />
        </div>

        {/* 3. Bottom Inspector: Tabs */}
        <div
          className="bg-[#1a1a1a] border-t border-[#262626] flex flex-col flex-shrink-0"
          style={{ height: inspectorHeight }}
        >
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
                (() => {
                  const trackersAllFailing = trackersAllFailingCheck(selectedTorrent);
                  return (
                    <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                      {/* Card 1: Transfer */}
                      <div className="bg-[#1b1b1b] rounded-lg border border-[#2a2a2a] p-3 flex flex-col justify-between space-y-3">
                        <div>
                          <div className="border-b border-[#262626] pb-1.5 mb-2.5 flex items-center justify-between">
                            <h4 className="text-[11px] font-bold text-white uppercase tracking-wider font-mono flex items-center gap-1.5">
                              <Activity className="w-3.5 h-3.5 text-[#e50914]" />
                              <span>Transfer</span>
                            </h4>
                          </div>
                          <div className="grid grid-cols-2 gap-x-3 gap-y-2.5 font-mono text-xs">
                            <div className="space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Time Active
                              </span>
                              <span className="text-white">
                                {selectedTorrent.added_on > 0 ? formatAnnounceDelta(selectedTorrent.added_on) : '—'}
                              </span>
                            </div>
                            <div className="space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                ETA
                              </span>
                              <span className="text-white">
                                {formatEta(selectedTorrent.eta_seconds)}
                              </span>
                            </div>
                            <div className="space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Downloaded
                              </span>
                              <span className="text-white">
                                {formatBytes(selectedTorrent.downloaded_bytes)} ({(selectedTorrent.progress * 100).toFixed(1)}%)
                              </span>
                            </div>
                            <div className="space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Uploaded
                              </span>
                              <span className="text-white">
                                {formatBytes(selectedTorrent.uploaded_bytes)} ({formatRatio(selectedTorrent.ratio)})
                              </span>
                            </div>
                            <div className="space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                DL Speed
                              </span>
                              <span
                                className="text-white"
                                title={selectedTorrent.current_download_rate !== undefined ? `Since-transfer average shown. Current rate: ${formatSpeed(selectedTorrent.current_download_rate)}` : undefined}
                              >
                                {formatSpeed(selectedTorrent.download_rate)}
                                <span className="ml-1 text-[9px] uppercase text-[#a3a3a3]">avg</span>
                                {selectedTorrent.download_limit_kib > 0 ? ` [${selectedTorrent.download_limit_kib}K]` : ''}
                              </span>
                            </div>
                            <div className="space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                UL Speed
                              </span>
                              <span
                                className="text-white"
                                title={selectedTorrent.current_upload_rate !== undefined ? `Since-transfer average shown. Current rate: ${formatSpeed(selectedTorrent.current_upload_rate)}` : undefined}
                              >
                                {formatSpeed(selectedTorrent.upload_rate)}
                                <span className="ml-1 text-[9px] uppercase text-[#a3a3a3]">avg</span>
                                {selectedTorrent.upload_limit_kib > 0 ? ` [${selectedTorrent.upload_limit_kib}K]` : ''}
                              </span>
                            </div>
                            <div className="space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Seeds
                              </span>
                              <span className="text-white">
                                {trackersAllFailing ? '—' : (selectedTorrent.seeds ?? '—')}
                              </span>
                            </div>
                            <div className="space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Peers
                              </span>
                              <span className="text-white">
                                {trackersAllFailing ? '—' : selectedTorrent.peers}
                                {selectedTorrent.peerLimit ? ` (${selectedTorrent.peerLimit} max)` : ''}
                              </span>
                            </div>
                          </div>
                        </div>
                      </div>

                      {/* Card 2: Connections & Trackers */}
                      <div className="bg-[#1b1b1b] rounded-lg border border-[#2a2a2a] p-3 flex flex-col justify-between space-y-3">
                        <div>
                          <div className="border-b border-[#262626] pb-1.5 mb-2.5 flex items-center justify-between">
                            <h4 className="text-[11px] font-bold text-white uppercase tracking-wider font-mono flex items-center gap-1.5">
                              <Layers className="w-3.5 h-3.5 text-[#e50914]" />
                              <span>Connections & Trackers</span>
                            </h4>
                          </div>
                          <div className="grid grid-cols-2 gap-x-3 gap-y-2.5 font-mono text-xs">
                            <div className="col-span-2 space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Status
                              </span>
                              <div className="flex items-center gap-2 flex-wrap">
                                <span className="text-white font-medium">{selectedTorrent.status}</span>
                                {selectedTorrent.status === 'Downloading' && selectedTorrent.racingMode && (
                                  <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-semibold bg-red-950/80 text-red-400 border border-red-700/50">
                                    <Zap className="w-2.5 h-2.5 text-red-500 fill-current" />
                                    <span>Racing mode</span>
                                  </span>
                                )}
                                {selectedTorrent.status === 'Seeding' && selectedTorrent.super_seeding && (
                                  <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-semibold bg-emerald-950/80 text-emerald-400 border border-emerald-700/50">
                                    <Zap className="w-2.5 h-2.5 text-emerald-500 fill-current" />
                                    <span>Super seeding</span>
                                  </span>
                                )}
                              </div>
                            </div>
                            <div className="col-span-2 space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Tracker Message
                              </span>
                              <span
                                className="text-white text-[11px] truncate block"
                                title={announceStatus}
                              >
                                {announceStatus}
                              </span>
                            </div>
                            <div className="space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Last Announce
                              </span>
                              <span className="text-white text-[11px]" title={announceTimes.lastAbs || ''}>
                                {announceTimes.lastRel}
                              </span>
                            </div>
                            <div className="space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Next Announce
                              </span>
                              <span className="text-white text-[11px]" title={announceTimes.nextAbs || ''}>
                                {announceTimes.nextRel}
                              </span>
                            </div>
                          </div>
                        </div>

                        {!isRemoteServer && (
                          <div className="pt-2 border-t border-[#262626] flex flex-wrap items-center gap-2">
                            <button
                              type="button"
                              onClick={async () => {
                                try {
                                  await toggleSequential(selectedTorrent.id);
                                } catch (err: unknown) {
                                  alert(`Could not toggle sequential download:\n${err}`);
                                }
                              }}
                              className={`px-2 py-1 rounded border text-[11px] flex items-center gap-1.5 transition ${
                                selectedTorrent.sequential_download
                                  ? 'bg-emerald-500/20 text-emerald-400 border-emerald-500/40'
                                  : 'bg-[#1f1f1f] text-[#a3a3a3] border-[#333333] hover:text-white'
                              }`}
                              title={`Sequential Downloading (Stream in order) — ${
                                selectedTorrent.sequential_download ? 'ON' : 'OFF'
                              }`}
                            >
                              <Zap className="w-3.5 h-3.5" />
                              <span>Sequential {selectedTorrent.sequential_download ? 'ON' : 'OFF'}</span>
                            </button>
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
                              className="px-2 py-1 rounded bg-[#1f1f1f] hover:bg-[#262626] border border-[#333333] text-[11px] text-white flex items-center gap-1.5 transition"
                              title="Edit speed and ratio limits"
                            >
                              <Sliders className="w-3.5 h-3.5 text-[#e50914]" />
                              <span>Edit Limits</span>
                            </button>
                          </div>
                        )}
                      </div>

                      {/* Card 3: Torrent Information */}
                      <div className="bg-[#1b1b1b] rounded-lg border border-[#2a2a2a] p-3 flex flex-col justify-between space-y-3">
                        <div>
                          <div className="border-b border-[#262626] pb-1.5 mb-2.5 flex items-center justify-between">
                            <h4 className="text-[11px] font-bold text-white uppercase tracking-wider font-mono flex items-center gap-1.5">
                              <FileText className="w-3.5 h-3.5 text-[#e50914]" />
                              <span>Torrent Information</span>
                            </h4>
                          </div>
                          <div className="grid grid-cols-2 gap-x-3 gap-y-2.5 font-mono text-xs">
                            <div className="col-span-2 space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Total Size
                              </span>
                              <span className="text-white">
                                {formatBytes(selectedTorrent.total_bytes)} ({selectedTorrent.total_pieces} pieces × {formatBytes(selectedTorrent.piece_size)})
                              </span>
                            </div>
                            <div className="space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Added On
                              </span>
                              <span className="text-white">
                                {selectedTorrent.added_on > 0 ? new Date(selectedTorrent.added_on * 1000).toLocaleString() : '—'}
                              </span>
                            </div>
                            <div className="space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Completed On
                              </span>
                              <span className="text-white">
                                {selectedTorrent.completed_on && selectedTorrent.completed_on > 0
                                  ? new Date(selectedTorrent.completed_on * 1000).toLocaleString()
                                  : '—'}
                              </span>
                            </div>
                            <div className="col-span-2 space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Save Path
                              </span>
                              <div className="flex items-center justify-between gap-1.5 bg-[#141414] px-2 py-1 rounded border border-[#262626]">
                                <span className="text-white truncate block text-[11px] font-mono" title={selectedTorrent.save_path}>
                                  {selectedTorrent.save_path}
                                </span>
                                <button
                                  type="button"
                                  onClick={() => handleCopy(selectedTorrent.save_path, 'save_path')}
                                  className="p-1 rounded hover:bg-[#262626] text-[#a3a3a3] hover:text-white transition flex-shrink-0"
                                  title="Copy Save Path"
                                >
                                  {copiedField === 'save_path' ? (
                                    <Check className="w-3 h-3 text-emerald-400" />
                                  ) : (
                                    <Copy className="w-3 h-3" />
                                  )}
                                </button>
                              </div>
                            </div>
                            <div className="col-span-2 space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Info Hash
                              </span>
                              <div className="flex items-center justify-between gap-1.5 bg-[#141414] px-2 py-1 rounded border border-[#262626]">
                                <span className="text-white truncate block text-[10px] font-mono" title={selectedTorrent.info_hash}>
                                  {selectedTorrent.info_hash}
                                </span>
                                <button
                                  type="button"
                                  onClick={() => handleCopy(selectedTorrent.info_hash, 'info_hash')}
                                  className="p-1 rounded hover:bg-[#262626] text-[#a3a3a3] hover:text-white transition flex-shrink-0"
                                  title="Copy Info Hash"
                                >
                                  {copiedField === 'info_hash' ? (
                                    <Check className="w-3 h-3 text-emerald-400" />
                                  ) : (
                                    <Copy className="w-3 h-3" />
                                  )}
                                </button>
                              </div>
                            </div>
                            <div className="space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Created By
                              </span>
                              <span className="text-white truncate block text-[11px]" title={selectedTorrent.created_by || '—'}>
                                {selectedTorrent.created_by || '—'}
                              </span>
                            </div>
                            <div className="space-y-0.5">
                              <span className="text-[10px] uppercase text-[#a3a3a3] block font-sans">
                                Comment
                              </span>
                              <span className="text-white truncate block text-[11px]" title={selectedTorrent.comment || '—'}>
                                {selectedTorrent.comment || '—'}
                              </span>
                            </div>
                          </div>
                        </div>
                      </div>
                    </div>
                  );
                })()
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
                            onContextMenu={(e) => {
                              e.preventDefault();
                              if (isTransmission && !isRemoteServer) {
                                openRenameFileModal(file.index, file.name);
                              }
                            }}
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
                                    openInPlayer(fullPath, {
                                      displayTitle: selectedTorrent.media_meta?.clean_title || selectedTorrent.name,
                                      year: selectedTorrent.media_meta?.year,
                                    }).catch(() => {});
                                  }}
                                  className="px-2 py-0.5 rounded bg-[#141414] hover:bg-[#262626] border border-[#333333] text-[#a3a3a3] hover:text-white flex items-center gap-1 text-[10px] transition"
                                  title="Play in media player"
                                >
                                  <Play className="w-3 h-3 text-[#e50914]" />
                                  <span>Play</span>
                                </button>
                              )}
                              {!isRemoteServer && (
                                isTransmission ? (
                                  <button
                                    type="button"
                                    onClick={() => openRenameFileModal(file.index, file.name)}
                                    className="px-2 py-0.5 rounded bg-[#141414] hover:bg-[#262626] border border-[#333333] text-[#a3a3a3] hover:text-white flex items-center gap-1 text-[10px] transition"
                                    title={`Rename file "${file.name}"`}
                                  >
                                    <Edit2 className="w-3 h-3 text-[#e50914]" />
                                    <span>Rename</span>
                                  </button>
                                ) : (
                                  <span
                                    className="px-2 py-0.5 rounded bg-[#141414] border border-[#1f1f1f] text-[#525252] flex items-center gap-1 text-[10px] cursor-not-allowed"
                                    title="Individual files cannot be renamed with the active engine"
                                  >
                                    <Edit2 className="w-3 h-3 text-[#525252]" />
                                    <span>Rename</span>
                                  </span>
                                )
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
                          <span className="text-[10px] text-slate-500">
                            Tier {tr.tier} • Seeds: {tr.seeds ?? '—'} • Peers: {tr.peers ?? '—'}
                            {tr.next_announce && tr.next_announce > 0
                              ? ` • Next announce: in ${formatAnnounceUntil(tr.next_announce)}`
                              : ''}
                            {tr.last_announce
                              ? ` • Last: ${formatAnnounceDelta(tr.last_announce)} ago`
                              : ''}
                          </span>
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
                /* ── Peers Tab ─────────────────────────────────────────── */
                <PeersTabPanel
                  peers={peers}
                  settings={peeringSettings}
                  fetchSettings={fetchSettings}
                  saveSettings={saveSettings}
                  isTransmission={isTransmission}
                />
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
                    <span className="text-[10px]">Average Since Transfer Began (1s samples)</span>
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
          {!isRemoteServer && (
            <button
              onClick={async () => {
                const t = torrents.find((item) => item.id === contextMenu.torrentId);
                setContextMenu((prev) => ({ ...prev, visible: false }));
                if (!t) return;
                const folder = await selectDownloadDirectory();
                if (folder) setLocation(t.id, folder);
              }}
              className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-[#e50914] text-white text-left transition"
            >
              <FolderInput className="w-3.5 h-3.5" />
              <span>Move to Folder...</span>
            </button>
          )}
          <button
            onClick={async () => {
              const targetId = contextMenu.torrentId;
              setContextMenu((prev) => ({ ...prev, visible: false }));
              try {
                const res = await forceRecheck(targetId);
                if (!res.success && res.error) {
                  console.warn(`[TorrentManager] Could not start verification for ${targetId}:`, res.error);
                }
              } catch (err) {
                console.error(`[TorrentManager] forceRecheck failed:`, err);
              }
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
          <button
            onClick={async () => {
              const t = torrents.find((item) => item.id === contextMenu.torrentId);
              setContextMenu((prev) => ({ ...prev, visible: false }));
              if (t && t.save_path) {
                try {
                  await invoke('open_torrent_folder', { path: resolveTorrentDataDir(t) });
                } catch (err: unknown) {
                  alert(`Could not reveal in Finder:\n${err}`);
                }
              }
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-[#e50914] text-white text-left transition"
          >
            <FolderOpen className="w-3.5 h-3.5" />
            <span>Show in Finder</span>
          </button>
          <button
            onClick={() => {
              invoke('save_torrent_file_as', { id: contextMenu.torrentId })
                .then((savedPath) => {
                  // Confirm what happened - the dialog can land behind
                  // other windows and look like nothing happened.
                  if (typeof savedPath === 'string' && savedPath) {
                    alert(`Torrent file saved to:\n${savedPath}`);
                  }
                })
                .catch((e) => {
                  if (typeof e === 'string' && e.includes('cancelled')) return;
                  alert(`Could not save the torrent file:\n${e}`);
                });
              setContextMenu((prev) => ({ ...prev, visible: false }));
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-[#e50914] text-white text-left transition"
          >
            <Save className="w-3.5 h-3.5" />
            <span>Save Torrent File As...</span>
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
          {(() => {
            // Sequential download only makes sense mid-download; hide the
            // toggle for completed/seeding/error/stopped torrents.
            const t = torrents.find((item) => item.id === contextMenu.torrentId);
            if (t?.status !== 'Downloading') return null;
            return (
              <button
                onClick={async () => {
                  const tid = contextMenu.torrentId;
                  setContextMenu((prev) => ({ ...prev, visible: false }));
                  try {
                    await toggleSequential(tid);
                  } catch (err: unknown) {
                    alert(`Could not toggle sequential download:\n${err}`);
                  }
                }}
                className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-[#e50914] text-white text-left transition"
              >
                <Zap className="w-3.5 h-3.5" />
                <span>Toggle Sequential Download</span>
              </button>
            );
          })()}
          {!isRemoteServer && (() => {
            const t = torrents.find((item) => item.id === contextMenu.torrentId);
            const isRacing = t?.racingMode || (t?.bandwidthPriority === 1 && (t?.peerLimit ?? 0) >= 150);
            return (
              <button
                onClick={() => {
                  invoke('set_torrent_racing_mode', { id: contextMenu.torrentId, enabled: !isRacing })
                    .catch(() => {});
                  setContextMenu((prev) => ({ ...prev, visible: false }));
                }}
                className="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-[#e50914] text-white text-left transition"
              >
                <Zap className="w-3.5 h-3.5 text-[#e50914]" />
                <span>{isRacing ? 'Disable Racing Mode preset' : 'Enable Racing Mode preset'}</span>
              </button>
            );
          })()}
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
