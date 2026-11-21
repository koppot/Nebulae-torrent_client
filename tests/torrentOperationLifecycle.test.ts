import test from 'node:test';
import assert from 'node:assert/strict';

// Mock Tauri invoke BEFORE importing store
const mockActiveSnapshots: any[] = [];
let dismissShouldFail = false;
globalThis.window = {
  __TAURI_INTERNALS__: {
    invoke: async (cmd: string, args: any) => {
      if (cmd === 'get_active_torrent_operations') {
        return mockActiveSnapshots;
      }
      if (cmd === 'dismiss_torrent_operation') {
        if (dismissShouldFail) {
          throw new Error('Backend dismissal failed: registry lock poisoned');
        }
        return undefined;
      }
      if (cmd === 'set_torrent_location') {
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

test('dismissOperation failure does not clear aliases or record tombstone', async () => {
  dismissShouldFail = true;
  const failErrorOp = {
    operation_id: 'op-fail-dismiss-1',
    torrent_id: 't-fail-dismiss',
    kind: { type: 'remove' as const, delete_files: false },
    phase: 'error' as const,
    safe_error: 'Simulated backend error',
    revision: 2,
    created_at: 4000,
  };

  useTorrentStore.setState({
    torrents: [
      {
        id: 't-fail-dismiss',
        name: 'Failure Test Torrent',
        info_hash: 'aabbccddeeff00112233aabbccddeeff00112233',
        status: 'Error',
      } as any,
    ],
    activeOperations: {
      't-fail-dismiss': failErrorOp,
      ['t-fail-dismiss'.toLowerCase()]: failErrorOp,
      'aabbccddeeff00112233aabbccddeeff00112233': failErrorOp,
      'op-fail-dismiss-1': failErrorOp,
    },
    tombstones: {},
  });

  // Attempt dismissal when backend throws
  await useTorrentStore.getState().dismissOperation('t-fail-dismiss');

  // Verify aliases are NOT cleared when backend dismissal fails
  assert.ok(
    useTorrentStore.getState().activeOperations['t-fail-dismiss'],
    'Aliases must be retained when backend dismissal fails'
  );
  assert.ok(
    useTorrentStore.getState().activeOperations['aabbccddeeff00112233aabbccddeeff00112233'],
    'Hash alias must be retained when backend dismissal fails'
  );
  assert.ok(
    useTorrentStore.getState().activeOperations['op-fail-dismiss-1'],
    'Operation ID alias must be retained when backend dismissal fails'
  );

  // Verify no tombstone is created
  assert.equal(
    useTorrentStore.getState().tombstones['op-fail-dismiss-1'],
    undefined,
    'Tombstone must not be created when backend dismissal fails'
  );

  dismissShouldFail = false;
});

test('Move complete event updates save_path, exposes notice, and retains torrent row', () => {
  useTorrentStore.setState({
    torrents: [
      {
        id: 't-move-1',
        name: 'The Matrix 1999',
        info_hash: '4455667788990011223344556677889900112233',
        save_path: '/Volumes/OldStorage/Movies',
        status: 'Seeding',
        progress: 1.0,
      } as any,
    ],
    activeOperations: {
      't-move-1': {
        operation_id: 'op-move-1',
        torrent_id: 't-move-1',
        kind: { type: 'move', destination: '/Volumes/FastNVMe/Movies' },
        phase: 'moving',
        destination: '/Volumes/FastNVMe/Movies',
        revision: 1,
        created_at: 5000,
      },
    },
    terminalNotice: null,
    lastOutcome: null,
    tombstones: {},
  });

  // Dispatch complete event for move
  useTorrentStore.getState().handleOperationEvent({
    operation_id: 'op-move-1',
    torrent_id: 't-move-1',
    kind: { type: 'move', destination: '/Volumes/FastNVMe/Movies' },
    phase: 'complete',
    destination: '/Volumes/FastNVMe/Movies',
    outcome: 'Moved to /Volumes/FastNVMe/Movies',
    revision: 2,
    created_at: 5000,
  });

  // 1. Torrent row is RETAINED (not deleted)
  assert.equal(useTorrentStore.getState().torrents.length, 1, 'Move complete must retain torrent row');
  const updatedTorrent = useTorrentStore.getState().torrents[0];
  assert.equal(updatedTorrent.id, 't-move-1');

  // 2. save_path is updated to the confirmed new destination
  assert.equal(
    updatedTorrent.save_path,
    '/Volumes/FastNVMe/Movies',
    'Move complete must update save_path to confirmed destination'
  );

  // 3. Terminal outcome is exposed
  assert.equal(useTorrentStore.getState().lastOutcome, 'Moved to /Volumes/FastNVMe/Movies');
  assert.equal(useTorrentStore.getState().terminalNotice?.outcome, 'Moved to /Volumes/FastNVMe/Movies');
  assert.equal(useTorrentStore.getState().terminalNotice?.name, 'The Matrix 1999');

  // 4. Active operation aliases are cleared and tombstoned
  assert.equal(useTorrentStore.getState().activeOperations['t-move-1'], undefined);
  assert.ok(useTorrentStore.getState().tombstones['op-move-1']);
});

test('Move in-flight retains last confirmed location until engine-confirmed success', () => {
  useTorrentStore.setState({
    torrents: [
      {
        id: 't-move-inflight',
        name: 'Inception 2010',
        info_hash: '5566778899001122334455667788990011223344',
        save_path: '/Volumes/DiskA/Movies',
        status: 'Downloading',
        progress: 0.8,
      } as any,
    ],
    activeOperations: {},
    tombstones: {},
  });

  // Dispatch moving event
  useTorrentStore.getState().handleOperationEvent({
    operation_id: 'op-move-2',
    torrent_id: 't-move-inflight',
    kind: { type: 'move', destination: '/Volumes/DiskB/Movies' },
    phase: 'moving',
    destination: '/Volumes/DiskB/Movies',
    revision: 1,
    created_at: 6000,
  });

  // In-flight: row is retained and still shows last confirmed location
  assert.equal(useTorrentStore.getState().torrents.length, 1);
  assert.equal(
    useTorrentStore.getState().torrents[0].save_path,
    '/Volumes/DiskA/Movies',
    'In-flight move must retain last confirmed location until completed'
  );

  const activeOp = useTorrentStore.getState().activeOperations['t-move-inflight'];
  assert.ok(activeOp);
  assert.equal(activeOp.phase, 'moving');
  assert.equal(activeOp.destination, '/Volumes/DiskB/Movies');
});

test('Move failure exposes safe_error and allows dismissal', async () => {
  useTorrentStore.setState({
    torrents: [
      {
        id: 't-move-fail',
        name: 'Blade Runner 2049',
        info_hash: '6677889900112233445566778899001122334455',
        save_path: '/Volumes/DiskA/Movies',
        status: 'Downloading',
      } as any,
    ],
    activeOperations: {},
    tombstones: {},
  });

  // Dispatch error event for move
  useTorrentStore.getState().handleOperationEvent({
    operation_id: 'op-move-fail-1',
    torrent_id: 't-move-fail',
    kind: { type: 'move', destination: '/Volumes/ReadOnly/Movies' },
    phase: 'error',
    safe_error: 'Move failed: Read-only file system',
    revision: 2,
    created_at: 7000,
  });

  const activeOp = useTorrentStore.getState().activeOperations['t-move-fail'];
  assert.ok(activeOp);
  assert.equal(activeOp.phase, 'error');
  assert.equal(activeOp.safe_error, 'Move failed: Read-only file system');

  // Dismissing the move error clears it
  await useTorrentStore.getState().dismissOperation('t-move-fail');
  assert.equal(useTorrentStore.getState().activeOperations['t-move-fail'], undefined);
  assert.ok(useTorrentStore.getState().tombstones['op-move-fail-1']);
});

test('setLocation prevents starting conflicting move while operation is in-flight', async () => {
  useTorrentStore.setState({
    torrents: [
      {
        id: 't-move-conflict',
        name: 'Interstellar 2014',
        info_hash: '7788990011223344556677889900112233445566',
        save_path: '/Volumes/DiskA/Movies',
        status: 'Downloading',
      } as any,
    ],
    activeOperations: {
      't-move-conflict': {
        operation_id: 'op-move-conflict-1',
        torrent_id: 't-move-conflict',
        kind: { type: 'move', destination: '/Volumes/DiskB/Movies' },
        phase: 'moving',
        destination: '/Volumes/DiskB/Movies',
        revision: 1,
        created_at: 8000,
      },
    },
    tombstones: {},
  });

  // Calling setLocation when another operation is already in flight should be a no-op
  await useTorrentStore.getState().setLocation('t-move-conflict', '/Volumes/DiskC/Movies');

  const activeOp = useTorrentStore.getState().activeOperations['t-move-conflict'];
  assert.equal(
    activeOp.destination,
    '/Volumes/DiskB/Movies',
    'Conflicting setLocation must not overwrite in-flight move operation'
  );
});
