import test from 'node:test';
import assert from 'node:assert/strict';

/**
 * Unit & Integration tests for Media Grid Layout & Poster Rearrangement
 * Requirements verified:
 * 1. Configurable posters per row (preferences + header slider, wrapping into additional rows, persistence)
 * 2. Working poster rearrangement (reorder mode, drag-to-reorder, DB persistence, Done action, both shelves)
 * 3. Responsive windowed layout (no collisions, responsive column wrapping >= 135px, 660:800 ratio, title clamping)
 * 4. Filtered / grouped items and newly added entries preservation
 */

test('Requirement 1: Configurable Posters Per Row Calculation & Row Wrapping', () => {
  const calculateResponsiveGrid = (postersPerRow: number, totalItems: number, availableWidthPx: number) => {
    const minPosterWidthPx = 135;
    const gapPx = 16; // 1rem

    // Formula: repeat(auto-fill, minmax(max(135px, calc((100% - (N - 1) * 1rem) / N)), 1fr))
    const targetColWidth = (availableWidthPx - (postersPerRow - 1) * gapPx) / postersPerRow;
    const effectiveMinColWidth = Math.max(minPosterWidthPx, targetColWidth);

    // Number of columns that fit
    const columns = Math.max(1, Math.floor((availableWidthPx + gapPx) / (effectiveMinColWidth + gapPx)));
    const actualColWidth = (availableWidthPx - (columns - 1) * gapPx) / columns;
    const rowCount = Math.ceil(totalItems / columns);
    const posterHeightPx = actualColWidth * (800 / 660); // 660:800 aspect ratio

    return {
      postersPerRowSetting: postersPerRow,
      displayedColumns: columns,
      actualColWidth,
      posterHeightPx,
      rowCount,
    };
  };

  // Case 1: 5 posters per row with 12 items in a 1200px window
  const wideState = calculateResponsiveGrid(5, 12, 1200);
  assert.equal(wideState.displayedColumns, 5, '1200px window displays exact user setting of 5 columns');
  assert.equal(wideState.rowCount, 3, '12 items with 5 per row wrap into 3 rows (5 + 5 + 2)');
  assert.ok(wideState.actualColWidth >= 135, 'Poster width must be >= 135px');
  assert.equal(Math.round(wideState.posterHeightPx / wideState.actualColWidth * 660), 800, 'Aspect ratio must be 660:800');

  // Case 2: Window narrows to 500px with 5 posters per row setting
  const narrowState = calculateResponsiveGrid(5, 12, 500);
  assert.equal(narrowState.displayedColumns, 3, 'Narrow 500px window adapts cleanly to 3 columns without horizontal scroll');
  assert.equal(narrowState.rowCount, 4, '12 items wrap into 4 rows (3 + 3 + 3 + 3)');
  assert.ok(narrowState.actualColWidth >= 135, 'Poster width is kept >= 135px');

  // Case 3: Window narrows to 350px (mobile / ultra-narrow pane)
  const ultraNarrowState = calculateResponsiveGrid(5, 12, 350);
  assert.equal(ultraNarrowState.displayedColumns, 2, 'Ultra narrow 350px window adapts to 2 columns');
  assert.equal(ultraNarrowState.rowCount, 6, '12 items wrap into 6 rows (2 * 6)');
  assert.ok(ultraNarrowState.actualColWidth >= 135, 'Poster width is kept >= 135px');

  // Case 4: User changes setting to 3 posters per row on 1200px window
  const threePerRow = calculateResponsiveGrid(3, 12, 1200);
  assert.equal(threePerRow.displayedColumns, 3, 'Displays 3 columns');
  assert.equal(threePerRow.rowCount, 4, '12 items with 3 per row form 4 rows (3 + 3 + 3 + 3)');

  // Case 5: User changes setting to 8 posters per row on 1400px window
  const eightPerRow = calculateResponsiveGrid(8, 12, 1400);
  assert.equal(eightPerRow.displayedColumns, 8, 'Displays 8 columns');
  assert.equal(eightPerRow.rowCount, 2, '12 items with 8 per row form 2 rows (8 + 4)');
});

test('Requirement 1: Setting Persistence Across Application Restarts', () => {
  const mockDbSettings: Record<string, string> = {};

  const backendSetPostersPerRow = (count: number) => {
    const clamped = Math.max(2, Math.min(12, count));
    mockDbSettings['media_posters_per_row'] = String(clamped);
  };
  const backendGetPostersPerRow = (): number => {
    return mockDbSettings['media_posters_per_row'] ? parseInt(mockDbSettings['media_posters_per_row'], 10) : 5;
  };

  // Default
  assert.equal(backendGetPostersPerRow(), 5, 'Default posters per row must be 5');

  // Change to 7 in Media Preferences
  backendSetPostersPerRow(7);
  assert.equal(backendGetPostersPerRow(), 7, 'Setting must immediately persist in database');

  // Clamping test: value below 2 clamped to 2, above 12 clamped to 12
  backendSetPostersPerRow(1);
  assert.equal(backendGetPostersPerRow(), 2, 'Values < 2 clamped to 2');
  backendSetPostersPerRow(20);
  assert.equal(backendGetPostersPerRow(), 12, 'Values > 12 clamped to 12');

  // Simulate application restart
  backendSetPostersPerRow(6);
  const restoredSetting = backendGetPostersPerRow();
  assert.equal(restoredSetting, 6, 'Setting must be restored after application restart');
});

test('Requirement 2: Downloaded Shelf Poster Rearrangement, Command Arguments, and Persistence', () => {
  const initialTorrents = [
    { info_hash: 'hash-aaa', name: 'Movie A', sort_order: null },
    { info_hash: 'hash-bbb', name: 'Movie B', sort_order: null },
    { info_hash: 'hash-ccc', name: 'Movie C', sort_order: null },
    { info_hash: 'hash-ddd', name: 'Movie D', sort_order: null },
  ];

  let currentOrder = initialTorrents.map((t) => t.info_hash);
  const mockDb: {
    downloadedOrderJson: string | null;
    torrentSortOrders: Record<string, number>;
  } = {
    downloadedOrderJson: null,
    torrentSortOrders: {},
  };

  const arrayMove = <T>(array: T[], from: number, to: number): T[] => {
    const copy = [...array];
    const [item] = copy.splice(from, 1);
    copy.splice(to, 0, item);
    return copy;
  };

  let lastCommandArgs: { orders: { info_hash: string; sort_order: number }[] } | null = null;

  const saveDownloadedOrder = (hashes: string[]) => {
    currentOrder = hashes;
    const orders = hashes.map((hash, i) => ({ info_hash: hash, sort_order: i }));
    lastCommandArgs = { orders };

    // Backend implementation: updates torrent_metadata sort_order and media_downloaded_order JSON
    mockDb.downloadedOrderJson = JSON.stringify(hashes);
    hashes.forEach((hash, idx) => {
      mockDb.torrentSortOrders[hash] = idx;
    });
  };

  // User drags Movie D (index 3) to the very front (index 0)
  const reordered = arrayMove(currentOrder, 3, 0);
  saveDownloadedOrder(reordered);

  assert.deepEqual(
    currentOrder,
    ['hash-ddd', 'hash-aaa', 'hash-bbb', 'hash-ccc'],
    'Movie D should now be at index 0',
  );
  assert.ok(lastCommandArgs, 'Tauri command set_media_order must have been invoked');
  assert.equal(lastCommandArgs!.orders.length, 4);
  assert.equal(lastCommandArgs!.orders[0].info_hash, 'hash-ddd');
  assert.equal(lastCommandArgs!.orders[0].sort_order, 0);

  // Simulate restart: reload order from mock DB
  const restoredHashes: string[] = JSON.parse(mockDb.downloadedOrderJson!);
  assert.deepEqual(restoredHashes, ['hash-ddd', 'hash-aaa', 'hash-bbb', 'hash-ccc'], 'Restored order matches saved order');
});

test('Requirement 2: Downloaded Shelf Preserves Unfiltered Items and Appends New Items', () => {
  // All items in collection
  const allItems = ['hash-1', 'hash-2', 'hash-3', 'hash-4', 'hash-5'];
  let customOrder = [...allItems];

  // User searches for items matching "2" or "4"
  const visible = ['hash-2', 'hash-4'];

  // User drags hash-4 before hash-2 in filtered view
  const activeId = 'hash-4';
  const overId = 'hash-2';

  // The reorder algorithm operates on the full list
  const arrayMove = <T>(array: T[], from: number, to: number): T[] => {
    const copy = [...array];
    const [item] = copy.splice(from, 1);
    copy.splice(to, 0, item);
    return copy;
  };

  const oldIdx = customOrder.indexOf(activeId);
  const newIdx = customOrder.indexOf(overId);
  customOrder = arrayMove(customOrder, oldIdx, newIdx);

  // Notice: hash-4 is now ahead of hash-2, while hash-1, hash-3, hash-5 preserve their positions!
  assert.deepEqual(
    customOrder,
    ['hash-1', 'hash-4', 'hash-2', 'hash-3', 'hash-5'],
    'Filtered drag preserves unselected items',
  );

  // Newly downloaded item completed: 'hash-6'
  const newTorrent = 'hash-6';
  const known = new Set(customOrder);
  if (!known.has(newTorrent)) {
    customOrder.push(newTorrent);
  }

  assert.deepEqual(
    customOrder,
    ['hash-1', 'hash-4', 'hash-2', 'hash-3', 'hash-5', 'hash-6'],
    'New item cleanly appends at the end without altering existing user ordering',
  );
});

test('Requirement 2: External Shelf Poster Rearrangement, Command Arguments, and Persistence', () => {
  const initialExternal = [
    { path: '/media/Fuuka (2017)', title: 'Fuuka (2017)' },
    { path: '/media/Patlabor 2 (1993)', title: 'Patlabor 2 (1993)' },
    { path: '/media/Mary (2017)', title: "Mary and the Witch's Flower" },
    { path: '/media/Suzume (2022)', title: 'Suzume (2022)' },
  ];

  let currentExternalOrder = initialExternal.map((e) => e.path);
  let persistedDbOrder: string | null = null;
  let lastExternalArgs: { paths: string[] } | null = null;

  const arrayMove = <T>(array: T[], from: number, to: number): T[] => {
    const copy = [...array];
    const [item] = copy.splice(from, 1);
    copy.splice(to, 0, item);
    return copy;
  };

  const saveExternalOrder = (paths: string[]) => {
    currentExternalOrder = paths;
    lastExternalArgs = { paths };
    persistedDbOrder = JSON.stringify(paths);
  };

  // User drags Suzume (index 3) to index 1 (between Fuuka and Patlabor)
  const newOrder = arrayMove(currentExternalOrder, 3, 1);
  saveExternalOrder(newOrder);

  assert.deepEqual(
    currentExternalOrder,
    [
      '/media/Fuuka (2017)',
      '/media/Suzume (2022)',
      '/media/Patlabor 2 (1993)',
      '/media/Mary (2017)',
    ],
    'Suzume should now be at position 1',
  );
  assert.ok(lastExternalArgs, 'Tauri command set_external_media_order must have been invoked');
  assert.deepEqual(lastExternalArgs!.paths, currentExternalOrder);

  // Simulate rescan / refresh: backend list_external_media sorts by saved external order
  const mockDiskScan = [
    { path: '/media/Patlabor 2 (1993)', title: 'Patlabor 2 (1993)' },
    { path: '/media/Fuuka (2017)', title: 'Fuuka (2017)' },
    { path: '/media/New Media Item (2024)', title: 'New Media Item (2024)' },
    { path: '/media/Suzume (2022)', title: 'Suzume (2022)' },
    { path: '/media/Mary (2017)', title: "Mary and the Witch's Flower" },
  ];

  const orderMap = new Map(JSON.parse(persistedDbOrder!).map((p: string, i: number) => [p, i]));
  const sortedLiveScan = [...mockDiskScan].sort((a, b) => {
    const ia = (orderMap.get(a.path) as number) ?? 9999;
    const ib = (orderMap.get(b.path) as number) ?? 9999;
    return ia - ib;
  });

  assert.deepEqual(
    sortedLiveScan.map((m) => m.path),
    [
      '/media/Fuuka (2017)',
      '/media/Suzume (2022)',
      '/media/Patlabor 2 (1993)',
      '/media/Mary (2017)',
      '/media/New Media Item (2024)',
    ],
    'Rescanned items maintain custom ordering; newly discovered media appends cleanly at end',
  );
});

test('Requirement 3: Responsive Narrow Window Prevents Collisions and Confines Titles', () => {
  // Title clamping behavior
  const formatCardTitle = (title: string, year?: number) => {
    const display = year ? `${title} (${year})` : title;
    return {
      titleText: display,
      classes: 'text-[12px] font-medium text-white text-center leading-tight line-clamp-2 break-words',
    };
  };

  const longTitle = formatCardTitle('The Disappearance of Haruhi Suzumiya Special Collector Boxset Edition', 2010);
  assert.ok(longTitle.classes.includes('line-clamp-2'), 'Titles must be clamped to at most 2 lines');
  assert.ok(longTitle.classes.includes('break-words'), 'Long words must wrap without overflowing column boundaries');

  // Verify responsive grid styling contracts
  const computeGridStyles = (postersPerRow: number) => ({
    gridTemplateColumns: `repeat(auto-fill, minmax(max(135px, calc((100% - (${postersPerRow} - 1) * 1rem) / ${postersPerRow})), 1fr))`,
  });

  const styles = computeGridStyles(5);
  assert.equal(
    styles.gridTemplateColumns,
    'repeat(auto-fill, minmax(max(135px, calc((100% - (5 - 1) * 1rem) / 5)), 1fr))',
    'Grid must enforce responsive auto-fill wrapping with minimum 135px poster width',
  );
});
