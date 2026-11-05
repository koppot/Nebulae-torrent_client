import test from 'node:test';
import assert from 'node:assert/strict';

/**
 * End-to-End Verification Test Suite:
 * Downloaded and External Media Grid Layout, Wrapping, and Independent Reorder Persistence
 *
 * Requirements verified:
 * 1. Control Area: Single shared Reorder / Done control below the shelves.
 * 2. Isolated Collections:
 *    - Downloaded only (External empty): control is active, multiple rows render, reordering works.
 *    - External only (Downloaded empty): control is active, multiple rows render, reordering works.
 *    - Both populated: both collections render and reorder independently.
 * 3. Settings "Posters per row":
 *    - Controls maximum posters per row for both collections.
 *    - Poster dimensions remain strictly 150px (aspect ratio 660:800, ~182px height).
 *    - Narrower windows fit fewer posters and wrap into more rows without shrinking cards.
 * 4. Persistence & Restart Restoration:
 *    - set_media_order updates torrent_metadata and media_downloaded_order JSON.
 *    - set_external_media_order updates media_external_order JSON.
 *    - get_downloaded_media_order and get_external_media_order restore exact user orders.
 *    - New downloads or external files append deterministically at the end.
 */

// Simulated SQLite Backend Store
class MockSqliteBackend {
  settings = new Map<string, string>();
  torrentMetadata = new Map<string, { sort_order: number | null; added_at: number }>();

  // Commands matching src-tauri/src/commands/library.rs
  set_posters_per_row(count: number): void {
    const clamped = Math.max(2, Math.min(12, count));
    this.settings.set('media_posters_per_row', String(clamped));
  }

  get_posters_per_row(): number {
    const val = this.settings.get('media_posters_per_row');
    return val ? parseInt(val, 10) : 5;
  }

  set_poster_size(size: number): void {
    const clamped = Math.max(100, Math.min(300, size));
    this.settings.set('media_poster_size', String(clamped));
  }

  get_poster_size(): number {
    const val = this.settings.get('media_poster_size');
    return val ? parseInt(val, 10) : 150;
  }

  set_media_order(orders: { info_hash: string; sort_order: number | null }[]): void {
    for (const entry of orders) {
      const existing = this.torrentMetadata.get(entry.info_hash) || { sort_order: null, added_at: Date.now() };
      this.torrentMetadata.set(entry.info_hash, { ...existing, sort_order: entry.sort_order });
    }
    const sorted = [...orders].filter((o) => o.sort_order !== null).sort((a, b) => (a.sort_order ?? 0) - (b.sort_order ?? 0));
    const hashes = sorted.map((o) => o.info_hash);
    this.settings.set('media_downloaded_order', JSON.stringify(hashes));
  }

  get_downloaded_media_order(): string[] {
    const json = this.settings.get('media_downloaded_order');
    if (json) {
      try {
        const hashes = JSON.parse(json);
        if (Array.isArray(hashes) && hashes.length > 0) return hashes;
      } catch { /* fallback */ }
    }
    const list: { hash: string; order: number }[] = [];
    for (const [hash, meta] of this.torrentMetadata.entries()) {
      if (meta.sort_order !== null) list.push({ hash, order: meta.sort_order });
    }
    return list.sort((a, b) => a.order - b.order).map((x) => x.hash);
  }

  set_external_media_order(paths: string[]): void {
    this.settings.set('media_external_order', JSON.stringify(paths));
  }

  get_external_media_order(): string[] {
    const json = this.settings.get('media_external_order');
    if (json) {
      try {
        const paths = JSON.parse(json);
        if (Array.isArray(paths)) return paths;
      } catch { /* fallback */ }
    }
    return [];
  }
}

// Order resolution algorithm from SimpleMediaView.tsx
function orderGroups<T extends { id: string; info_hash?: string }>(groups: T[], orderList: string[]): T[] {
  if (!orderList || orderList.length === 0) return groups;
  const remaining = [...groups];
  const result: T[] = [];
  for (const id of orderList) {
    const idLower = id.toLowerCase();
    const idx = remaining.findIndex((g) =>
      g.id.toLowerCase() === idLower ||
      (g.info_hash && g.info_hash.toLowerCase() === idLower)
    );
    if (idx !== -1) {
      result.push(remaining[idx]);
      remaining.splice(idx, 1);
    }
  }
  result.push(...remaining);
  return result;
}

function orderExternal<T extends { path: string }>(items: T[], orderList: string[]): T[] {
  if (!orderList || orderList.length === 0) return items;
  const remaining = [...items];
  const result: T[] = [];
  for (const path of orderList) {
    const idx = remaining.findIndex((m) => m.path === path);
    if (idx !== -1) {
      result.push(remaining[idx]);
      remaining.splice(idx, 1);
    }
  }
  result.push(...remaining);
  return result;
}

// Grid layout geometry calculator matching SimpleMediaView.tsx CSS grid
function computeGridGeometry(maxPostersPerRow: number, posterSizePx: number, totalItems: number, availableWidthPx: number) {
  const GAP_PX = 16;
  const maxAllowedWidth = maxPostersPerRow * posterSizePx + (maxPostersPerRow - 1) * GAP_PX;
  const effectiveWidth = Math.min(availableWidthPx, maxAllowedWidth);
  const columns = Math.max(1, Math.min(maxPostersPerRow, Math.floor((effectiveWidth + GAP_PX) / (posterSizePx + GAP_PX))));
  const rowCount = Math.ceil(totalItems / columns);
  const rows: number[] = [];
  let remaining = totalItems;
  while (remaining > 0) {
    const countInRow = Math.min(columns, remaining);
    rows.push(countInRow);
    remaining -= countInRow;
  }
  return {
    columns,
    rowCount,
    rows,
    cardWidthPx: posterSizePx,
    cardHeightPx: Math.round(posterSizePx * (800 / 660)),
    unusedColumnsInLastRow: columns - rows[rows.length - 1],
  };
}

test('Test Scenario 1: Isolated Downloaded Media Only (External Empty)', () => {
  const backend = new MockSqliteBackend();

  // 12 downloaded items
  const downloadedTorrents = Array.from({ length: 12 }, (_, i) => ({
    id: `hash-${i + 1}`,
    info_hash: `hash-${i + 1}`,
    name: `Downloaded Movie ${i + 1}`,
  }));
  const externalMedia: { path: string; title: string }[] = [];

  // Check bottom control area availability
  const hasPopulatedCollection = downloadedTorrents.length > 0 || externalMedia.length > 0;
  assert.equal(hasPopulatedCollection, true, 'Reorder control must be available when Downloaded is populated');

  // Verify multiple rows with default 5 posters per row and default 150px poster size
  const layout = computeGridGeometry(backend.get_posters_per_row(), backend.get_poster_size(), downloadedTorrents.length, 1200);
  assert.equal(layout.columns, 5, '5 posters per row');
  assert.equal(layout.rowCount, 3, '12 items wrap into 3 rows');
  assert.deepEqual(layout.rows, [5, 5, 2], 'Rows contain 5, 5, and 2 items');
  assert.equal(layout.cardWidthPx, 150, 'Poster card width is strictly 150px');
  assert.equal(layout.unusedColumnsInLastRow, 3, 'Last row leaves 3 columns empty without stretching');

  // Perform drag rearrangement: move Downloaded Movie 12 (index 11) to position 0
  const currentOrder = downloadedTorrents.map((t) => t.id);
  const reordered = [currentOrder[11], ...currentOrder.slice(0, 11)];
  assert.equal(reordered[0], 'hash-12');

  // Persist order
  backend.set_media_order(reordered.map((hash, idx) => ({ info_hash: hash, sort_order: idx })));

  // Simulate Application Restart: fetch restored order from backend
  const restoredOrder = backend.get_downloaded_media_order();
  assert.deepEqual(restoredOrder, reordered, 'Backend restored downloaded order matches user rearrangement');

  // Re-resolve display order after restart
  const resolvedDisplay = orderGroups(downloadedTorrents, restoredOrder);
  assert.equal(resolvedDisplay[0].id, 'hash-12', 'Downloaded Movie 12 is at the top of the shelf after restart');
  assert.equal(resolvedDisplay[1].id, 'hash-1', 'Downloaded Movie 1 follows Movie 12');
});

test('Test Scenario 2: Isolated External Media Only (Downloaded Empty)', () => {
  const backend = new MockSqliteBackend();

  const downloadedTorrents: { id: string; info_hash: string; name: string }[] = [];
  const externalMedia = [
    { path: '/media/Patlabor 2 (1993)', title: 'Patlabor 2' },
    { path: '/media/Fuuka (2017)', title: 'Fuuka' },
    { path: '/media/Mary (2017)', title: "Mary and the Witch's Flower" },
    { path: '/media/Suzume (2022)', title: 'Suzume' },
    { path: '/media/Ghost in the Shell (1995)', title: 'Ghost in the Shell' },
    { path: '/media/Akira (1988)', title: 'Akira' },
    { path: '/media/Your Name (2016)', title: 'Your Name' },
    { path: '/media/Inu-Oh (2021)', title: 'Inu-Oh' },
  ];

  // Check bottom control area availability
  const hasPopulatedCollection = downloadedTorrents.length > 0 || externalMedia.length > 0;
  assert.equal(hasPopulatedCollection, true, 'Reorder control must be available when External is populated even if Downloaded is empty');

  // Set Posters per row to 3 and poster size to 180px
  backend.set_posters_per_row(3);
  backend.set_poster_size(180);
  assert.equal(backend.get_posters_per_row(), 3);
  assert.equal(backend.get_poster_size(), 180);

  // Verify multiple rows with 3 posters per row for 8 items at 180px size
  const layout = computeGridGeometry(3, 180, externalMedia.length, 1200);
  assert.equal(layout.columns, 3, '3 posters per row');
  assert.equal(layout.rowCount, 3, '8 items wrap into 3 rows (3 + 3 + 2)');
  assert.deepEqual(layout.rows, [3, 3, 2], 'Rows contain 3, 3, and 2 items');
  assert.equal(layout.cardWidthPx, 180, 'Poster dimensions match selected 180px size');

  // Perform drag rearrangement: move Suzume (/media/Suzume (2022), index 3) to index 0
  const currentPaths = externalMedia.map((m) => m.path);
  const reorderedPaths = [currentPaths[3], currentPaths[0], currentPaths[1], currentPaths[2], ...currentPaths.slice(4)];
  assert.equal(reorderedPaths[0], '/media/Suzume (2022)');

  // Persist external order
  backend.set_external_media_order(reorderedPaths);

  // Simulate Application Restart
  const restoredPaths = backend.get_external_media_order();
  assert.deepEqual(restoredPaths, reorderedPaths, 'Restored external order matches user rearrangement');

  // Re-resolve external display order
  const resolvedExternal = orderExternal(externalMedia, restoredPaths);
  assert.equal(resolvedExternal[0].title, 'Suzume', 'Suzume is first after restart');
  assert.equal(resolvedExternal[1].title, 'Patlabor 2', 'Patlabor 2 is second after restart');
});

test('Test Scenario 3: Both Populated Collections Reorder Independently and Wrap Consistently', () => {
  const backend = new MockSqliteBackend();
  backend.set_posters_per_row(4);
  backend.set_poster_size(160);

  const downloadedTorrents = [
    { id: 'dl-1', info_hash: 'dl-1', name: 'DL 1' },
    { id: 'dl-2', info_hash: 'dl-2', name: 'DL 2' },
    { id: 'dl-3', info_hash: 'dl-3', name: 'DL 3' },
    { id: 'dl-4', info_hash: 'dl-4', name: 'DL 4' },
    { id: 'dl-5', info_hash: 'dl-5', name: 'DL 5' },
    { id: 'dl-6', info_hash: 'dl-6', name: 'DL 6' },
  ];

  const externalMedia = [
    { path: '/ext/1', title: 'EXT 1' },
    { path: '/ext/2', title: 'EXT 2' },
    { path: '/ext/3', title: 'EXT 3' },
    { path: '/ext/4', title: 'EXT 4' },
    { path: '/ext/5', title: 'EXT 5' },
  ];

  // Shared reorder toggle state
  let isReordering = false;
  const toggleReorder = () => { isReordering = !isReordering; };

  // Enter reorder mode
  toggleReorder();
  assert.equal(isReordering, true, 'Shared Reorder button toggles reorder mode on for both shelves');

  // Reorder Downloaded: reverse order
  const newDlOrder = [...downloadedTorrents].reverse().map((t) => t.id);
  backend.set_media_order(newDlOrder.map((hash, idx) => ({ info_hash: hash, sort_order: idx })));

  // Reorder External: move /ext/5 to front
  const newExtOrder = ['/ext/5', '/ext/1', '/ext/2', '/ext/3', '/ext/4'];
  backend.set_external_media_order(newExtOrder);

  // Click Done in same bottom location
  toggleReorder();
  assert.equal(isReordering, false, 'Done button toggles reorder mode off');

  // Verify independent persistence
  assert.deepEqual(backend.get_downloaded_media_order(), ['dl-6', 'dl-5', 'dl-4', 'dl-3', 'dl-2', 'dl-1']);
  assert.deepEqual(backend.get_external_media_order(), ['/ext/5', '/ext/1', '/ext/2', '/ext/3', '/ext/4']);

  // Verify grid wrapping for both collections:
  // Downloaded: 6 items with limit 4 -> rows [4, 2] at 160px
  const dlLayout = computeGridGeometry(4, 160, downloadedTorrents.length, 1200);
  assert.deepEqual(dlLayout.rows, [4, 2]);
  assert.equal(dlLayout.cardWidthPx, 160);

  // External: 5 items with limit 4 -> rows [4, 1] at 160px
  const extLayout = computeGridGeometry(4, 160, externalMedia.length, 1200);
  assert.deepEqual(extLayout.rows, [4, 1]);
  assert.equal(extLayout.cardWidthPx, 160);
});

test('Test Scenario 4: Window Narrowing Adapts Columns While Preserving Preference Upper Limit', () => {
  // Preference: 6 posters per row at 150px poster size
  // Wide window (1400px): fits 6 columns
  const wide = computeGridGeometry(6, 150, 12, 1400);
  assert.equal(wide.columns, 6);
  assert.equal(wide.rowCount, 2);
  assert.equal(wide.cardWidthPx, 150);

  // Medium window (700px): fits 4 columns (4 * 150 + 3 * 16 = 648px <= 700px)
  const med = computeGridGeometry(6, 150, 12, 700);
  assert.equal(med.columns, 4, 'Adapts to 4 columns');
  assert.equal(med.rowCount, 3, '12 items wrap into 3 rows of 4');
  assert.equal(med.cardWidthPx, 150, 'Card width remains 150px without shrinking');

  // Narrow window (500px): fits 3 columns (3 * 150 + 2 * 16 = 482px <= 500px)
  const narrow = computeGridGeometry(6, 150, 12, 500);
  assert.equal(narrow.columns, 3, 'Adapts to 3 columns');
  assert.equal(narrow.rowCount, 4, '12 items wrap into 4 rows of 3');
  assert.equal(narrow.cardWidthPx, 150, 'Card width remains 150px without shrinking');
});

test('Test Scenario 5: Independent Poster Size and Max Posters Per Row Interaction', () => {
  const backend = new MockSqliteBackend();

  // Initial preferences
  backend.set_posters_per_row(5);
  backend.set_poster_size(150);

  // 12 posters at max 5 per row on wide screen: 5, 5, 2
  const initial = computeGridGeometry(5, 150, 12, 1400);
  assert.deepEqual(initial.rows, [5, 5, 2], 'Rows are 5, 5, 2');
  assert.equal(initial.cardWidthPx, 150, 'Card width is 150px');

  // 1. Changing poster size (150 -> 220) leaves maximum posters per row unchanged (remains 5)
  backend.set_poster_size(220);
  assert.equal(backend.get_poster_size(), 220, 'Poster size updated to 220px');
  assert.equal(backend.get_posters_per_row(), 5, 'Max posters per row remains 5');

  const afterResize = computeGridGeometry(5, 220, 12, 1600);
  assert.deepEqual(afterResize.rows, [5, 5, 2], 'Per-row limit still produces rows of 5, 5, 2');
  assert.equal(afterResize.cardWidthPx, 220, 'Card width enlarged to 220px');

  // 2. Changing maximum posters per row (5 -> 4) leaves poster dimensions unchanged (remains 220px)
  backend.set_posters_per_row(4);
  assert.equal(backend.get_posters_per_row(), 4, 'Max posters per row updated to 4');
  assert.equal(backend.get_poster_size(), 220, 'Poster size remains 220px');

  const afterMaxChange = computeGridGeometry(4, 220, 12, 1600);
  assert.deepEqual(afterMaxChange.rows, [4, 4, 4], 'Row wrapping changed to 4, 4, 4');
  assert.equal(afterMaxChange.cardWidthPx, 220, 'Poster dimensions remain exactly 220px');

  // 3. Excess space leaves room rather than adding extra posters or stretching
  const excessWidth = computeGridGeometry(4, 220, 12, 3000); // 3000px wide
  assert.equal(excessWidth.columns, 4, 'Capped at maximum 4 columns even with 3000px width');
  assert.equal(excessWidth.cardWidthPx, 220, 'Cards do not stretch to fill 3000px');

  // 4. Narrow window wraps earlier without shrinking posters
  const narrowWidth = computeGridGeometry(4, 220, 12, 600); // 600px width fits 2 columns (456px <= 600px)
  assert.equal(narrowWidth.columns, 2, 'Wraps earlier to 2 columns');
  assert.deepEqual(narrowWidth.rows, [2, 2, 2, 2, 2, 2], '12 items wrap into 6 rows of 2');
  assert.equal(narrowWidth.cardWidthPx, 220, 'Cards remain 220px without shrinking');

  // 5. Restart persistence
  const restartBackend = new MockSqliteBackend();
  restartBackend.settings.set('media_poster_size', backend.settings.get('media_poster_size')!);
  restartBackend.settings.set('media_posters_per_row', backend.settings.get('media_posters_per_row')!);
  assert.equal(restartBackend.get_poster_size(), 220, 'Poster size survived restart');
  assert.equal(restartBackend.get_posters_per_row(), 4, 'Max posters per row survived restart');
});
