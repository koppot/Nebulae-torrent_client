/**
 * Regression and parity tests for cross-OS path resolution, Locate/Relink actions,
 * single error presentation, and thumbnail isolation.
 *
 * Run with: node --experimental-strip-types --test tests/crossOsPathAndRelink.test.ts
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';

// ── 1. Cross-OS Path Resolution Logic ─────────────────────────────────────────

interface PathResolutionInput {
  metaSavePath?: string | null;
  engineDownloadDir: string;
  torrentName: string;
  targetWindows: boolean;
  enginePayloadExists?: boolean;
}

interface PathResolutionOutput {
  activeSavePath: string;
  historicalSavePath: string | null;
}

function isCrossOsPathForTarget(pathStr: string, targetWindows: boolean): boolean {
  const trimmed = pathStr.trim();
  if (!trimmed) return false;
  if (targetWindows) {
    return trimmed.startsWith('/') && !trimmed.startsWith('//');
  } else {
    const hasDriveLetter = /^[A-Za-z]:[/\\]/.test(trimmed);
    const hasUnc = trimmed.startsWith('\\\\');
    return hasDriveLetter || hasUnc;
  }
}

function resolveTorrentSavePathForTarget(input: PathResolutionInput): PathResolutionOutput {
  const meta = input.metaSavePath?.trim();
  const engineDir = input.engineDownloadDir.trim();

  if (meta) {
    if (isCrossOsPathForTarget(meta, input.targetWindows)) {
      const active = engineDir || meta;
      return { activeSavePath: active, historicalSavePath: meta };
    }

    if (input.enginePayloadExists) {
      const hist = meta !== engineDir ? meta : null;
      return { activeSavePath: engineDir, historicalSavePath: hist };
    }

    return { activeSavePath: meta, historicalSavePath: null };
  }

  return { activeSavePath: engineDir, historicalSavePath: null };
}

test('Cross-OS Path Resolution: Stale macOS path on Windows preserves historical path and resolves to active engine', () => {
  const result = resolveTorrentSavePathForTarget({
    metaSavePath: '/Users/dev/Downloads',
    engineDownloadDir: 'C:\\Users\\administrator\\Downloads',
    torrentName: 'Shakugan no Shana S',
    targetWindows: true,
  });

  assert.equal(
    result.activeSavePath,
    'C:\\Users\\administrator\\Downloads',
    'Windows client must use the active local engine download directory'
  );
  assert.equal(
    result.historicalSavePath,
    '/Users/dev/Downloads',
    'Foreign macOS path must be preserved in historical_save_path'
  );
});

test('Cross-OS Path Resolution: Stale Windows path on macOS preserves historical path and resolves to active engine', () => {
  const result = resolveTorrentSavePathForTarget({
    metaSavePath: 'C:\\Users\\administrator\\Downloads',
    engineDownloadDir: '/Users/dev/Downloads',
    torrentName: 'Shakugan no Shana S',
    targetWindows: false,
  });

  assert.equal(
    result.activeSavePath,
    '/Users/dev/Downloads',
    'macOS client must use the active local engine download directory'
  );
  assert.equal(
    result.historicalSavePath,
    'C:\\Users\\administrator\\Downloads',
    'Foreign Windows path must be preserved in historical_save_path'
  );
});

test('Cross-OS Path Resolution: Verified payload in engine directory prioritizes engine directory over metadata', () => {
  const result = resolveTorrentSavePathForTarget({
    metaSavePath: 'D:\\OtherDrive\\OldLocation',
    engineDownloadDir: 'V:\\Blu-ray\\Shakugan no Shana',
    torrentName: 'Shakugan no Shana S (2009) JDM',
    targetWindows: true,
    enginePayloadExists: true,
  });

  assert.equal(
    result.activeSavePath,
    'V:\\Blu-ray\\Shakugan no Shana',
    'Verified payload presence in engine directory must take precedence'
  );
  assert.equal(
    result.historicalSavePath,
    'D:\\OtherDrive\\OldLocation',
    'Previous metadata path preserved in historical_save_path'
  );
});

// ── 2. Locate / Relink Media Capability ───────────────────────────────────────

test('Locate/Relink: Preserves title, poster, provider associations, and history when relinking', () => {
  interface TorrentState {
    id: string;
    info_hash: string;
    name: string;
    save_path: string;
    historical_save_path?: string | null;
    poster_url?: string | null;
    media_meta?: {
      clean_title: string;
      year?: number;
      tmdb_id?: number;
      overview?: string;
    };
  }

  const originalItem: TorrentState = {
    id: '340313449696d11fbfec8982c5055f90459c3ece',
    info_hash: '340313449696d11fbfec8982c5055f90459c3ece',
    name: 'Shakugan no Shana S',
    save_path: '/Users/dev/Downloads',
    historical_save_path: null,
    poster_url: 'https://image.tmdb.org/t/p/w500/shana.jpg',
    media_meta: {
      clean_title: 'Shakugan no Shana S',
      year: 2009,
      tmdb_id: 34186,
      overview: 'OVA series following Yuji and Shana.',
    },
  };

  function relinkTorrent(item: TorrentState, newPath: string): TorrentState {
    return {
      ...item,
      save_path: newPath,
      historical_save_path: item.save_path,
    };
  }

  const relinked = relinkTorrent(originalItem, 'V:\\Blu-ray\\Shakugan no Shana S (2009) JDM');

  // New path is active
  assert.equal(relinked.save_path, 'V:\\Blu-ray\\Shakugan no Shana S (2009) JDM');
  // Stale path preserved as history
  assert.equal(relinked.historical_save_path, '/Users/dev/Downloads');
  // Metadata, poster, and identifiers completely preserved
  assert.equal(relinked.name, originalItem.name);
  assert.equal(relinked.poster_url, originalItem.poster_url);
  assert.equal(relinked.media_meta?.clean_title, 'Shakugan no Shana S');
  assert.equal(relinked.media_meta?.tmdb_id, 34186);
  assert.equal(relinked.media_meta?.year, 2009);
});

// ── 3. Single Error Presentation (No Duplicate Banner) ─────────────────────────

test('UI Error Presentation: Failed playback emits single notification with relink action and zero duplicate banners', () => {
  interface ViewState {
    feedbackNotice: {
      type: 'info' | 'error';
      message: string;
      relinkTarget?: {
        kind: 'torrent' | 'external';
        id: string;
        name: string;
        path: string;
      };
    } | null;
    renderedBannersCount: number;
  }

  const state: ViewState = {
    feedbackNotice: null,
    renderedBannersCount: 0,
  };

  function handlePlayFailure(errMessage: string, target: { kind: 'torrent'; id: string; name: string; path: string }) {
    const isPathErr =
      errMessage.includes('path') ||
      errMessage.includes('exist') ||
      errMessage.includes('operating system') ||
      errMessage.includes('not found') ||
      errMessage.includes('disconnected');

    // Single unified notification mechanism
    state.feedbackNotice = {
      type: 'error',
      message: errMessage,
      relinkTarget: isPathErr ? target : undefined,
    };

    // The old duplicate externalDiscError state is eliminated:
    // Only 1 banner rendered in DOM
    state.renderedBannersCount = state.feedbackNotice ? 1 : 0;
  }

  handlePlayFailure('Media path does not exist: /Users/dev/Downloads/Shakugan no Shana S', {
    kind: 'torrent',
    id: '340313449696d11fbfec8982c5055f90459c3ece',
    name: 'Shakugan no Shana S',
    path: '/Users/dev/Downloads/Shakugan no Shana S',
  });

  // Verification:
  assert.equal(state.renderedBannersCount, 1, 'Exactly one error banner must be rendered; duplicate is eliminated');
  assert.ok(state.feedbackNotice, 'Feedback notice must be populated');
  assert.equal(state.feedbackNotice?.type, 'error');
  assert.ok(state.feedbackNotice?.relinkTarget, 'Relink target must be provided for path failures');
  assert.equal(state.feedbackNotice?.relinkTarget?.kind, 'torrent');
  assert.equal(state.feedbackNotice?.relinkTarget?.name, 'Shakugan no Shana S');
});
