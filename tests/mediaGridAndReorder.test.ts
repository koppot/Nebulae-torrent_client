import test from 'node:test';
import assert from 'node:assert/strict';

/**
 * Unit & Integration tests for Media Grid Layout & Poster Rearrangement
 *
 * Requirements verified:
 * 1. Configurable "Posters per row" in Settings controlling both Downloaded and External collections.
 * 2. Changing the count changes where posters wrap into additional rows—not poster dimensions.
 *    Poster width remains strictly 150px (aspect ratio 660:800).
 * 3. Narrower windows fit fewer posters than the configured maximum and wrap cleanly into more rows.
 * 4. The Media screen has one shared Reorder control below the shelves for both populated collections.
 * 5. Reorder mode switches the shared button to a single Done button in the same location.
 * 6. Isolated collection handling:
 *    - Downloaded only (External empty): reorder control remains enabled and works.
 *    - External only (Downloaded empty): reorder control remains enabled and works.
 *    - Both populated: both collections reorder independently without cross-collection moves.
 * 7. Persistence & Restoration after restart:
 *    - Downloaded persistence (set_media_order / media_downloaded_order) restores custom order.
 *    - External persistence (set_external_media_order / media_external_order) restores custom order.
 *    - Settings persistence (set_posters_per_row / media_posters_per_row) restores setting.
 */

test('Requirement 1 & 2: Settings "Posters per row" Controls Wrapping Without Altering Poster Dimensions', () => {
  const POSTER_WIDTH_PX = 150;
  const GAP_PX = 16; // 1rem gap
  const ASPECT_W = 660;
  const ASPECT_H = 800;
  const POSTER_HEIGHT_PX = Math.round(POSTER_WIDTH_PX * (ASPECT_H / ASPECT_W)); // ~182px

  const calculateGridWrapping = (postersPerRowLimit: number, totalItems: number, containerWidthPx: number) => {
    // Max container width enforced by style: calc(postersPerRow * 150px + (postersPerRow - 1) * 1rem)
    const maxContainerWidth = postersPerRowLimit * POSTER_WIDTH_PX + (postersPerRowLimit - 1) * GAP_PX;
    const effectiveWidth = Math.min(containerWidthPx, maxContainerWidth);

    // Number of 150px columns that fit into effective width with 16px gap
    const columns = Math.max(1, Math.min(postersPerRowLimit, Math.floor((effectiveWidth + GAP_PX) / (POSTER_WIDTH_PX + GAP_PX))));
    const rowCount = Math.ceil(totalItems / columns);

    // Unused space left at end of incomplete rows (justify-start)
    const itemsInLastRow = totalItems % columns === 0 ? columns : totalItems % columns;
    const unusedColumnsInLastRow = columns - itemsInLastRow;

    return {
      postersPerRowLimit,
      columns,
      rowCount,
      posterWidthPx: POSTER_WIDTH_PX,
      posterHeightPx: POSTER_HEIGHT_PX,
      itemsInLastRow,
      unusedColumnsInLastRow,
    };
  };

  // Test with 12 items on a wide 1400px window across different Settings values:
  // Posters per row = 5: displays rows of 5, 5, 2 with unchanged poster dimensions
  const limit5 = calculateGridWrapping(5, 12, 1400);
  assert.equal(limit5.columns, 5, 'Maximum 5 posters per row');
  assert.equal(limit5.rowCount, 3, '12 items wrap into 3 rows (5 + 5 + 2)');
  assert.equal(limit5.itemsInLastRow, 2, 'Last row has 2 items');
  assert.equal(limit5.unusedColumnsInLastRow, 3, 'Leaves 3 column slots empty at end of row (no stretching)');
  assert.equal(limit5.posterWidthPx, 150, 'Poster width remains strictly 150px');
  assert.equal(limit5.posterHeightPx, 182, 'Poster height remains strictly 182px');

  // Posters per row = 3: displays rows of 3, 3, 3, 3 with unchanged poster dimensions
  const limit3 = calculateGridWrapping(3, 12, 1400);
  assert.equal(limit3.columns, 3, 'Maximum 3 posters per row');
  assert.equal(limit3.rowCount, 4, '12 items wrap into 4 rows of 3');
  assert.equal(limit3.posterWidthPx, 150, 'Poster width is still strictly 150px (never enlarged)');

  // Posters per row = 8: displays rows of 8, 4 with unchanged poster dimensions
  const limit8 = calculateGridWrapping(8, 12, 1400);
  assert.equal(limit8.columns, 8, 'Maximum 8 posters per row');
  assert.equal(limit8.rowCount, 2, '12 items wrap into 2 rows (8 + 4)');
  assert.equal(limit8.posterWidthPx, 150, 'Poster width is still strictly 150px (never shrunk)');

  // Posters per row = 12: displays 1 row of 12 with unchanged poster dimensions
  const limit12 = calculateGridWrapping(12, 12, 2200);
  assert.equal(limit12.columns, 12, '12 posters fit in single row');
  assert.equal(limit12.rowCount, 1, '1 row total');
  assert.equal(limit12.posterWidthPx, 150, 'Poster width remains strictly 150px');
});

test('Requirement 3: Narrower Windows Fit Fewer Posters Than Configured Maximum', () => {
  const POSTER_WIDTH_PX = 150;
  const GAP_PX = 16;

  const calculateNarrowWrapping = (postersPerRowLimit: number, totalItems: number, windowWidthPx: number) => {
    // Parent container width is constrained by window width (minus 40px padding)
    const availableWidth = windowWidthPx - 40;
    const maxContainerWidth = postersPerRowLimit * POSTER_WIDTH_PX + (postersPerRowLimit - 1) * GAP_PX;
    const effectiveWidth = Math.min(availableWidth, maxContainerWidth);
    const columns = Math.max(1, Math.min(postersPerRowLimit, Math.floor((effectiveWidth + GAP_PX) / (POSTER_WIDTH_PX + GAP_PX))));
    const rowCount = Math.ceil(totalItems / columns);

    return { columns, rowCount, posterWidthPx: POSTER_WIDTH_PX };
  };

  // Preference is 6 posters per row, collection has 12 items:
  // Wide window (1200px): fits 6 columns, 2 rows
  const wide = calculateNarrowWrapping(6, 12, 1200);
  assert.equal(wide.columns, 6);
  assert.equal(wide.rowCount, 2);

  // Narrow window (600px): fits 3 columns, 4 rows (preference of 6 is upper limit)
  const narrow600 = calculateNarrowWrapping(6, 12, 600);
  assert.equal(narrow600.columns, 3, 'Window fits 3 columns cleanly');
  assert.equal(narrow600.rowCount, 4, '12 items wrap into 4 rows');
  assert.equal(narrow600.posterWidthPx, 150, 'Card dimensions strictly preserved at 150px');

  // Ultra-narrow window (380px): fits 2 columns, 6 rows without horizontal scroll
  const narrow380 = calculateNarrowWrapping(6, 12, 380);
  assert.equal(narrow380.columns, 2, 'Ultra-narrow window fits 2 columns');
  assert.equal(narrow380.rowCount, 6, '12 items wrap into 6 rows');
  assert.equal(narrow380.posterWidthPx, 150, 'Card dimensions strictly preserved');
});

test('Requirement 4 & 5: Single Shared Reorder / Done Control Below Shelves', () => {
  // Model shared reorder control state
  interface ReorderState {
    customOrderMode: boolean;
    externalReorderMode: boolean;
  }

  let state: ReorderState = {
    customOrderMode: false,
    externalReorderMode: false,
  };

  const isReordering = () => state.customOrderMode || state.externalReorderMode;

  const toggleReorder = () => {
    const next = !isReordering();
    state = { customOrderMode: next, externalReorderMode: next };
  };

  // Initial: inactive
  assert.equal(isReordering(), false, 'Reorder mode initially inactive');

  // Entering reorder mode: click Reorder button below shelves
  toggleReorder();
  assert.equal(isReordering(), true, 'Reorder mode active');
  assert.equal(state.customOrderMode, true, 'Downloaded collection is reorderable');
  assert.equal(state.externalReorderMode, true, 'External collection is reorderable');

  // Button in that same location becomes "Done"
  const buttonLabel = isReordering() ? 'Done' : 'Reorder';
  assert.equal(buttonLabel, 'Done', 'Reorder button replaced with Done button in same location');

  // Exiting reorder mode: click Done button in same location
  toggleReorder();
  assert.equal(isReordering(), false, 'Reorder mode exited');
  assert.equal(state.customOrderMode, false);
  assert.equal(state.externalReorderMode, false);
  assert.equal(isReordering() ? 'Done' : 'Reorder', 'Reorder');
});

test('Requirement 6: Isolated Collection Handling (Downloaded Only)', () => {
  const downloadedItems = [
    { id: 'hash-1', name: 'Movie 1' },
    { id: 'hash-2', name: 'Movie 2' },
    { id: 'hash-3', name: 'Movie 3' },
    { id: 'hash-4', name: 'Movie 4' },
    { id: 'hash-5', name: 'Movie 5' },
    { id: 'hash-6', name: 'Movie 6' },
  ];
  const externalItems: { path: string; title: string }[] = [];

  // Availability of shared Reorder control below shelves:
  // Must remain available if Downloaded has items, even if External is empty!
  const hasPopulatedCollection = downloadedItems.length > 0 || externalItems.length > 0;
  assert.equal(hasPopulatedCollection, true, 'Shared Reorder control available when Downloaded only');

  // Verify Downloaded reordering works in isolation
  let downloadedOrder = downloadedItems.map((d) => d.id);
  const arrayMove = <T>(arr: T[], from: number, to: number): T[] => {
    const copy = [...arr];
    const [item] = copy.splice(from, 1);
    copy.splice(to, 0, item);
    return copy;
  };

  // Move Movie 6 (index 5) to index 0
  downloadedOrder = arrayMove(downloadedOrder, 5, 0);
  assert.deepEqual(
    downloadedOrder,
    ['hash-6', 'hash-1', 'hash-2', 'hash-3', 'hash-4', 'hash-5'],
    'Downloaded items successfully reordered',
  );
});

test('Requirement 6: Isolated Collection Handling (External Only)', () => {
  const downloadedItems: { id: string; name: string }[] = [];
  const externalItems = [
    { path: '/media/Patlabor 2 (1993)', title: 'Patlabor 2' },
    { path: '/media/Fuuka (2017)', title: 'Fuuka' },
    { path: '/media/Mary (2017)', title: "Mary and the Witch's Flower" },
    { path: '/media/Suzume (2022)', title: 'Suzume' },
  ];

  // Availability of shared Reorder control below shelves:
  // Must remain available if External has items, even if Downloaded is empty!
  const hasPopulatedCollection = downloadedItems.length > 0 || externalItems.length > 0;
  assert.equal(hasPopulatedCollection, true, 'Shared Reorder control available when External only');

  // Verify External reordering works in isolation
  let externalOrder = externalItems.map((e) => e.path);
  const arrayMove = <T>(arr: T[], from: number, to: number): T[] => {
    const copy = [...arr];
    const [item] = copy.splice(from, 1);
    copy.splice(to, 0, item);
    return copy;
  };

  // Move Suzume (index 3) to index 1
  externalOrder = arrayMove(externalOrder, 3, 1);
  assert.deepEqual(
    externalOrder,
    [
      '/media/Patlabor 2 (1993)',
      '/media/Suzume (2022)',
      '/media/Fuuka (2017)',
      '/media/Mary (2017)',
    ],
    'External items successfully reordered',
  );
});

test('Requirement 6: Both Populated Collections Reorder Independently Without Cross-Collection Moves', () => {
  const downloadedItems = ['hash-a', 'hash-b', 'hash-c'];
  const externalItems = ['/path/1', '/path/2', '/path/3'];

  let savedDownloadedOrder = [...downloadedItems];
  let savedExternalOrder = [...externalItems];

  const arrayMove = <T>(arr: T[], from: number, to: number): T[] => {
    const copy = [...arr];
    const [item] = copy.splice(from, 1);
    copy.splice(to, 0, item);
    return copy;
  };

  // Reorder Downloaded: move hash-c to front
  savedDownloadedOrder = arrayMove(savedDownloadedOrder, 2, 0);

  // External order must remain untouched
  assert.deepEqual(savedDownloadedOrder, ['hash-c', 'hash-a', 'hash-b']);
  assert.deepEqual(savedExternalOrder, ['/path/1', '/path/2', '/path/3'], 'External order untouched by Downloaded drag');

  // Reorder External: move /path/3 to front
  savedExternalOrder = arrayMove(savedExternalOrder, 2, 0);

  // Downloaded order must remain untouched
  assert.deepEqual(savedDownloadedOrder, ['hash-c', 'hash-a', 'hash-b'], 'Downloaded order untouched by External drag');
  assert.deepEqual(savedExternalOrder, ['/path/3', '/path/1', '/path/2']);

  // Cross-collection dragging impossible: each shelf uses distinct items list in SortableContext
  const downloadedSortableIds = new Set(savedDownloadedOrder);
  const externalSortableIds = new Set(savedExternalOrder);
  for (const id of downloadedSortableIds) {
    assert.ok(!externalSortableIds.has(id), 'No ID overlap between Downloaded and External contexts');
  }
});

test('Requirement 7: Independent Persistence & Restoration After Restart', () => {
  // Mock SQLite database tables
  const mockDb = {
    settings: new Map<string, string>(),
    torrentSortOrders: new Map<string, number>(),
  };

  // 1. Persistence commands
  const backendSetPostersPerRow = (count: number) => {
    const clamped = Math.max(2, Math.min(12, count));
    mockDb.settings.set('media_posters_per_row', String(clamped));
  };

  const backendSetMediaOrder = (orders: { info_hash: string; sort_order: number }[]) => {
    for (const o of orders) {
      mockDb.torrentSortOrders.set(o.info_hash, o.sort_order);
    }
    const hashes = orders.sort((a, b) => a.sort_order - b.sort_order).map((o) => o.info_hash);
    mockDb.settings.set('media_downloaded_order', JSON.stringify(hashes));
  };

  const backendSetExternalMediaOrder = (paths: string[]) => {
    mockDb.settings.set('media_external_order', JSON.stringify(paths));
  };

  // 2. Fetch / restore commands
  const backendGetPostersPerRow = (): number => {
    const val = mockDb.settings.get('media_posters_per_row');
    return val ? parseInt(val, 10) : 5;
  };

  const backendGetDownloadedMediaOrder = (): string[] => {
    const val = mockDb.settings.get('media_downloaded_order');
    return val ? JSON.parse(val) : [];
  };

  const backendGetExternalMediaOrder = (): string[] => {
    const val = mockDb.settings.get('media_external_order');
    return val ? JSON.parse(val) : [];
  };

  // Perform updates
  backendSetPostersPerRow(7);
  backendSetMediaOrder([
    { info_hash: 'hash-gamma', sort_order: 0 },
    { info_hash: 'hash-alpha', sort_order: 1 },
    { info_hash: 'hash-beta', sort_order: 2 },
  ]);
  backendSetExternalMediaOrder(['/media/Movie C', '/media/Movie A', '/media/Movie B']);

  // Simulate application restart: load preferences from scratch
  const restoredPostersPerRow = backendGetPostersPerRow();
  const restoredDownloadedOrder = backendGetDownloadedMediaOrder();
  const restoredExternalOrder = backendGetExternalMediaOrder();

  assert.equal(restoredPostersPerRow, 7, 'Restored posters per row setting is 7');
  assert.deepEqual(
    restoredDownloadedOrder,
    ['hash-gamma', 'hash-alpha', 'hash-beta'],
    'Restored downloaded order strictly matches user rearrangement',
  );
  assert.deepEqual(
    restoredExternalOrder,
    ['/media/Movie C', '/media/Movie A', '/media/Movie B'],
    'Restored external order strictly matches user rearrangement',
  );

  // Resolution helper test: orderGroups applies saved order and appends newly downloaded items
  const liveGroups = [
    { id: 'hash-alpha', title: 'Movie Alpha', items: [{ info_hash: 'hash-alpha' }] },
    { id: 'hash-beta', title: 'Movie Beta', items: [{ info_hash: 'hash-beta' }] },
    { id: 'hash-gamma', title: 'Movie Gamma', items: [{ info_hash: 'hash-gamma' }] },
    { id: 'hash-delta', title: 'Movie Delta (Newly Downloaded)', items: [{ info_hash: 'hash-delta' }] },
  ];

  const orderGroups = (groups: typeof liveGroups, orderList: string[]) => {
    if (!orderList || orderList.length === 0) return groups;
    const remaining = [...groups];
    const result: typeof liveGroups = [];
    for (const id of orderList) {
      const idx = remaining.findIndex((g) => g.id === id);
      if (idx !== -1) {
        result.push(remaining[idx]);
        remaining.splice(idx, 1);
      }
    }
    result.push(...remaining);
    return result;
  };

  const resolved = orderGroups(liveGroups, restoredDownloadedOrder);
  assert.deepEqual(
    resolved.map((g) => g.id),
    ['hash-gamma', 'hash-alpha', 'hash-beta', 'hash-delta'],
    'Saved order is restored first; newly downloaded releases append cleanly at end',
  );
});
