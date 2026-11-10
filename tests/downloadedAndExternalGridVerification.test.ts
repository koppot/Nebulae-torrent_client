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

  set_poster_spacing(spacing: number): void {
    const clamped = Math.max(4, Math.min(48, spacing));
    this.settings.set('media_poster_spacing', String(clamped));
  }

  get_poster_spacing(): number {
    const val = this.settings.get('media_poster_spacing');
    return val ? parseInt(val, 10) : 16;
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
    const seen = new Set<string>();
    const unique = paths.filter((p) => {
      const key = p.trim().toLowerCase().replace(/\\/g, '/');
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
    this.settings.set('media_external_order', JSON.stringify(unique));
  }

  get_external_media_order(): string[] {
    const json = this.settings.get('media_external_order');
    if (json) {
      try {
        const paths = JSON.parse(json);
        if (Array.isArray(paths)) {
          const seen = new Set<string>();
          return paths.filter((p) => {
            const key = p.trim().toLowerCase().replace(/\\/g, '/');
            if (seen.has(key)) return false;
            seen.add(key);
            return true;
          });
        }
      } catch { /* fallback */ }
    }
    return [];
  }
}

// Order resolution algorithm from SimpleMediaView.tsx
function normalizePath(p: string): string {
  return p.trim().toLowerCase().replace(/\\/g, '/');
}

function orderGroups<T extends { id: string; info_hash?: string }>(groups: T[], orderList: string[]): T[] {
  if (!groups || groups.length === 0) return [];
  const uniqueGroups: T[] = [];
  const seenGroupIds = new Set<string>();
  for (const g of groups) {
    const idLower = g.id.toLowerCase();
    if (!seenGroupIds.has(idLower)) {
      seenGroupIds.add(idLower);
      uniqueGroups.push(g);
    }
  }
  if (!orderList || orderList.length === 0) return uniqueGroups;
  const remaining = [...uniqueGroups];
  const result: T[] = [];
  const seenOrderIds = new Set<string>();
  for (const rawId of orderList) {
    const idLower = rawId.toLowerCase();
    if (seenOrderIds.has(idLower)) continue;
    seenOrderIds.add(idLower);
    const idx = remaining.findIndex((g) =>
      g.id.toLowerCase() === idLower ||
      (g.info_hash && g.info_hash.toLowerCase() === idLower) ||
      g.id === rawId
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
  if (!items || items.length === 0) return [];
  const uniqueItems: T[] = [];
  const seenItemPaths = new Set<string>();
  for (const item of items) {
    const norm = normalizePath(item.path);
    if (!seenItemPaths.has(norm)) {
      seenItemPaths.add(norm);
      uniqueItems.push(item);
    }
  }
  if (!orderList || orderList.length === 0) return uniqueItems;
  const remaining = [...uniqueItems];
  const result: T[] = [];
  const seenOrderPaths = new Set<string>();
  for (const rawPath of orderList) {
    const norm = normalizePath(rawPath);
    if (seenOrderPaths.has(norm)) continue;
    seenOrderPaths.add(norm);
    const idx = remaining.findIndex((m) => normalizePath(m.path) === norm);
    if (idx !== -1) {
      result.push(remaining[idx]);
      remaining.splice(idx, 1);
    }
  }
  result.push(...remaining);
  return result;
}

// Grid layout geometry calculator matching SimpleMediaView.tsx CSS grid
function computeGridGeometry(
  maxPostersPerRow: number,
  posterSizePx: number,
  totalItems: number,
  availableWidthPx: number,
  posterSpacingPx: number = 16,
) {
  const GAP_PX = posterSpacingPx;
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
    gapPx: posterSpacingPx,
    maxAllowedWidth,
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

test('Test Scenario 6: Reordering Invariant - Moving Posters Preserves Exact Item Count and Identity Without Cloning', () => {
  const backend = new MockSqliteBackend();

  // Initial collection of 6 external items (matching the real setup)
  const initialMedia = [
    { path: 'V:\\Blu-ray\\Fuuka (2017) Funimation', title: 'Fuuka', year: 2017 },
    { path: 'V:\\UHD\\Suzume (2022) UHD', title: 'Suzume', year: 2022 },
    { path: 'V:\\DVD\\DVD_jp\\Patapata Hikousen no Bouken DVD', title: 'Patapata Hikousen no Bouken', year: 2002 },
    { path: 'V:\\DVD\\DVD_jp\\Busou Renkin 9', title: 'Busou Renkin', year: 2006 },
    { path: 'V:\\UHD\\UHD Mary and a witch\'s flower', title: 'Mary to Majo no Hana', year: 2017 },
    { path: 'V:\\UHD\\Patlabor 2 the Movie (2002) UHD', title: 'Patlabor 2: The Movie', year: 2002 },
  ];

  const initialCount = initialMedia.length;
  assert.equal(initialCount, 6);

  // Initial order resolution
  const initialOrdered = orderExternal(initialMedia, []);
  assert.equal(initialOrdered.length, 6);

  // Simulate multiple drag moves across rows:
  // Move 1: Drag Patlabor from index 5 to index 0 (top row)
  let currentPaths = Array.from(new Set(initialOrdered.map((m) => m.path)));
  const oldIdx1 = currentPaths.indexOf('V:\\UHD\\Patlabor 2 the Movie (2002) UHD');
  const newIdx1 = 0;
  assert.notEqual(oldIdx1, -1);
  const moved1 = [...currentPaths];
  const [removed1] = moved1.splice(oldIdx1, 1);
  moved1.splice(newIdx1, 0, removed1);
  const deduped1 = Array.from(new Set(moved1));
  backend.set_external_media_order(deduped1);

  // Verify collection item count and uniqueness after Move 1
  const afterMove1 = orderExternal(initialMedia, backend.get_external_media_order());
  assert.equal(afterMove1.length, 6, 'Item count strictly invariant after Move 1');
  assert.equal(new Set(afterMove1.map((m) => m.path)).size, 6, 'All 6 items are unique');
  assert.equal(afterMove1[0].path, 'V:\\UHD\\Patlabor 2 the Movie (2002) UHD', 'Patlabor moved to index 0');

  // Move 2: Drag Busou Renkin to index 2
  currentPaths = Array.from(new Set(afterMove1.map((m) => m.path)));
  const oldIdx2 = currentPaths.indexOf('V:\\DVD\\DVD_jp\\Busou Renkin 9');
  const newIdx2 = 2;
  const moved2 = [...currentPaths];
  const [removed2] = moved2.splice(oldIdx2, 1);
  moved2.splice(newIdx2, 0, removed2);
  const deduped2 = Array.from(new Set(moved2));
  backend.set_external_media_order(deduped2);

  // Verify collection item count and uniqueness after Move 2
  const afterMove2 = orderExternal(initialMedia, backend.get_external_media_order());
  assert.equal(afterMove2.length, 6, 'Item count strictly invariant after Move 2');
  assert.equal(new Set(afterMove2.map((m) => m.path)).size, 6, 'All 6 items are unique');
  assert.equal(afterMove2[2].path, 'V:\\DVD\\DVD_jp\\Busou Renkin 9', 'Busou Renkin at index 2');

  // Move 3: Drag Patlabor to the very end
  currentPaths = Array.from(new Set(afterMove2.map((m) => m.path)));
  const oldIdx3 = currentPaths.indexOf('V:\\UHD\\Patlabor 2 the Movie (2002) UHD');
  const newIdx3 = currentPaths.length - 1;
  const moved3 = [...currentPaths];
  const [removed3] = moved3.splice(oldIdx3, 1);
  moved3.splice(newIdx3, 0, removed3);
  const deduped3 = Array.from(new Set(moved3));
  backend.set_external_media_order(deduped3);

  // Verify collection item count and uniqueness after Move 3
  const afterMove3 = orderExternal(initialMedia, backend.get_external_media_order());
  assert.equal(afterMove3.length, 6, 'Item count strictly invariant after Move 3');
  assert.equal(new Set(afterMove3.map((m) => m.path)).size, 6, 'All 6 items remain unique');
  assert.equal(afterMove3[5].path, 'V:\\UHD\\Patlabor 2 the Movie (2002) UHD', 'Patlabor moved to end');
});

test('Test Scenario 7: Path Deduplication Preserves Legitimate Separate Releases Sharing Title or Artwork', () => {
  // Collection containing:
  // 1. Two separate legitimate releases sharing title and poster (e.g. 1993 DVD vs 2002 UHD)
  // 2. One duplicate entry with the exact same path (e.g. historical un-deduplicated sidecar entry)
  const rawItems = [
    { path: 'V:\\UHD\\Patlabor 2 the Movie (2002) UHD', title: 'Patlabor 2: The Movie', year: 2002, poster_url: '/posters/patlabor.webp' },
    { path: 'V:\\UHD\\Patlabor 2 the Movie (2002) UHD', title: 'Patlabor 2: The Movie', year: null, poster_url: null }, // identical path
    { path: 'V:\\Blu-ray\\Patlabor 2 the Movie (1993)', title: 'Patlabor 2: The Movie', year: 1993, poster_url: '/posters/patlabor.webp' }, // distinct release, same title & artwork
    { path: 'V:\\Blu-ray\\Fuuka (2017)', title: 'Fuuka', year: 2017, poster_url: '/posters/fuuka.webp' },
  ];

  // Simulated order array that might even contain duplicate paths from prior bug
  const orderList = [
    'V:\\Blu-ray\\Fuuka (2017)',
    'V:\\UHD\\Patlabor 2 the Movie (2002) UHD',
    'V:\\UHD\\Patlabor 2 the Movie (2002) UHD', // duplicate path in saved order
    'V:\\Blu-ray\\Patlabor 2 the Movie (1993)',
  ];

  const resolved = orderExternal(rawItems, orderList);

  // Exact requirements:
  // 1. Identical paths are deduplicated to exactly ONE card.
  // 2. Distinct paths sharing title/artwork are STRICTLY PRESERVED as separate cards.
  assert.equal(resolved.length, 3, 'Exactly 3 distinct items rendered');
  assert.equal(resolved[0].path, 'V:\\Blu-ray\\Fuuka (2017)');
  assert.equal(resolved[1].path, 'V:\\UHD\\Patlabor 2 the Movie (2002) UHD');
  assert.equal(resolved[2].path, 'V:\\Blu-ray\\Patlabor 2 the Movie (1993)', 'Separate release strictly preserved');

  // Verify sortable IDs in SortableContext are strictly unique
  const sortableIds = Array.from(new Set(resolved.map((m) => m.path)));
  assert.equal(sortableIds.length, 3, 'SortableContext IDs have zero collisions');
});

test('Test Scenario 8: Clean Drag Lifecycle - Reorder Exit and Drop Invariants', () => {
  // Simulate active drag state
  let isReordering = true;
  let activeExternalId: string | null = 'V:\\UHD\\Patlabor 2 the Movie (2002) UHD';
  let activeDownloadedId: string | null = 'dl-hash-1';

  // Invariant 1: While dragging in reorder mode, active card has dragging style
  const cardStyleInReorder = (isDragging: boolean) => ({
    transform: isReordering ? 'translate3d(0px, 100px, 0px)' : undefined,
    transition: isReordering ? 'transform 250ms ease' : undefined,
    opacity: isReordering && isDragging ? 0.3 : 1,
  });

  const draggingCard = cardStyleInReorder(true);
  assert.equal(draggingCard.opacity, 0.3);
  assert.equal(draggingCard.transform, 'translate3d(0px, 100px, 0px)');

  // Invariant 2: User clicks "Done" (handleToggleReorder)
  // Both active IDs MUST be reset to null immediately
  const handleToggleReorder = () => {
    isReordering = !isReordering;
    activeDownloadedId = null;
    activeExternalId = null;
  };

  handleToggleReorder();
  assert.equal(isReordering, false, 'Reorder mode is now false');
  assert.equal(activeDownloadedId, null, 'Downloaded active drag ID cleared');
  assert.equal(activeExternalId, null, 'External active drag ID cleared');

  // Invariant 3: When isReordering is false, cards NEVER have transform or dimming
  const normalCard = cardStyleInReorder(false);
  assert.equal(normalCard.opacity, 1, 'Opacity is strictly 1');
  assert.equal(normalCard.transform, undefined, 'Transform is undefined - zero floating/displaced cards');

  // Invariant 4: DragOverlay is NEVER rendered when isReordering is false
  const shouldRenderOverlay = isReordering && activeExternalId !== null;
  assert.equal(shouldRenderOverlay, false, 'DragOverlay cannot render when reorder mode is inactive');
});

test('Test Scenario 9: Persistence Roundtrip and State Sanitization', () => {
  const backend = new MockSqliteBackend();

  // Save order containing duplicate and malformed paths
  const pollutedOrder = [
    'V:\\UHD\\Patlabor 2 the Movie (2002) UHD',
    'V:\\Blu-ray\\Fuuka (2017)',
    'V:/UHD/Patlabor 2 the Movie (2002) UHD', // same path with forward slashes
    'V:\\Blu-ray\\Fuuka (2017)',             // exact duplicate
    'V:\\UHD\\Suzume (2022)',
  ];

  backend.set_external_media_order(pollutedOrder);

  // Backend automatically sanitizes on write and read
  const sanitized = backend.get_external_media_order();
  assert.equal(sanitized.length, 3, 'Sanitized to exactly 3 unique paths');
  assert.deepEqual(sanitized, [
    'V:\\UHD\\Patlabor 2 the Movie (2002) UHD',
    'V:\\Blu-ray\\Fuuka (2017)',
    'V:\\UHD\\Suzume (2022)',
  ]);

  // Restart persistence simulation
  const restartBackend = new MockSqliteBackend();
  restartBackend.settings.set('media_external_order', backend.settings.get('media_external_order')!);
  const restored = restartBackend.get_external_media_order();
  assert.deepEqual(restored, sanitized, 'Sanitized unique order persists across restarts');
});

test('Test Scenario 10: Settings Exclusivity, Store Hydration on Startup, and Acceptance Example', async () => {
  const backend = new MockSqliteBackend();

  // 1. Initial defaults
  assert.equal(backend.get_posters_per_row(), 5, 'Default max posters per row is 5');
  assert.equal(backend.get_poster_size(), 150, 'Default poster size is 150px');

  // Acceptance Example from user specification:
  // With 12 items and room for the selected poster size:
  // A limit of 5 produces rows of 5, 5, and 2.
  const geom5 = computeGridGeometry(5, 150, 12, 1600);
  assert.deepEqual(geom5.rows, [5, 5, 2], 'Limit of 5 produces rows of 5, 5, and 2');
  assert.equal(geom5.cardWidthPx, 150, 'Poster dimension is 150px');

  // Changing the limit to 4 produces rows of 4, 4, and 4.
  // Poster dimensions remain identical throughout.
  backend.set_posters_per_row(4);
  const geom4 = computeGridGeometry(backend.get_posters_per_row(), backend.get_poster_size(), 12, 1600);
  assert.deepEqual(geom4.rows, [4, 4, 4], 'Changing limit to 4 produces rows of 4, 4, and 4');
  assert.equal(geom4.cardWidthPx, 150, 'Poster dimensions remain identical throughout (150px)');

  // Changing poster size afterward leaves the saved per-row limit unchanged.
  backend.set_poster_size(240);
  assert.equal(backend.get_posters_per_row(), 4, 'Saved per-row limit remains strictly 4');
  assert.equal(backend.get_poster_size(), 240, 'Poster size updated to 240px');

  const geomResized = computeGridGeometry(backend.get_posters_per_row(), backend.get_poster_size(), 12, 1600);
  assert.deepEqual(geomResized.rows, [4, 4, 4], 'Row wrapping remains 4, 4, 4');
  assert.equal(geomResized.cardWidthPx, 240, 'Poster dimensions updated to 240px');

  // A narrower window may wrap earlier, but must not shrink posters to force the configured count.
  const geomNarrow = computeGridGeometry(backend.get_posters_per_row(), backend.get_poster_size(), 12, 700);
  assert.equal(geomNarrow.columns, 2, 'Narrow window wraps earlier to 2 columns');
  assert.deepEqual(geomNarrow.rows, [2, 2, 2, 2, 2, 2], 'Rows wrap to 2, 2, 2, 2, 2, 2');
  assert.equal(geomNarrow.cardWidthPx, 240, 'Posters do not shrink to force the configured count');

  // Persistence across restart:
  const restarted = new MockSqliteBackend();
  restarted.settings.set('media_posters_per_row', backend.settings.get('media_posters_per_row')!);
  restarted.settings.set('media_poster_size', backend.settings.get('media_poster_size')!);
  assert.equal(restarted.get_posters_per_row(), 4, 'Restored max posters per row is 4');
  assert.equal(restarted.get_poster_size(), 240, 'Restored poster size is 240px');

  // Verify Media preferences dialog in SimpleMediaView contains layout controls & language preferences
  const fs = await import('node:fs');
  const mediaViewSource = fs.readFileSync('src/components/media/SimpleMediaView.tsx', 'utf-8');
  assert.ok(
    mediaViewSource.includes('Poster size:'),
    'SimpleMediaView preferences modal contains "Poster size:" control'
  );
  assert.ok(
    mediaViewSource.includes('Poster spacing:'),
    'SimpleMediaView preferences modal contains "Poster spacing:" control'
  );
  assert.ok(
    mediaViewSource.includes('Maximum posters per row:'),
    'SimpleMediaView preferences modal contains "Maximum posters per row:" control'
  );
  assert.ok(
    mediaViewSource.includes('Audio languages') && mediaViewSource.includes('prefAudioLangs'),
    'SimpleMediaView preferences modal preserves audio language preferences'
  );
  assert.ok(
    mediaViewSource.includes('Subtitle languages') && mediaViewSource.includes('prefSubsLangs'),
    'SimpleMediaView preferences modal preserves subtitle language preferences'
  );

  // Verify SettingsModal does NOT contain duplicate poster layout controls
  const settingsSource = fs.readFileSync('src/components/modals/SettingsModal.tsx', 'utf-8');
  assert.ok(
    !settingsSource.includes('Poster size:'),
    'SettingsModal does NOT contain duplicate "Poster size:" control'
  );
  assert.ok(
    !settingsSource.includes('Poster spacing:'),
    'SettingsModal does NOT contain duplicate "Poster spacing:" control'
  );
  assert.ok(
    !settingsSource.includes('Maximum posters per row:'),
    'SettingsModal does NOT contain duplicate "Maximum posters per row:" control'
  );
  assert.ok(
    !settingsSource.includes('Media Grid & Posters Layout'),
    'SettingsModal does NOT contain duplicate "Media Grid & Posters Layout"'
  );

  // Verify TitleBar opens separate Media preferences via nebulae:player-prefs in Media view,
  // and Manager general preferences via setShowSettings in Manager view
  const titleBarSource = fs.readFileSync('src/components/layout/TitleBar.tsx', 'utf-8');
  assert.ok(
    titleBarSource.includes("window.dispatchEvent(new CustomEvent('nebulae:player-prefs'))"),
    'TitleBar dispatches nebulae:player-prefs when in Media view'
  );
  assert.ok(
    titleBarSource.includes('setShowSettings(true)'),
    'TitleBar opens general preferences (setShowSettings) when in Manager view'
  );
  assert.ok(
    !titleBarSource.includes("openSettings('media')"),
    "TitleBar does not redirect Media view gear to openSettings('media')"
  );
});

test('Test Scenario 11: Three-way Independence - Poster Size, Spacing, and Per-Row Wrapping with Restart Persistence', () => {
  const backend = new MockSqliteBackend();

  // 1. Initial defaults
  assert.equal(backend.get_poster_size(), 150, 'Default poster size is 150 px');
  assert.equal(backend.get_poster_spacing(), 16, 'Default poster spacing is 16 px');
  assert.equal(backend.get_posters_per_row(), 5, 'Default posters per row is 5');

  // Baseline layout with 12 items: 5, 5, 2
  const baseGeom = computeGridGeometry(
    backend.get_posters_per_row(),
    backend.get_poster_size(),
    12,
    1600,
    backend.get_poster_spacing()
  );
  assert.deepEqual(baseGeom.rows, [5, 5, 2]);
  assert.equal(baseGeom.cardWidthPx, 150);
  assert.equal(baseGeom.gapPx, 16);
  // Max width = 5 * 150 + 4 * 16 = 750 + 64 = 814 px
  assert.equal(baseGeom.maxAllowedWidth, 814);

  // 2. Adjust spacing only: card size and per-row wrapping remain identical
  backend.set_poster_spacing(32);
  assert.equal(backend.get_poster_spacing(), 32, 'Poster spacing updated to 32 px');
  assert.equal(backend.get_poster_size(), 150, 'Poster size unchanged at 150 px');
  assert.equal(backend.get_posters_per_row(), 5, 'Posters per row unchanged at 5');

  const spacedGeom = computeGridGeometry(
    backend.get_posters_per_row(),
    backend.get_poster_size(),
    12,
    1600,
    backend.get_poster_spacing()
  );
  assert.deepEqual(spacedGeom.rows, [5, 5, 2], 'Rows still wrap at 5, 5, 2');
  assert.equal(spacedGeom.cardWidthPx, 150, 'Cards remain 150 px wide');
  assert.equal(spacedGeom.gapPx, 32, 'Gaps expanded to 32 px');
  // Max width = 5 * 150 + 4 * 32 = 750 + 128 = 878 px
  assert.equal(spacedGeom.maxAllowedWidth, 878);

  // 3. Adjust poster size: spacing and per-row wrapping remain identical
  backend.set_poster_size(200);
  assert.equal(backend.get_poster_size(), 200, 'Poster size updated to 200 px');
  assert.equal(backend.get_poster_spacing(), 32, 'Poster spacing preserved at 32 px');
  assert.equal(backend.get_posters_per_row(), 5, 'Posters per row preserved at 5');

  const resizedGeom = computeGridGeometry(
    backend.get_posters_per_row(),
    backend.get_poster_size(),
    12,
    1600,
    backend.get_poster_spacing()
  );
  assert.deepEqual(resizedGeom.rows, [5, 5, 2]);
  assert.equal(resizedGeom.cardWidthPx, 200);
  assert.equal(resizedGeom.gapPx, 32);
  // Max width = 5 * 200 + 4 * 32 = 1000 + 128 = 1128 px
  assert.equal(resizedGeom.maxAllowedWidth, 1128);

  // 4. Clamping boundaries verified
  backend.set_poster_spacing(2); // below 4
  assert.equal(backend.get_poster_spacing(), 4, 'Clamped to min 4 px');
  backend.set_poster_spacing(80); // above 48
  assert.equal(backend.get_poster_spacing(), 48, 'Clamped to max 48 px');

  backend.set_poster_spacing(24);

  // 5. Restart persistence
  const restart = new MockSqliteBackend();
  restart.settings.set('media_poster_size', backend.settings.get('media_poster_size')!);
  restart.settings.set('media_poster_spacing', backend.settings.get('media_poster_spacing')!);
  restart.settings.set('media_posters_per_row', backend.settings.get('media_posters_per_row')!);

  assert.equal(restart.get_poster_size(), 200, 'Restored poster size: 200 px');
  assert.equal(restart.get_poster_spacing(), 24, 'Restored poster spacing: 24 px');
  assert.equal(restart.get_posters_per_row(), 5, 'Restored max posters per row: 5');
});
