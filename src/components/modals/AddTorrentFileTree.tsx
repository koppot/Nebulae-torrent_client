import React, { useEffect, useMemo, useRef, useState } from 'react';
import { ChevronDown, ChevronRight, Edit2, File as FileIcon, Folder as FolderIcon } from 'lucide-react';
import type { StagedTorrentRename } from '../../types';

// F-2 (task order section 3): the Add Torrent file tree. The torrent does not
// exist yet, so every choice here is STAGED and applied by
// add_torrent_staged after the torrent is added PAUSED:
//   add paused -> rename (root, folders, files) -> exclude files -> start.
// Renames are recorded against the ORIGINAL paths (the backend applies them
// deepest-first and re-resolves current paths itself), so the tree only needs
// to collect "original path -> new name" pairs.

export interface AddTorrentFileTreeProps {
  /** The torrent's ORIGINAL top-level name (paths sent to the engine are prefixed with it). */
  torrentName: string;
  /** Metainfo file list; names are relative to the torrent root (no prefix). */
  files: { index: number; name: string; size_bytes: number }[];
  /** Controlled new root name (starts as torrentName; null change = unchanged). */
  rootName: string;
  onRootNameChange: (name: string) => void;
  /** Controlled set of file indices NOT to download (empty = all wanted). */
  excluded: Set<number>;
  onExcludedChange: (next: Set<number>) => void;
  /** Controlled staged renames (original path -> new name). */
  renames: StagedTorrentRename[];
  onRenamesChange: (next: StagedTorrentRename[]) => void;
}

interface TreeNode {
  /** Path relative to the torrent root ('' for the root node itself). */
  path: string;
  name: string;
  isFolder: boolean;
  file?: { index: number; size_bytes: number };
  sizeBytes: number;
  fileCount: number;
  children: TreeNode[];
}

const ROW_H = 28; // px, must match the row height class below
const VIEWPORT_H = 288; // px of visible list
const OVERSCAN = 8;
// Below this many rows the window covers the whole list anyway; the windowing
// math is identical, so no special case is needed.

const formatBytes = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
};

const isValidName = (name: string): boolean => {
  const t = name.trim();
  return (
    t.length > 0 &&
    !t.includes('/') &&
    !t.includes('\\') &&
    t !== '.' &&
    t !== '..'
  );
};

type SelectionState = 'all' | 'none' | 'some';

const TriStateCheckbox: React.FC<{
  state: SelectionState;
  onChange: (wanted: boolean) => void;
  label: string;
}> = ({ state, onChange, label }) => {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = state === 'some';
  }, [state]);
  return (
    <input
      ref={ref}
      type="checkbox"
      checked={state === 'all'}
      onChange={(e) => onChange(e.target.checked)}
      onClick={(e) => e.stopPropagation()}
      title={label}
      aria-label={label}
      className="w-3.5 h-3.5 flex-shrink-0 accent-[#e50914] cursor-pointer"
    />
  );
};

export const AddTorrentFileTree: React.FC<AddTorrentFileTreeProps> = ({
  torrentName,
  files,
  rootName,
  onRootNameChange,
  excluded,
  onExcludedChange,
  renames,
  onRenamesChange,
}) => {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState('');
  const [renaming, setRenaming] = useState<{ key: string; value: string } | null>(null);
  const [invalidName, setInvalidName] = useState(false);
  const [scrollTop, setScrollTop] = useState(0);
  const renameInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (renaming) renameInputRef.current?.select();
  }, [renaming]);

  const renameMap = useMemo(() => {
    const m = new Map<string, string>();
    for (const r of renames) m.set(r.path, r.name);
    return m;
  }, [renames]);

  // Build the tree from the metainfo layout. Names may carry no root prefix
  // (single-file) or full inner paths (multi-file).
  const tree = useMemo<{ root: TreeNode; singleFile: boolean }>(() => {
    const root: TreeNode = {
      path: '',
      name: rootName,
      isFolder: true,
      sizeBytes: 0,
      fileCount: 0,
      children: [],
    };
    if (files.length === 1 && !files[0].name.includes('/')) {
      // Single-file torrent: the root row IS the file.
      const f = files[0];
      root.isFolder = false;
      root.file = { index: f.index, size_bytes: f.size_bytes };
      root.sizeBytes = f.size_bytes;
      root.fileCount = 1;
      return { root, singleFile: true };
    }
    for (const f of files) {
      const parts = f.name.split('/');
      let node = root;
      let pathSoFar = '';
      for (let i = 0; i < parts.length; i++) {
        const part = parts[i];
        pathSoFar = pathSoFar ? `${pathSoFar}/${part}` : part;
        const isLeaf = i === parts.length - 1;
        let child = node.children.find((c) => c.name === part && c.isFolder === !isLeaf);
        if (!child) {
          child = {
            path: pathSoFar,
            name: part,
            isFolder: !isLeaf,
            sizeBytes: 0,
            fileCount: 0,
            children: [],
          };
          node.children.push(child);
        }
        if (isLeaf) {
          child.file = { index: f.index, size_bytes: f.size_bytes };
        }
        node = child;
      }
    }
    const aggregate = (n: TreeNode): void => {
      if (n.isFolder) {
        for (const c of n.children) aggregate(c);
        n.sizeBytes = n.children.reduce((s, c) => s + c.sizeBytes, 0);
        n.fileCount = n.children.reduce((s, c) => s + c.fileCount, 0);
      } else {
        n.sizeBytes = n.file?.size_bytes ?? 0;
        n.fileCount = 1;
      }
    };
    aggregate(root);
    root.children.sort((a, b) =>
      a.isFolder === b.isFolder ? a.name.localeCompare(b.name) : a.isFolder ? -1 : 1,
    );
    return { root, singleFile: false };
  }, [files, rootName]);

  const descendantIndices = useMemo(() => {
    const collect = (n: TreeNode, acc: number[]): number[] => {
      if (n.isFolder) {
        for (const c of n.children) collect(c, acc);
      } else if (n.file) {
        acc.push(n.file.index);
      }
      return acc;
    };
    const map = new Map<TreeNode, number[]>();
    const walk = (n: TreeNode): number[] => {
      const acc = collect(n, []);
      map.set(n, acc);
      for (const c of n.children) walk(c);
      return acc;
    };
    walk(tree.root);
    return map;
  }, [tree]);

  const selectionState = (indices: number[]): SelectionState => {
    if (indices.length === 0) return 'all';
    const excludedCount = indices.filter((i) => excluded.has(i)).length;
    if (excludedCount === 0) return 'all';
    if (excludedCount === indices.length) return 'none';
    return 'some';
  };

  const toggleIndices = (indices: number[], want: boolean): void => {
    const next = new Set(excluded);
    for (const i of indices) {
      if (want) next.delete(i);
      else next.add(i);
    }
    onExcludedChange(next);
  };

  // A node matches the filter when it is a file whose name (or full path)
  // contains the filter, or a folder with a matching descendant.
  const matchesFilter = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return null;
    const matching = new Set<string>();
    const walk = (n: TreeNode): boolean => {
      let any = false;
      for (const c of n.children) any = walk(c) || any;
      const self = !n.isFolder && n.name.toLowerCase().includes(q);
      if (self || any) matching.add(n.path);
      return self || any;
    };
    walk(tree.root);
    // The root path is '' — a direct name hit on the root is not a file hit.
    if (tree.root.isFolder && !matching.has('')) {
      // keep root visible as the tree's anchor even when nothing matches yet
    }
    return matching;
  }, [filter, tree]);

  // Flatten visible rows (respecting collapse + filter) for rendering.
  const rows = useMemo(() => {
    const out: TreeNode[] = [];
    const push = (n: TreeNode): void => {
      out.push(n);
      if (!n.isFolder) return;
      if (matchesFilter) {
        for (const c of n.children) if (matchesFilter.has(c.path)) push(c);
      } else if (!collapsed.has(n.path)) {
        for (const c of n.children) push(c);
      }
    };
    push(tree.root);
    return out;
  }, [tree, collapsed, matchesFilter]);

  const startRename = (n: TreeNode): void => {
    if (tree.singleFile && n === tree.root) {
      // Single-file torrents rename the file through the root field.
      setRenaming({ key: '', value: rootName });
      return;
    }
    if (n.isFolder && n.path === '' && !tree.singleFile) {
      setRenaming({ key: '', value: rootName });
      return;
    }
    setRenaming({ key: n.path, value: renameMap.get(n.path) ?? n.name });
    setInvalidName(false);
  };

  const commitRename = (): void => {
    if (!renaming) return;
    const value = renaming.value.trim();
    if (!isValidName(value)) {
      setInvalidName(true);
      return;
    }
    if (renaming.key === '') {
      // Root (or the single file): rename the torrent's top-level name.
      if (value !== rootName) onRootNameChange(value);
    } else if ((renameMap.get(renaming.key) ?? undefined) !== value) {
      const original = files.find((f) => f.name === renaming.key);
      const isFolder = !original;
      const next = renames.filter((r) => r.path !== renaming.key);
      next.push({
        path: `${torrentName}/${renaming.key}`,
        name: value,
        is_folder: isFolder,
        file_index: original?.index ?? 0,
      });
      onRenamesChange(next);
    }
    setRenaming(null);
    setInvalidName(false);
  };

  const selectedCount = files.length - excluded.size;
  const selectedBytes = files.reduce(
    (s, f) => s + (excluded.has(f.index) ? 0 : f.size_bytes),
    0,
  );

  // Windowing: render only the visible slice (thousands of files must not
  // freeze the dialog).
  const start = Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN);
  const end = Math.min(rows.length, Math.ceil((scrollTop + VIEWPORT_H) / ROW_H) + OVERSCAN);
  const visibleRows = rows.slice(start, end);

  const renderRow = (n: TreeNode, i: number): React.ReactNode => {
    const displayName = renameMap.get(n.path) ?? n.name;
    const isRoot = n.path === '' && n === tree.root;
    const indices = descendantIndices.get(n) ?? [];
    const sel = n.isFolder ? selectionState(indices) : excluded.has(n.file!.index) ? 'none' : 'all';
    const depth = n.path === '' ? 0 : n.path.split('/').length - (n.isFolder ? 1 : 0);
    const isRenaming = renaming?.key === n.path || (renaming?.key === '' && isRoot);
    const dimmed = n.isFolder
      ? sel === 'none'
      : excluded.has(n.file!.index);
    return (
      <div
        key={n.path || '__root__'}
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === 'F2') {
            e.preventDefault();
            startRename(n);
          }
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          startRename(n);
        }}
        className="absolute left-0 right-0 flex items-center gap-2 px-2 rounded hover:bg-[#1f1f1f] text-xs outline-none focus:bg-[#222222]"
        style={{ top: i * ROW_H, height: ROW_H, paddingLeft: `${depth * 16 + 8}px` }}
        title={n.isFolder ? `${n.fileCount} files · right-click or F2 to rename` : `${displayName} · right-click or F2 to rename`}
      >
        <TriStateCheckbox
          state={sel}
          onChange={(wanted) => toggleIndices(indices, wanted)}
          label={n.isFolder ? `Download folder "${displayName}"` : `Download "${displayName}"`}
        />
        <button
          type="button"
          className="flex-shrink-0 text-[#737373] hover:text-white disabled:invisible"
          disabled={matchesFilter !== null}
          onClick={() =>
            setCollapsed((prev) => {
              const next = new Set(prev);
              if (next.has(n.path)) next.delete(n.path);
              else next.add(n.path);
              return next;
            })
          }
          aria-label={collapsed.has(n.path) ? `Expand ${displayName}` : `Collapse ${displayName}`}
        >
          {n.isFolder ? (
            collapsed.has(n.path) || matchesFilter ? (
              <ChevronRight className="w-3 h-3" />
            ) : (
              <ChevronDown className="w-3 h-3" />
            )
          ) : (
            <span className="inline-block w-3" />
          )}
        </button>
        {n.isFolder ? (
          <FolderIcon className="w-3.5 h-3.5 text-[#e50914] flex-shrink-0" />
        ) : (
          <FileIcon className="w-3.5 h-3.5 text-[#a3a3a3] flex-shrink-0" />
        )}
        {isRenaming ? (
          <input
            ref={renameInputRef}
            type="text"
            value={renaming.value}
            autoFocus
            onChange={(e) => {
              setRenaming({ ...renaming, value: e.target.value });
              setInvalidName(false);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') commitRename();
              if (e.key === 'Escape') {
                setRenaming(null);
                setInvalidName(false);
              }
              e.stopPropagation();
            }}
            onBlur={commitRename}
            className={`flex-1 min-w-0 px-1.5 py-0.5 rounded bg-[#141414] border text-white font-mono text-[11px] focus:outline-none ${
              invalidName ? 'border-red-600' : 'border-[#e50914]'
            }`}
          />
        ) : (
          <span
            className={`truncate font-mono text-[11px] ${dimmed ? 'text-[#525252] line-through' : 'text-[#e5e5e5]'}`}
            title={n.isFolder ? `${torrentName}/${n.path}` : `${torrentName}/${n.path}`}
          >
            {displayName}
          </span>
        )}
        <span className="ml-auto text-[10px] text-[#737373] font-mono flex-shrink-0 tabular-nums">
          {formatBytes(n.sizeBytes)}
        </span>
        {!isRenaming && !dimmed && (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              startRename(n);
            }}
            className="p-0.5 rounded text-[#525252] hover:text-[#e50914] opacity-0 hover:opacity-100 focus:opacity-100 transition"
            title={`Rename "${displayName}"`}
            aria-label={`Rename "${displayName}"`}
          >
            <Edit2 className="w-3 h-3" />
          </button>
        )}
      </div>
    );
  };

  return (
    <div className="space-y-1.5">
      {/* Toolbar: select all/none, filter, summary */}
      <div className="flex items-center gap-1.5">
        <button
          type="button"
          onClick={() => onExcludedChange(new Set())}
          className="px-2 py-0.5 rounded bg-[#262626] hover:bg-[#333333] text-[10px] text-white transition"
        >
          Select all
        </button>
        <button
          type="button"
          onClick={() => onExcludedChange(new Set(files.map((f) => f.index)))}
          className="px-2 py-0.5 rounded bg-[#262626] hover:bg-[#333333] text-[10px] text-white transition"
        >
          Select none
        </button>
        <input
          type="text"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter files..."
          className="flex-1 min-w-0 px-2 py-0.5 rounded bg-[#141414] border border-[#333333] text-[10px] text-white font-mono focus:outline-none focus:border-[#e50914]"
        />
        <span className="text-[10px] text-[#a3a3a3] font-mono flex-shrink-0">
          {selectedCount}/{files.length} · {formatBytes(selectedBytes)}
        </span>
      </div>
      {renames.length > 0 && (
        <div className="text-[10px] text-[#d4c4a8]">
          {renames.length} rename{renames.length === 1 ? '' : 's'} staged (applied before the
          torrent starts).
          <button
            type="button"
            onClick={() => onRenamesChange([])}
            className="ml-2 text-[#e50914] hover:underline"
          >
            Undo all
          </button>
        </div>
      )}
      {filter.trim() !== '' && rows.length === 0 && (
        <div className="text-[10px] text-[#737373] px-2 py-2">No file matches the filter.</div>
      )}
      {/* Virtualized list */}
      <div
        className="overflow-y-auto border border-[#2a2a2a] rounded bg-[#181818] relative"
        style={{ height: Math.min(VIEWPORT_H, Math.max(rows.length * ROW_H, ROW_H)) }}
        onScroll={(e) => setScrollTop((e.target as HTMLDivElement).scrollTop)}
      >
        <div style={{ height: rows.length * ROW_H, position: 'relative' }}>
          {visibleRows.map((n, i) => renderRow(n, start + i))}
        </div>
      </div>
      <p className="text-[10px] text-[#737373]">
        Tick what to download. Right-click a row (or focus it and press F2) to rename the file or
        folder; names become part of the torrent, so the renames are applied to the paused torrent
        before it starts.
      </p>
    </div>
  );
};

export default AddTorrentFileTree;
