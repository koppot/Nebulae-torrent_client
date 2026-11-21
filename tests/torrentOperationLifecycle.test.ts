import test from 'node:test';
import assert from 'node:assert/strict';

// Mock Tauri invoke BEFORE importing store
const mockActiveSnapshots: any[] = [];
globalThis.window = {
  __TAURI_INTERNALS__: {
    invoke: async (cmd: string, args: any) => {
      if (cmd === 'get_active_torrent_operations') {
        return mockActiveSnapshots;
      }
      if (cmd === 'dismiss_torrent_operation') {
        return undefined;
      }
      if (cmd === 'list_torrents') {
        return [];
      }
      return undefined;
    },
  },
} as any;

import { useTorrentStore } from '../src/stores/torrentStore.ts';

test('Complete event exposes the outcome and removes the transfer row', () => {
  useTorrentStore.setState({
    torrents: [
      {
        id: 't-complete-1',
        name: 'Ubuntu 24.04 Desktop',
        info_hash: '3b827e4dd3671f746816c22dd9e2bfa7285d38bb',
        save_path: '/downloads',
        status: 'Downloading',
        progress: 0.5,
        download_rate: 1000,
        upload_rate: 0,
        downloaded_bytes: 500,
        uploaded_bytes: 0,
        total_bytes: 1000,
        seeds: 10,
        peers: 5,
        ratio: 0,
        category: 'iso',
        added_on: 100,
      } as any,
    ],
    activeOperations: {
      't-complete-1': {
        operation_id: 'op-comp-1',
        torrent_id: 't-complete-1',
        kind: { type: 'remove', delete_files: false },
        phase: 'removing_torrent',
        revision: 1,
        created_at: 1000,
      },
    },
    terminalNotice: null,
    lastOutcome: null,
    tombstones: {},
  });

  // Verify torrent is initially in the list
  assert.equal(useTorrentStore.getState().torrents.length, 1);
  assert.equal(useTorrentStore.getState().torrents[0].id, 't-complete-1');

  // Dispatch complete event
  useTorrentStore.getState().handleOperationEvent({
    operation_id: 'op-comp-1',
    torrent_id: 't-complete-1',
    kind: { type: 'remove', delete_files: false },
    phase: 'complete',
    outcome: 'Torrent removed',
    revision: 2,
    created_at: 1000,
  });

  // 1. Transfer row is removed
  assert.equal(
    useTorrentStore.getState().torrents.find((t) => t.id === 't-complete-1'),
    undefined,
    'Transfer row must be removed from torrents upon completion'
  );
  assert.equal(useTorrentStore.getState().torrents.length, 0);

  // 2. Outcome is exposed via store
  assert.equal(
    useTorrentStore.getState().lastOutcome,
    'Torrent removed',
    'lastOutcome must expose the terminal outcome'
  );
  assert.notEqual(useTorrentStore.getState().terminalNotice, null);
  assert.equal(
    useTorrentStore.getState().terminalNotice?.outcome,
    'Torrent removed',
    'terminalNotice must contain the outcome'
  );
  assert.equal(
    useTorrentStore.getState().terminalNotice?.name,
    'Ubuntu 24.04 Desktop',
    'terminalNotice must preserve the torrent name'
  );

  // 3. Operation is removed from activeOperations
  assert.equal(useTorrentStore.getState().activeOperations['t-complete-1'], undefined);

  // 4. Dismissing terminal notice clears it
  useTorrentStore.getState().dismissTerminalNotice();
  assert.equal(useTorrentStore.getState().terminalNotice, null);
  assert.equal(useTorrentStore.getState().lastOutcome, null);
});

test('Older event/snapshot after completion does not resurrect the operation', async () => {
  // Ensure op-comp-1 is tombstoned from previous completion
  assert.ok(
    useTorrentStore.getState().tombstones['op-comp-1'],
    'Completed operation must have a tombstone'
  );

  // 1. Delayed older event for the completed operation arrives
  useTorrentStore.getState().handleOperationEvent({
    operation_id: 'op-comp-1',
    torrent_id: 't-complete-1',
    kind: { type: 'remove', delete_files: false },
    phase: 'removing_torrent',
    revision: 1,
    created_at: 1000,
  });

  assert.equal(
    useTorrentStore.getState().activeOperations['t-complete-1'],
    undefined,
    'Delayed older event must not resurrect completed operation in activeOperations'
  );
  assert.equal(
    useTorrentStore.getState().activeOperations['op-comp-1'],
    undefined,
    'Delayed older event must not resurrect completed operation alias'
  );

  // 2. In-flight snapshot from get_active_torrent_operations arrives
  mockActiveSnapshots.length = 0;
  mockActiveSnapshots.push({
    operation_id: 'op-comp-1',
    torrent_id: 't-complete-1',
    kind: { type: 'remove', delete_files: false },
    phase: 'removing_torrent',
    revision: 1,
    created_at: 1000,
  });

  await useTorrentStore.getState().fetchActiveOperations();

  assert.equal(
    useTorrentStore.getState().activeOperations['t-complete-1'],
    undefined,
    'Stale snapshot must not resurrect completed operation in activeOperations'
  );
  assert.equal(
    useTorrentStore.getState().activeOperations['op-comp-1'],
    undefined,
    'Stale snapshot must not resurrect completed operation alias'
  );
});

test('Dismissing an error clears its aliases without later resurrection', async () => {
  const errorOp = {
    operation_id: 'op-err-42',
    torrent_id: 't-err-1',
    kind: { type: 'remove' as const, delete_files: true },
    phase: 'error' as const,
    safe_error: 'Directory not found',
    revision: 2,
    created_at: 2000,
  };

  useTorrentStore.setState({
    torrents: [
      {
        id: 't-err-1',
        name: 'Erroneous Torrent',
        info_hash: '1122334455667788990011223344556677889900',
        status: 'Error',
        progress: 0.1,
      } as any,
    ],
    activeOperations: {
      't-err-1': errorOp,
      ['t-err-1'.toLowerCase()]: errorOp,
      '1122334455667788990011223344556677889900': errorOp,
      'op-err-42': errorOp,
    },
    tombstones: {},
  });

  // Verify all aliases are initially registered
  assert.ok(useTorrentStore.getState().activeOperations['t-err-1']);
  assert.ok(useTorrentStore.getState().activeOperations['1122334455667788990011223344556677889900']);
  assert.ok(useTorrentStore.getState().activeOperations['op-err-42']);

  // Dismiss the error operation
  await useTorrentStore.getState().dismissOperation('t-err-1');

  // 1. All aliases are cleared
  assert.equal(
    useTorrentStore.getState().activeOperations['t-err-1'],
    undefined,
    'Dismissal must clear id alias'
  );
  assert.equal(
    useTorrentStore.getState().activeOperations['1122334455667788990011223344556677889900'],
    undefined,
    'Dismissal must clear info_hash alias'
  );
  assert.equal(
    useTorrentStore.getState().activeOperations['op-err-42'],
    undefined,
    'Dismissal must clear operation_id alias'
  );

  // 2. Tombstone recorded
  assert.ok(
    useTorrentStore.getState().tombstones['op-err-42'],
    'Dismissed operation must have a tombstone'
  );

  // 3. Delayed event cannot resurrect dismissed error
  useTorrentStore.getState().handleOperationEvent(errorOp);
  assert.equal(
    useTorrentStore.getState().activeOperations['t-err-1'],
    undefined,
    'Delayed error event must not resurrect dismissed error'
  );
  assert.equal(
    useTorrentStore.getState().activeOperations['op-err-42'],
    undefined,
    'Delayed error event must not resurrect dismissed error alias'
  );

  // 4. In-flight snapshot cannot resurrect dismissed error
  mockActiveSnapshots.length = 0;
  mockActiveSnapshots.push(errorOp);
  await useTorrentStore.getState().fetchActiveOperations();

  assert.equal(
    useTorrentStore.getState().activeOperations['t-err-1'],
    undefined,
    'Stale snapshot must not resurrect dismissed error'
  );
  assert.equal(
    useTorrentStore.getState().activeOperations['op-err-42'],
    undefined,
    'Stale snapshot must not resurrect dismissed error alias'
  );
});

test('File deletion unconfirmed outcome is exposed truthfully', () => {
  useTorrentStore.setState({
    torrents: [
      {
        id: 't-file-del-1',
        name: 'Seeded Movie',
        info_hash: '9988776655443322110099887766554433221100',
        status: 'Seeding',
        progress: 1.0,
      } as any,
    ],
    activeOperations: {},
    tombstones: {},
  });

  useTorrentStore.getState().handleOperationEvent({
    operation_id: 'op-del-files-1',
    torrent_id: 't-file-del-1',
    kind: { type: 'remove', delete_files: true },
    phase: 'complete',
    outcome: 'Removal request accepted; file deletion not independently confirmed',
    revision: 2,
    created_at: 3000,
  });

  assert.equal(
    useTorrentStore.getState().lastOutcome,
    'Removal request accepted; file deletion not independently confirmed'
  );
  assert.equal(
    useTorrentStore.getState().terminalNotice?.outcome,
    'Removal request accepted; file deletion not independently confirmed'
  );
  assert.equal(useTorrentStore.getState().torrents.length, 0);
});
