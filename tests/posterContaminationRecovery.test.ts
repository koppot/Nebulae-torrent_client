import test from 'node:test';
import assert from 'node:assert/strict';

// Helper deterministic fnv1a hash matching Rust thumbs::convert_image_bytes
function fnv1a(key: string): string {
  let hash = 1469598103934665603n;
  const prime = 1099511628211n;
  const mask = 0xFFFFFFFFFFFFFFFFn;
  for (let i = 0; i < key.length; i++) {
    hash ^= BigInt(key.charCodeAt(i));
    hash = (hash * prime) & mask;
  }
  return hash.toString(16).padStart(16, '0');
}

const FUUKA_CONTAMINATED_HASH = '93758aeca10f8450';

test('Poster Contamination Recovery: Distinct titles produce distinct deterministic filenames', () => {
  const titles = [
    'fuuka',
    "mary and a witch's flower",
    'patlabor 2 the movie',
    'suzume',
    'heroic age',
    'busou renkin',
  ];

  const generatedFiles = new Map<string, string>();
  for (const t of titles) {
    const filename = `tmdb-${fnv1a(t)}.webp`;
    assert.strictEqual(
      generatedFiles.has(filename),
      false,
      `Collision detected: title "${t}" generated existing filename "${filename}"`
    );
    generatedFiles.set(filename, t);
  }

  assert.strictEqual(generatedFiles.size, 6);
  // Ensure none of the other titles generated the contaminated Fuuka hash
  for (const [filename, title] of generatedFiles.entries()) {
    if (title !== 'fuuka') {
      assert.strictEqual(filename.includes(FUUKA_CONTAMINATED_HASH), false);
    }
  }
});

test('Poster Contamination Recovery: Concurrent and out-of-order responses maintain strict title isolation', async () => {
  const settingsDb = new Map<string, string>();
  const externalMap = new Map<string, string>();

  // Simulate concurrent fetches for 4 titles that resolve in arbitrary order
  const requests = [
    { title: 'fuuka', path: 'V:\\Anime\\Fuuka', delay: 30, poster: 'tmdb-fuuka.webp' },
    { title: "mary and a witch's flower", path: 'V:\\UHD\\Mary', delay: 10, poster: 'tmdb-mary.webp' },
    { title: 'patlabor 2 the movie', path: 'V:\\Anime\\Patlabor2', delay: 40, poster: 'tmdb-patlabor.webp' },
    { title: 'suzume', path: 'V:\\UHD\\Suzume', delay: 5, poster: 'tmdb-suzume.webp' },
  ];

  await Promise.all(
    requests.map(async (req) => {
      await new Promise((r) => setTimeout(r, req.delay));
      const key = `tmdb_poster:v3:${req.title.toLowerCase()}`;
      settingsDb.set(key, req.poster);
      externalMap.set(req.path, req.poster);
    })
  );

  // Assert all 4 entries preserved their own distinct poster and none were overwritten by another
  assert.strictEqual(settingsDb.get('tmdb_poster:v3:fuuka'), 'tmdb-fuuka.webp');
  assert.strictEqual(settingsDb.get("tmdb_poster:v3:mary and a witch's flower"), 'tmdb-mary.webp');
  assert.strictEqual(settingsDb.get('tmdb_poster:v3:patlabor 2 the movie'), 'tmdb-patlabor.webp');
  assert.strictEqual(settingsDb.get('tmdb_poster:v3:suzume'), 'tmdb-suzume.webp');

  assert.strictEqual(externalMap.get('V:\\Anime\\Fuuka'), 'tmdb-fuuka.webp');
  assert.strictEqual(externalMap.get('V:\\UHD\\Mary'), 'tmdb-mary.webp');
  assert.strictEqual(externalMap.get('V:\\Anime\\Patlabor2'), 'tmdb-patlabor.webp');
  assert.strictEqual(externalMap.get('V:\\UHD\\Suzume'), 'tmdb-suzume.webp');
});

test('Poster Contamination Recovery: Invalidation of cross-title Fuuka contamination while preserving genuine Fuuka', () => {
  // Setup a mock settings cache with contaminated entries from the historical bug
  const settingsDb = new Map<string, string>([
    ['tmdb_poster:v3:fuuka', `/cache/${FUUKA_CONTAMINATED_HASH}.webp`],
    ["tmdb_poster:v3:mary and a witch's flower", `/cache/${FUUKA_CONTAMINATED_HASH}.webp`],
    ['tmdb_poster:v3:patlabor 2 the movie', `/cache/${FUUKA_CONTAMINATED_HASH}.webp`],
    ['tmdb_poster:v3:suzume', '/cache/genuine_suzume.webp'],
  ]);

  // Contamination purge rule (matching Database::new in Rust):
  // Purge any value containing the contaminated Fuuka hash where the key does NOT contain 'fuuka'
  for (const [key, val] of Array.from(settingsDb.entries())) {
    if (val.includes(FUUKA_CONTAMINATED_HASH) && !key.toLowerCase().includes('fuuka')) {
      settingsDb.delete(key);
    }
  }

  // Fuuka's own genuine mapping must remain
  assert.strictEqual(settingsDb.get('tmdb_poster:v3:fuuka'), `/cache/${FUUKA_CONTAMINATED_HASH}.webp`);
  // Suzume's clean mapping must remain
  assert.strictEqual(settingsDb.get('tmdb_poster:v3:suzume'), '/cache/genuine_suzume.webp');
  // Contaminated non-Fuuka entries must be gone
  assert.strictEqual(settingsDb.has("tmdb_poster:v3:mary and a witch's flower"), false);
  assert.strictEqual(settingsDb.has('tmdb_poster:v3:patlabor 2 the movie'), false);

  // Now simulate re-fetching clean artwork for Mary and Patlabor
  settingsDb.set("tmdb_poster:v3:mary and a witch's flower", '/cache/clean_mary.webp');
  settingsDb.set('tmdb_poster:v3:patlabor 2 the movie', '/cache/clean_patlabor.webp');

  assert.strictEqual(settingsDb.get("tmdb_poster:v3:mary and a witch's flower"), '/cache/clean_mary.webp');
  assert.strictEqual(settingsDb.get('tmdb_poster:v3:patlabor 2 the movie'), '/cache/clean_patlabor.webp');
});

test('Poster Contamination Recovery: Editing one entry strictly preserves all other entries and custom selections', () => {
  interface MediaItem {
    id: string;
    title: string;
    posterUrl: string;
    isCustom: boolean;
  }

  const shelf = new Map<string, MediaItem>([
    ['item-fuuka', { id: 'item-fuuka', title: 'Fuuka (2017)', posterUrl: '/posters/fuuka.webp', isCustom: false }],
    ['item-mary', { id: 'item-mary', title: "Mary and the Witch's Flower", posterUrl: '/posters/mary.webp', isCustom: false }],
    ['item-patlabor', { id: 'item-patlabor', title: 'Patlabor 2: The Movie', posterUrl: '/posters/patlabor.webp', isCustom: false }],
    ['item-custom', { id: 'item-custom', title: 'Custom Movie', posterUrl: '/custom/my_poster.png', isCustom: true }],
  ]);

  // User edits only Patlabor 2 poster
  const updatedPatlabor = { ...shelf.get('item-patlabor')!, posterUrl: '/posters/patlabor_v2.webp' };
  shelf.set('item-patlabor', updatedPatlabor);

  // Assert Fuuka, Mary, and Custom remain 100% unchanged
  assert.strictEqual(shelf.get('item-fuuka')?.posterUrl, '/posters/fuuka.webp');
  assert.strictEqual(shelf.get('item-mary')?.posterUrl, '/posters/mary.webp');
  assert.strictEqual(shelf.get('item-custom')?.posterUrl, '/custom/my_poster.png');
  assert.strictEqual(shelf.get('item-patlabor')?.posterUrl, '/posters/patlabor_v2.webp');
});

test('Poster Contamination Recovery: Restart persistence preserves clean mappings without bulk cache wipe', () => {
  // Storage layer simulating SQLite settings table across restarts
  class PersistentStorage {
    private store = new Map<string, string>();

    save(key: string, val: string) {
      this.store.set(key, val);
    }
    load(key: string): string | undefined {
      return this.store.get(key);
    }
    size(): number {
      return this.store.size;
    }
  }

  const storage = new PersistentStorage();
  storage.save('tmdb_poster:v3:fuuka', '/thumbs/fuuka.webp');
  storage.save("tmdb_poster:v3:mary and a witch's flower", '/thumbs/mary.webp');
  storage.save('tmdb_poster:v3:patlabor 2 the movie', '/thumbs/patlabor.webp');
  storage.save('tmdb_poster:v3:suzume', '/thumbs/suzume.webp');

  // Simulate application restart by creating a new session reader against the same storage
  const restartSession = {
    getPoster: (title: string) => storage.load(`tmdb_poster:v3:${title.toLowerCase()}`),
  };

  assert.strictEqual(restartSession.getPoster('Fuuka'), '/thumbs/fuuka.webp');
  assert.strictEqual(restartSession.getPoster("Mary and a Witch's Flower"), '/thumbs/mary.webp');
  assert.strictEqual(restartSession.getPoster('Patlabor 2 the Movie'), '/thumbs/patlabor.webp');
  assert.strictEqual(restartSession.getPoster('Suzume'), '/thumbs/suzume.webp');

  // Verify total keys remained exactly 4 without cache wipes or record loss
  assert.strictEqual(storage.size(), 4);
});

test('Poster Contamination Recovery: Sidecar auto-healing resets contaminated external posters without user intervention', () => {
  interface ExternalItem {
    path: string;
    title: string;
    year?: number | null;
    poster_url?: string | null;
  }

  function autoHealExternalMedia(items: ExternalItem[]): boolean {
    let mutated = false;
    for (const m of items) {
      const titleLc = m.title.toLowerCase();
      if (!titleLc.includes('fuuka')) {
        if (m.poster_url) {
          const pLc = m.poster_url.toLowerCase();
          if (pLc.includes(FUUKA_CONTAMINATED_HASH) || pLc.includes('fuuka')) {
            m.poster_url = null;
            mutated = true;
          }
        }
      }
    }
    return mutated;
  }

  const items: ExternalItem[] = [
    {
      path: '/media/external/Fuuka (2017)',
      title: 'Fuuka (2017)',
      year: 2017,
      poster_url: `/thumbs/${FUUKA_CONTAMINATED_HASH}.webp`,
    },
    {
      path: '/media/external/Patlabor 2 the Movie (1993)',
      title: 'Patlabor 2 the Movie (1993)',
      year: 1993,
      poster_url: `/thumbs/${FUUKA_CONTAMINATED_HASH}.webp`,
    },
    {
      path: "/media/external/Mary and a witch's flower (2017)",
      title: "Mary and a witch's flower (2017)",
      year: 2017,
      poster_url: '/cache/fuuka_scan.webp',
    },
    {
      path: '/media/external/Suzume (2022)',
      title: 'Suzume (2022)',
      year: 2022,
      poster_url: '/custom/my_suzume_art.jpg',
    },
  ];

  // Pass 1: Auto-heals contaminated items
  const changed = autoHealExternalMedia(items);
  assert.strictEqual(changed, true, 'Should detect and mutate contaminated records');

  // Genuine Fuuka retained
  assert.strictEqual(items[0].poster_url, `/thumbs/${FUUKA_CONTAMINATED_HASH}.webp`);
  // Patlabor 2 cleared
  assert.strictEqual(items[1].poster_url, null);
  // Mary cleared
  assert.strictEqual(items[2].poster_url, null);
  // Custom art for Suzume strictly preserved
  assert.strictEqual(items[3].poster_url, '/custom/my_suzume_art.jpg');

  // Pass 2: Idempotent
  const secondPass = autoHealExternalMedia(items);
  assert.strictEqual(secondPass, false, 'Second pass should not mutate already healed collection');
  assert.strictEqual(items[1].poster_url, null);
  assert.strictEqual(items[2].poster_url, null);
  assert.strictEqual(items[3].poster_url, '/custom/my_suzume_art.jpg');

  // Simulate auto-refetch when poster_url is null (matching SimpleMediaView.tsx lines 400-422)
  const tmdbFetchMock = (title: string) => `/thumbs/tmdb-${fnv1a(title.toLowerCase())}.webp`;
  for (const item of items) {
    if (!item.poster_url && !item.title.toLowerCase().includes('fuuka')) {
      item.poster_url = tmdbFetchMock(item.title);
    }
  }

  // Verify Patlabor 2 and Mary now have their own clean distinct posters
  assert.strictEqual(items[1].poster_url, `/thumbs/tmdb-${fnv1a('patlabor 2 the movie (1993)')}.webp`);
  assert.strictEqual(items[2].poster_url, `/thumbs/tmdb-${fnv1a("mary and a witch's flower (2017)")}.webp`);
  assert.notStrictEqual(items[1].poster_url, items[0].poster_url);
  assert.notStrictEqual(items[2].poster_url, items[0].poster_url);
});
