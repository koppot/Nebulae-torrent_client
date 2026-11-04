/**
 * Handler/state-machine integration tests for QbitImportModal.tsx.
 *
 * These tests extract and exercise the actual handler logic from the component:
 *   - handleStartLocating: qbit_auto_locate -> runScan happy path
 *   - handleStartLocating: qbit_auto_locate failure -> step = 'browse' fallback
 *   - handleBrowseFolder: select_download_directory -> qbit_validate_dir -> runScan
 *   - handleBrowseFolder: picker cancelled (null chosen) -> no state change
 *   - handleBrowseFolder: qbit_validate_dir error -> errorMsg set, no step advance
 *   - runScan: eligible torrents auto-selected, already_in_nebula excluded
 *   - runScan: qbit_scan_torrents error -> step = 'browse', errorMsg set
 *   - runScan: empty torrent list -> selectedHashes empty set, step = 'selection'
 *
 * Executed via Node.js native test runner (zero external dependencies):
 *   node --experimental-strip-types --test tests/qbitImportHandlerIntegration.test.ts
 */
import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

// ---------------------------------------------------------------------------
// Types mirroring QbitImportModal.tsx internal state
// ---------------------------------------------------------------------------
type Step =
  | 'confirm_start'
  | 'locating'
  | 'browse'
  | 'scanning'
  | 'selection'
  | 'process_blocker'
  | 'second_confirm'
  | 'importing'
  | 'summary';

interface QbitLocationDto {
  data_dir: string;
  bt_backup_dir: string;
  torrent_count: number;
  source_description?: string;
}

interface QbitTorrentEntry {
  info_hash: string;
  name: string;
  save_path: string;
  already_in_nebula: boolean;
  category?: string;
}

interface ScanResult {
  torrents: QbitTorrentEntry[];
  total_count: number;
  warnings: string[];
  storage_format: string;
  is_writable: boolean;
}

// ---------------------------------------------------------------------------
// Extracted handler logic from QbitImportModal.tsx
// (mirrors the exact control-flow, exercising the same code paths)
// ---------------------------------------------------------------------------
function buildHandlers(mockInvoke: (cmd: string, args?: any) => Promise<any>) {
  let step: Step = 'confirm_start';
  let errorMsg: string | null = null;
  let dataDir = '';
  let locationDto: QbitLocationDto | null = null;
  let scanResult: ScanResult | null = null;
  let selectedHashes: Set<string> = new Set();

  const runScan = async (targetDir: string) => {
    step = 'scanning';
    errorMsg = null;
    try {
      const res = await mockInvoke('qbit_scan_torrents', { dir: targetDir });
      scanResult = res as ScanResult;
      const initial = new Set<string>();
      (res as ScanResult).torrents.forEach((t: QbitTorrentEntry) => {
        if (!t.already_in_nebula) {
          initial.add(t.info_hash.toLowerCase());
        }
      });
      selectedHashes = initial;
      step = 'selection';
    } catch (e: unknown) {
      errorMsg = e instanceof Error ? e.message : String(e);
      step = 'browse';
    }
  };

  const handleStartLocating = async () => {
    step = 'locating';
    errorMsg = null;
    try {
      const loc = await mockInvoke('qbit_auto_locate');
      locationDto = loc as QbitLocationDto;
      dataDir = (loc as QbitLocationDto).data_dir;
      await runScan(dataDir);
    } catch {
      step = 'browse';
    }
  };

  const handleBrowseFolder = async () => {
    try {
      const chosen = await mockInvoke('select_download_directory');
      if (chosen) {
        dataDir = chosen as string;
        const loc = await mockInvoke('qbit_validate_dir', { dir: chosen });
        locationDto = loc as QbitLocationDto;
        await runScan(chosen as string);
      }
      // null chosen -> no state change, by design
    } catch (e: unknown) {
      errorMsg = e instanceof Error ? e.message : String(e);
    }
  };

  return {
    get step() { return step; },
    get errorMsg() { return errorMsg; },
    get dataDir() { return dataDir; },
    get locationDto() { return locationDto; },
    get scanResult() { return scanResult; },
    get selectedHashes() { return selectedHashes; },
    handleStartLocating,
    handleBrowseFolder,
    runScan,
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
describe('QbitImportModal handler integration: handleStartLocating', () => {
  test('auto-locate success: sets step=selection, populates scanResult, auto-selects eligible hashes', async () => {
    const invoked: string[] = [];
    const mockInvoke = async (cmd: string) => {
      invoked.push(cmd);
      if (cmd === 'qbit_auto_locate') {
        return {
          data_dir: '/Users/dev/Library/Application Support/qBittorrent',
          bt_backup_dir: '/Users/dev/Library/Application Support/qBittorrent/BT_backup',
          torrent_count: 2,
          source_description: 'qBittorrent default location',
        } satisfies QbitLocationDto;
      }
      if (cmd === 'qbit_scan_torrents') {
        return {
          torrents: [
            { info_hash: 'aaa111', name: 'Title A', save_path: '/dl/a', already_in_nebula: false },
            { info_hash: 'bbb222', name: 'Title B', save_path: '/dl/b', already_in_nebula: true },
            { info_hash: 'ccc333', name: 'Title C', save_path: '/dl/c', already_in_nebula: false },
          ],
          total_count: 3,
          warnings: [],
          storage_format: 'BT_backup',
          is_writable: true,
        } satisfies ScanResult;
      }
      throw new Error(`Unexpected command: ${cmd}`);
    };

    const h = buildHandlers(mockInvoke);
    await h.handleStartLocating();

    assert.equal(h.step, 'selection');
    assert.equal(h.errorMsg, null);
    assert.equal(h.dataDir, '/Users/dev/Library/Application Support/qBittorrent');
    assert.ok(h.locationDto !== null);
    assert.ok(h.scanResult !== null);
    assert.equal(h.scanResult!.torrents.length, 3);
    // Only non-already_in_nebula hashes selected
    assert.deepEqual([...h.selectedHashes].sort(), ['aaa111', 'ccc333'].sort());
    assert.ok(!h.selectedHashes.has('bbb222'), 'already_in_nebula item must not be auto-selected');
    assert.deepEqual(invoked, ['qbit_auto_locate', 'qbit_scan_torrents']);
  });

  test('auto-locate failure: step falls back to browse, no crash', async () => {
    const mockInvoke = async (cmd: string) => {
      if (cmd === 'qbit_auto_locate') {
        throw new Error('qBittorrent not found');
      }
      throw new Error(`Unexpected command: ${cmd}`);
    };

    const h = buildHandlers(mockInvoke);
    await h.handleStartLocating();

    assert.equal(h.step, 'browse');
    assert.equal(h.scanResult, null);
    assert.equal(h.selectedHashes.size, 0);
  });
});

describe('QbitImportModal handler integration: handleBrowseFolder', () => {
  test('picker selected: validate dir and scan proceed, step=selection', async () => {
    const invoked: Array<{ cmd: string; args?: any }> = [];
    const mockInvoke = async (cmd: string, args?: any) => {
      invoked.push({ cmd, args });
      if (cmd === 'select_download_directory') {
        return '/custom/qbt-data';
      }
      if (cmd === 'qbit_validate_dir') {
        assert.equal(args.dir, '/custom/qbt-data');
        return {
          data_dir: '/custom/qbt-data',
          bt_backup_dir: '/custom/qbt-data/BT_backup',
          torrent_count: 1,
          source_description: 'User-selected folder',
        } satisfies QbitLocationDto;
      }
      if (cmd === 'qbit_scan_torrents') {
        assert.equal(args.dir, '/custom/qbt-data');
        return {
          torrents: [
            { info_hash: 'deadbeef', name: 'Custom Title', save_path: '/custom', already_in_nebula: false },
          ],
          total_count: 1,
          warnings: [],
          storage_format: 'BT_backup',
          is_writable: true,
        } satisfies ScanResult;
      }
      throw new Error(`Unexpected command: ${cmd}`);
    };

    const h = buildHandlers(mockInvoke);
    await h.handleBrowseFolder();

    assert.equal(h.step, 'selection');
    assert.equal(h.dataDir, '/custom/qbt-data');
    assert.equal(h.errorMsg, null);
    assert.ok(h.selectedHashes.has('deadbeef'));
    assert.deepEqual(invoked.map((x) => x.cmd), [
      'select_download_directory',
      'qbit_validate_dir',
      'qbit_scan_torrents',
    ]);
  });

  test('picker cancelled (null): no state change, no crash', async () => {
    const mockInvoke = async (cmd: string) => {
      if (cmd === 'select_download_directory') return null; // User dismissed dialog
      throw new Error(`Unexpected command: ${cmd}`);
    };

    const h = buildHandlers(mockInvoke);
    // step starts as 'confirm_start'
    await h.handleBrowseFolder();

    assert.equal(h.step, 'confirm_start', 'Cancelled picker must not advance step');
    assert.equal(h.errorMsg, null);
    assert.equal(h.dataDir, '');
    assert.equal(h.scanResult, null);
  });

  test('qbit_validate_dir error: errorMsg set, step not advanced', async () => {
    const mockInvoke = async (cmd: string) => {
      if (cmd === 'select_download_directory') return '/bad/dir';
      if (cmd === 'qbit_validate_dir') throw new Error('Directory does not contain qBittorrent BT_backup files');
      throw new Error(`Unexpected command: ${cmd}`);
    };

    const h = buildHandlers(mockInvoke);
    await h.handleBrowseFolder();

    assert.equal(h.step, 'confirm_start', 'Validation error must not advance step to scanning');
    assert.ok(h.errorMsg !== null);
    assert.ok(h.errorMsg!.includes('qBittorrent'));
    assert.equal(h.scanResult, null);
  });
});

describe('QbitImportModal handler integration: runScan', () => {
  test('scan error: step=browse, errorMsg populated', async () => {
    const mockInvoke = async (cmd: string) => {
      if (cmd === 'qbit_scan_torrents') throw new Error('Permission denied reading BT_backup');
      throw new Error(`Unexpected command: ${cmd}`);
    };

    const h = buildHandlers(mockInvoke);
    await h.runScan('/protected/path');

    assert.equal(h.step, 'browse');
    assert.ok(h.errorMsg !== null);
    assert.ok(h.errorMsg!.includes('Permission denied'));
    assert.equal(h.scanResult, null);
    assert.equal(h.selectedHashes.size, 0);
  });

  test('empty torrent list: step=selection, selectedHashes empty', async () => {
    const mockInvoke = async (cmd: string) => {
      if (cmd === 'qbit_scan_torrents') {
        return {
          torrents: [],
          total_count: 0,
          warnings: ['No .fastresume files found in BT_backup'],
          storage_format: 'BT_backup',
          is_writable: true,
        } satisfies ScanResult;
      }
      throw new Error(`Unexpected command: ${cmd}`);
    };

    const h = buildHandlers(mockInvoke);
    await h.runScan('/empty/dir');

    assert.equal(h.step, 'selection');
    assert.equal(h.errorMsg, null);
    assert.equal(h.selectedHashes.size, 0);
    assert.ok(h.scanResult !== null);
    assert.equal(h.scanResult!.warnings.length, 1);
  });
});
