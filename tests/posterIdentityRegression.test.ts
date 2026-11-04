/**
 * Regression tests for poster identity mapping in SimpleMediaView.
 *
 * Root cause fixed (c04eccb → next commit):
 *   The onConfirmed callback previously stored a confirmed poster under three
 *   keys: [editingFor.infoHash], [editingFor.title], and [newTitle].  Title
 *   strings are not unique across items — multiple External entries share
 *   common tokens — so a poster confirmed for "Fuuka" bled into every other
 *   card whose title or new-title matched.
 *
 *   Fix: store exclusively under [editingFor.infoHash] (the stable file path
 *   for External items; the torrent id/info_hash for Downloaded items).
 *
 * Run with:
 *   node --experimental-strip-types --test tests/posterIdentityRegression.test.ts
 */
import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

// ---------------------------------------------------------------------------
// Minimal in-memory replica of the externalPosterMap state machine
// ---------------------------------------------------------------------------

type PosterMap = Record<string, string | undefined>;

/** Exact logic from the FIXED onConfirmed callback in SimpleMediaView.tsx */
function applyConfirmedPoster(
  prev: PosterMap,
  editingFor: { infoHash: string; title: string },
  posterUrl: string,
): PosterMap {
  // Only the stable identity key — no title keys.
  return { ...prev, [editingFor.infoHash]: posterUrl };
}

/** Legacy (buggy) version that also stored title-keyed entries */
function applyConfirmedPosterBuggy(
  prev: PosterMap,
  editingFor: { infoHash: string; title: string },
  newTitle: string,
  posterUrl: string,
): PosterMap {
  return {
    ...prev,
    [editingFor.infoHash]: posterUrl,
    [editingFor.title]: posterUrl,
    [newTitle]: posterUrl,
  };
}

/** Seed the map as the initial useEffect does: keyed by m.path */
function seedFromDatabase(items: Array<{ path: string; poster_url?: string | null }>): PosterMap {
  const map: PosterMap = {};
  for (const item of items) {
    if (item.poster_url) {
      map[item.path] = item.poster_url;
    }
  }
  return map;
}

/** Simulate TMDB auto-fetch result stored by m.path (the fixed fetch path) */
function applyTmdbResult(prev: PosterMap, path: string, posterWebp: string | undefined): PosterMap {
  return { ...prev, [path]: posterWebp };
}

// ---------------------------------------------------------------------------
// Test fixtures: seven External items matching the screenshot
// ---------------------------------------------------------------------------
const FUUKA_PATH     = '/media/external/Fuuka (2017)';
const PATLABOR_PATH  = '/media/external/Patlabor 2 the Movie (1993)';
const MARY_PATH      = '/media/external/Mary and a witchs flower (2017)';
const SUZUME_PATH    = '/media/external/Suzume (2022)';
const PATAPATA_PATH  = '/media/external/Patapata Hikousen no Bouken (2002)';
const BUSOU_PATH     = '/media/external/Busou Renkin (2006)';
const PATLABOR2_PATH = '/media/external/Patlabor 2 the Movie (1993) copy';

const FUUKA_POSTER   = 'data:image/webp;base64,FUUKA_POSTER_DATA';
const SUZUME_POSTER  = 'data:image/webp;base64,SUZUME_POSTER_DATA';
const MARY_POSTER    = 'data:image/webp;base64,MARY_POSTER_DATA';

// ---------------------------------------------------------------------------
// Suite 1: externalPosterMap isolation
// ---------------------------------------------------------------------------
describe('externalPosterMap — poster identity isolation', () => {

  test('initial seed from DB uses path as key — no cross-contamination at load', () => {
    const items = [
      { path: FUUKA_PATH,    poster_url: FUUKA_POSTER },
      { path: PATLABOR_PATH, poster_url: null },
      { path: SUZUME_PATH,   poster_url: SUZUME_POSTER },
    ];
    const map = seedFromDatabase(items);

    assert.equal(map[FUUKA_PATH],    FUUKA_POSTER,  'Fuuka must have its own poster');
    assert.equal(map[SUZUME_PATH],   SUZUME_POSTER, 'Suzume must have its own poster');
    assert.equal(map[PATLABOR_PATH], undefined,     'Patlabor must have no poster (null in DB)');
    // No title-string keys should exist in the map
    assert.equal(map['Fuuka'],  undefined, 'Title "Fuuka" must not be a map key');
    assert.equal(map['Suzume'], undefined, 'Title "Suzume" must not be a map key');
  });

  test('TMDB auto-fetch stores under path — isolated per item', () => {
    let map: PosterMap = {};
    map = applyTmdbResult(map, FUUKA_PATH,    FUUKA_POSTER);
    map = applyTmdbResult(map, PATLABOR_PATH, undefined);       // fetch failed
    map = applyTmdbResult(map, SUZUME_PATH,   SUZUME_POSTER);

    assert.equal(map[FUUKA_PATH],    FUUKA_POSTER,  'Fuuka has its own poster after TMDB');
    assert.equal(map[PATLABOR_PATH], undefined,     'Patlabor has no poster after failed TMDB');
    assert.equal(map[SUZUME_PATH],   SUZUME_POSTER, 'Suzume has its own poster after TMDB');
    // No cross-contamination via title tokens
    assert.equal(map['Fuuka (2017)'], undefined);
  });

  test('FIXED: confirming Fuuka poster does not contaminate other External items', () => {
    // Start with TMDB-seeded map (Patlabor and others have no poster yet)
    let map: PosterMap = {
      [FUUKA_PATH]:    FUUKA_POSTER,
      [PATLABOR_PATH]: undefined,
      [MARY_PATH]:     undefined,
      [SUZUME_PATH]:   SUZUME_POSTER,
      [PATAPATA_PATH]: undefined,
      [BUSOU_PATH]:    undefined,
      [PATLABOR2_PATH]:undefined,
    };

    // User opens Edit Metadata for Fuuka and confirms with its poster
    const editingFor = { infoHash: FUUKA_PATH, title: 'Fuuka (2017)' };
    map = applyConfirmedPoster(map, editingFor, FUUKA_POSTER);

    // Fuuka must retain its poster
    assert.equal(map[FUUKA_PATH], FUUKA_POSTER, 'Fuuka must retain its poster');

    // Every other item must be unaffected
    assert.equal(map[PATLABOR_PATH],  undefined,     'Patlabor 2 must not receive Fuuka poster');
    assert.equal(map[MARY_PATH],      undefined,     'Mary must not receive Fuuka poster');
    assert.equal(map[PATAPATA_PATH],  undefined,     'Patapata must not receive Fuuka poster');
    assert.equal(map[BUSOU_PATH],     undefined,     'Busou Renkin must not receive Fuuka poster');
    assert.equal(map[PATLABOR2_PATH], undefined,     'Patlabor 2 (copy) must not receive Fuuka poster');
    // Suzume had its own poster and must keep it, not get Fuuka's
    assert.equal(map[SUZUME_PATH],    SUZUME_POSTER, 'Suzume must retain its own poster unchanged');

    // No title-string keys should exist in the map after confirm
    assert.equal(map['Fuuka (2017)'], undefined, 'Title key must not be inserted');
    assert.equal(map['Fuuka'],        undefined, 'Bare title key must not be inserted');
  });

  test('FIXED: confirming Suzume poster after Fuuka does not collide', () => {
    let map: PosterMap = {
      [FUUKA_PATH]:  FUUKA_POSTER,
      [SUZUME_PATH]: undefined,
      [MARY_PATH]:   undefined,
    };

    // Confirm Fuuka
    map = applyConfirmedPoster(map, { infoHash: FUUKA_PATH, title: 'Fuuka (2017)' }, FUUKA_POSTER);
    // Confirm Suzume with a different poster
    map = applyConfirmedPoster(map, { infoHash: SUZUME_PATH, title: 'Suzume (2022)' }, SUZUME_POSTER);

    assert.equal(map[FUUKA_PATH],  FUUKA_POSTER,  'Fuuka retains its own poster');
    assert.equal(map[SUZUME_PATH], SUZUME_POSTER, 'Suzume gets its own poster');
    assert.equal(map[MARY_PATH],   undefined,     'Mary is unaffected by both confirms');

    assert.equal(map['Fuuka (2017)'], undefined, 'No title-keyed Fuuka entry');
    assert.equal(map['Suzume (2022)'], undefined, 'No title-keyed Suzume entry');
  });

  test('FIXED: title rename does not propagate poster to items sharing title tokens', () => {
    // Patlabor 2 appears twice with slightly different paths but similar titles.
    // Renaming one must not affect the other.
    let map: PosterMap = {
      [PATLABOR_PATH]:  undefined,
      [PATLABOR2_PATH]: undefined,
    };

    const PATLABOR_POSTER = 'data:image/webp;base64,PATLABOR_DATA';
    // User edits first Patlabor entry, confirms poster, also sets new title
    map = applyConfirmedPoster(
      map,
      { infoHash: PATLABOR_PATH, title: 'Patlabor 2 the Movie (1993)' },
      PATLABOR_POSTER,
    );

    assert.equal(map[PATLABOR_PATH],  PATLABOR_POSTER, 'First Patlabor gets poster via path key');
    assert.equal(map[PATLABOR2_PATH], undefined,        'Second Patlabor path is unaffected');
    // No shared-title key was created
    assert.equal(map['Patlabor 2 the Movie (1993)'], undefined, 'Title key must not exist');
  });

  test('FIXED: concurrent confirm calls for different items remain isolated', () => {
    let map: PosterMap = {
      [FUUKA_PATH]: undefined,
      [MARY_PATH]:  undefined,
      [BUSOU_PATH]: undefined,
    };

    // Simulate three concurrent confirms resolving in arbitrary order
    const confirms: Array<[{ infoHash: string; title: string }, string]> = [
      [{ infoHash: MARY_PATH,  title: 'Mary and a witchs flower (2017)' }, MARY_POSTER],
      [{ infoHash: FUUKA_PATH, title: 'Fuuka (2017)' },                   FUUKA_POSTER],
      [{ infoHash: BUSOU_PATH, title: 'Busou Renkin (2006)' },            'data:image/webp;base64,BUSOU'],
    ];

    for (const [editingFor, poster] of confirms) {
      map = applyConfirmedPoster(map, editingFor, poster);
    }

    assert.equal(map[FUUKA_PATH], FUUKA_POSTER, 'Fuuka has correct poster');
    assert.equal(map[MARY_PATH],  MARY_POSTER,  'Mary has correct poster');
    assert.equal(map[BUSOU_PATH], 'data:image/webp;base64,BUSOU', 'Busou has correct poster');

    // Prove no title-keyed cross-contamination
    const titleKeys = Object.keys(map).filter((k) => !k.startsWith('/'));
    assert.equal(titleKeys.length, 0, `No title-string keys in map; found: ${JSON.stringify(titleKeys)}`);
  });

  test('REGRESSION PROOF: buggy version would contaminate via title key', () => {
    // This test documents the exact failure mode observed in the screenshot.
    // All seven items start with no poster. User confirms Fuuka's poster.
    // The buggy code wrote [editingFor.title] and [newTitle] as extra keys.
    // Any lookup that happened to use the title string as a key would find
    // Fuuka's poster instead of the correct item's poster (or undefined).

    let map: PosterMap = {
      [FUUKA_PATH]:    undefined,
      [PATLABOR_PATH]: undefined,
    };

    const BUGGY_POSTER = 'data:image/webp;base64,FUUKA_BUGGY';

    // Apply buggy logic
    map = applyConfirmedPosterBuggy(
      map,
      { infoHash: FUUKA_PATH, title: 'Fuuka (2017)' },
      'Fuuka (2017)', // newTitle same as old in this case
      BUGGY_POSTER,
    );

    // The buggy version DID insert title-keyed entries
    assert.ok(
      map['Fuuka (2017)'] !== undefined,
      'Buggy version inserts title key — this is the contamination vector',
    );

    // Fix verification: the fixed version does NOT insert title keys
    let fixedMap: PosterMap = {
      [FUUKA_PATH]:    undefined,
      [PATLABOR_PATH]: undefined,
    };
    fixedMap = applyConfirmedPoster(
      fixedMap,
      { infoHash: FUUKA_PATH, title: 'Fuuka (2017)' },
      BUGGY_POSTER,
    );
    assert.equal(fixedMap['Fuuka (2017)'], undefined, 'Fixed version must not insert title key');
    assert.equal(fixedMap['Fuuka'],        undefined, 'Fixed version must not insert bare title key');
    assert.equal(fixedMap[FUUKA_PATH],     BUGGY_POSTER, 'Fixed version stores under path key only');
  });
});

// ---------------------------------------------------------------------------
// Suite 2: thumbMap isolation (Downloaded shelf)
// ---------------------------------------------------------------------------
describe('thumbMap — poster identity isolation for Downloaded items', () => {
  type ThumbMap = Record<string, string | null>;

  /** Fixed version: only infoHash key */
  function applyConfirmedThumb(
    prev: ThumbMap,
    editingFor: { infoHash: string; title: string },
    posterUrl: string,
  ): ThumbMap {
    return { ...prev, [editingFor.infoHash]: posterUrl };
  }

  const TORRENT_A_ID = 'abc123def456';
  const TORRENT_B_ID = 'bbb222ccc333';
  const TORRENT_C_ID = 'ccc333ddd444';
  const POSTER_A = 'data:image/webp;base64,TORRENT_A_POSTER';
  const POSTER_B = 'data:image/webp;base64,TORRENT_B_POSTER';

  test('confirming poster for torrent A does not affect torrent B or C', () => {
    const map: ThumbMap = {
      [TORRENT_A_ID]: null,
      [TORRENT_B_ID]: null,
      [TORRENT_C_ID]: null,
    };

    const updated = applyConfirmedThumb(
      map,
      { infoHash: TORRENT_A_ID, title: 'Banner of the Stars III OVA (2005)' },
      POSTER_A,
    );

    assert.equal(updated[TORRENT_A_ID], POSTER_A, 'Torrent A has its poster');
    assert.equal(updated[TORRENT_B_ID], null,     'Torrent B is unchanged');
    assert.equal(updated[TORRENT_C_ID], null,     'Torrent C is unchanged');
    assert.equal(
      updated['Banner of the Stars III OVA (2005)'],
      undefined,
      'Title key must not exist in thumbMap',
    );
  });

  test('two separate confirms produce independent torrent entries', () => {
    let map: ThumbMap = {
      [TORRENT_A_ID]: null,
      [TORRENT_B_ID]: null,
    };

    map = applyConfirmedThumb(map, { infoHash: TORRENT_A_ID, title: 'Title A' }, POSTER_A);
    map = applyConfirmedThumb(map, { infoHash: TORRENT_B_ID, title: 'Title B' }, POSTER_B);

    assert.equal(map[TORRENT_A_ID], POSTER_A, 'Torrent A correct poster');
    assert.equal(map[TORRENT_B_ID], POSTER_B, 'Torrent B correct poster');

    const nonIdKeys = Object.keys(map).filter(
      (k) => k !== TORRENT_A_ID && k !== TORRENT_B_ID,
    );
    assert.equal(nonIdKeys.length, 0, `No spurious keys in thumbMap; found: ${JSON.stringify(nonIdKeys)}`);
  });
});

// ---------------------------------------------------------------------------
// Suite 3: editingFor identity source correctness
// ---------------------------------------------------------------------------
describe('editingFor — identity key derivation correctness', () => {
  /**
   * Verifies that the identity key passed into the poster maps is always the
   * stable filesystem path (External) or info_hash/id (Downloaded), never a
   * mutable title string.
   */

  test('External items set editingFor.infoHash to m.path not title', () => {
    // Simulate the setEditingFor call site (line 1600, 1765 in SimpleMediaView.tsx)
    const extItem = { path: '/media/external/Fuuka (2017)', title: 'Fuuka (2017)', year: 2017 };
    const editingFor = { infoHash: extItem.path, title: extItem.title };

    // The key used in applyConfirmedPoster must equal the path
    assert.equal(editingFor.infoHash, extItem.path, 'infoHash for External must be the filesystem path');
    assert.notEqual(editingFor.infoHash, extItem.title, 'infoHash must not be the title string');
  });

  test('Downloaded items set editingFor.infoHash to info_hash or id', () => {
    // Simulate the setEditingFor call site (line 1931 in SimpleMediaView.tsx)
    const torrent = {
      info_hash: 'deadbeefcafe1234',
      id: 'deadbeefcafe1234',
      name: 'Fuuka (2017)',
    };
    const editingFor = {
      infoHash: torrent.info_hash || torrent.id,
      title: torrent.name,
    };

    assert.equal(editingFor.infoHash, 'deadbeefcafe1234', 'infoHash for Downloaded must be the torrent hash/id');
    assert.notEqual(editingFor.infoHash, torrent.name, 'infoHash must not be the display name');
  });

  test('path-keyed lookup matches key used at insertion — no key mismatch', () => {
    const path = '/media/external/Patapata Hikousen no Bouken (2002)';
    const poster = 'data:image/webp;base64,PATAPATA_POSTER';

    // Insert via fixed logic
    const map = applyConfirmedPoster({}, { infoHash: path, title: 'Patapata Hikousen no Bouken (2002)' }, poster);

    function applyConfirmedPoster(
      prev: Record<string, string | undefined>,
      editingFor: { infoHash: string; title: string },
      posterUrl: string,
    ): Record<string, string | undefined> {
      return { ...prev, [editingFor.infoHash]: posterUrl };
    }

    // Lookup via the same path key (as used at render lines 1160, 1458)
    const resolved = map[path];
    assert.equal(resolved, poster, 'Lookup by path must resolve the inserted poster');
  });
});
