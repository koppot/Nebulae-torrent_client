import type { TorrentFileItem } from '../types/index';

export interface FileTreeNode {
  name: string;
  path: string;
  isFolder: boolean;
  size_bytes: number;
  file?: TorrentFileItem;
  children: FileTreeNode[];
}

export function buildFileTree(files: TorrentFileItem[]): FileTreeNode {
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

export function countFilesInNode(node: FileTreeNode): number {
  if (!node.isFolder) return 1;
  return node.children.reduce((acc, c) => acc + countFilesInNode(c), 0);
}

export function nodeMatchesFilter(node: FileTreeNode, query: string): boolean {
  if (!query) return true;
  const q = query.toLowerCase();
  if (node.name.toLowerCase().includes(q)) return true;
  if (node.isFolder) {
    return node.children.some((c) => nodeMatchesFilter(c, query));
  }
  return false;
}

/** Completed bytes of a node: a file's own progress times its size, a folder's
 * sum over every descendant file. */
export function nodeCompletedBytes(node: FileTreeNode): number {
  if (!node.isFolder) {
    return node.file ? Math.min(1, Math.max(0, node.file.progress)) * node.file.size_bytes : 0;
  }
  let done = 0;
  for (const child of node.children) done += nodeCompletedBytes(child);
  return done;
}

/** Indices of every file at or below the node. */
export function collectFileIndices(node: FileTreeNode): number[] {
  if (!node.isFolder) return node.file ? [node.file.index] : [];
  return node.children.flatMap(collectFileIndices);
}

export type SelectionState = 'all' | 'some' | 'none';

/** Whether the files below the node are selected for download (priority above
 * zero): all of them, none of them, or a mix. */
export function nodeSelection(node: FileTreeNode): SelectionState {
  let wanted = 0;
  let total = 0;
  const walk = (n: FileTreeNode) => {
    if (!n.isFolder) {
      if (n.file) {
        total += 1;
        if (n.file.priority > 0) wanted += 1;
      }
      return;
    }
    n.children.forEach(walk);
  };
  walk(node);
  if (total === 0 || wanted === 0) return 'none';
  return wanted === total ? 'all' : 'some';
}

/** Priority shared by every file below the node (0 = do not download ... 3 =
 * maximum), or 'mixed' when the files differ. */
export function nodePriority(node: FileTreeNode): number | 'mixed' {
  let seen: number | null = null;
  let mixed = false;
  const walk = (n: FileTreeNode) => {
    if (mixed) return;
    if (!n.isFolder) {
      if (!n.file) return;
      if (seen === null) seen = n.file.priority;
      else if (seen !== n.file.priority) mixed = true;
      return;
    }
    n.children.forEach(walk);
  };
  walk(node);
  if (mixed) return 'mixed';
  return seen ?? 1;
}

/** File progress as reported; folder progress is completed bytes over total
 * bytes of all descendant files (never an average of the children's percentages). */
export function nodeProgress(node: FileTreeNode): number {
  if (!node.isFolder) return node.file?.progress ?? 0;
  return node.size_bytes > 0 ? nodeCompletedBytes(node) / node.size_bytes : 0;
}

/** Whole-percent label that never rounds an unfinished item up to 100. */
export function progressPercent(progress: number): number {
  if (progress >= 1) return 100;
  return Math.max(0, Math.min(99, Math.floor(progress * 100)));
}

/** Folds a fresh per-file snapshot into the previous one, reusing the previous
 * object for every file that did not change so unchanged rows keep their identity.
 * Returns `prev` itself when nothing differs. */
export function mergeFileStats(
  prev: TorrentFileItem[] | undefined,
  next: TorrentFileItem[],
): TorrentFileItem[] {
  if (!prev || prev.length !== next.length) return next;
  let changed = false;
  const merged = next.map((n, i) => {
    const p = prev[i];
    if (
      p.index === n.index &&
      p.name === n.name &&
      p.size_bytes === n.size_bytes &&
      p.progress === n.progress &&
      p.priority === n.priority
    ) {
      return p;
    }
    changed = true;
    return n;
  });
  return changed ? merged : prev;
}

/** Orders overlapping requests per key. A response is accepted unless a response
 * to a newer request for the same key has already been applied, so a slow
 * earlier response can never overwrite fresher data, yet an earlier response
 * is still used when it is the only one to arrive. */
export function createLatestGate() {
  const issued = new Map<string, number>();
  const applied = new Map<string, number>();
  let counter = 0;
  return {
    begin(key: string): number {
      counter += 1;
      issued.set(key, counter);
      return counter;
    },
    accept(key: string, seq: number): boolean {
      if (seq <= (applied.get(key) ?? 0)) return false;
      applied.set(key, seq);
      return true;
    },
  };
}
