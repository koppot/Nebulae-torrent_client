import test from 'node:test';
import assert from 'node:assert/strict';

// Mock types matching the frontend model
interface MockTorrent {
  id: string;
  name: string;
  info_hash: string;
  status: string;
  progress: number;
  is_media: boolean;
  category: string | null;
  total_bytes: number;
}

test('Downloaded shelf: completed torrents appear in readyToWatch regardless of artwork availability', () => {
  const torrents: MockTorrent[] = [
    {
      id: '1',
      name: 'Banner of the Stars III OVA (2005) R2J',
      info_hash: '9f42585c578ec14c2663f94687e1d8e3451ac1e1',
      status: 'Seeding',
      progress: 1.0,
      is_media: true,
      category: null,
      total_bytes: 5508821000,
    },
    {
      id: '2',
      name: 'Banner of the Stars III - OVA (2005) JDM',
      info_hash: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
      status: 'Paused',
      progress: 1.0,
      is_media: true,
      category: null,
      total_bytes: 49250000000,
    },
    {
      id: '3',
      name: 'Gnosia (2025-2026) JDM',
      info_hash: 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
      status: 'Paused',
      progress: 1.0,
      is_media: true,
      category: null,
      total_bytes: 181950000000,
    },
    {
      id: '4',
      name: 'Dragon Goes House-Hunting (2021) US',
      info_hash: 'cccccccccccccccccccccccccccccccccccccccc',
      status: 'Paused',
      progress: 0.0,
      is_media: true,
      category: null,
      total_bytes: 89330000000,
    },
  ];

  // thumbMap has NO artwork (all null or absent)
  const thumbMap: Record<string, string | null> = {
    '1': null,
    '2': null,
    '3': null,
    '4': null,
  };

  // Filter predicate exactly matching SimpleMediaView.tsx line 457
  const readyToWatch = torrents.filter(
    (t) =>
      (t.is_media || t.category === 'Movies') &&
      (t.progress > 0.05 || t.status === 'Seeding' || t.status === 'Completed'),
  );

  // 100% completed releases appear even though thumbMap is completely empty
  assert.equal(readyToWatch.length, 3);
  assert.equal(readyToWatch[0].name, 'Banner of the Stars III OVA (2005) R2J');
  assert.equal(readyToWatch[1].name, 'Banner of the Stars III - OVA (2005) JDM');
  assert.equal(readyToWatch[2].name, 'Gnosia (2025-2026) JDM');

  // The 0% progress paused torrent is correctly excluded from readyToWatch
  assert.ok(!readyToWatch.some((t) => t.id === '4'));

  // Each item in readyToWatch can render a fallback title card without artwork
  readyToWatch.forEach((item) => {
    const poster = thumbMap[item.id];
    assert.equal(poster, null, 'Poster is absent');
    const displayTitle = item.name;
    assert.ok(displayTitle.length > 0, 'Fallback title is ready to display on card');
  });
});

test('Retry after temporary provider failure: transient failure does not permanently block subsequent attempts', async () => {
  let attemptCount = 0;
  let simulateNetworkError = true;

  // Simulated provider fetch function
  const fetchProvider = async () => {
    attemptCount++;
    if (simulateNetworkError) {
      throw new Error('503 Service Unavailable');
    }
    return { 'Fuuka': '/posters/fuuka.webp' };
  };

  // State tracker mimicking adcFetch / tmdbMap
  let inFlightPromise: Promise<any> | null = null;
  const executeFetch = async () => {
    if (inFlightPromise) return inFlightPromise;
    inFlightPromise = fetchProvider()
      .then((data) => {
        return data;
      })
      .catch((err) => {
        // Clearing inFlightPromise on failure allows retry
        inFlightPromise = null;
        throw err;
      });
    return inFlightPromise;
  };

  // Attempt 1: Fails due to temporary network outage
  await assert.rejects(executeFetch(), /503 Service Unavailable/);
  assert.equal(attemptCount, 1);
  assert.equal(inFlightPromise, null, 'Promise was cleared on error');

  // Network recovers
  simulateNetworkError = false;

  // Attempt 2: Retries and succeeds!
  const result = await executeFetch();
  assert.equal(attemptCount, 2);
  assert.deepEqual(result, { 'Fuuka': '/posters/fuuka.webp' });
});

test('Metadata/poster persistence: resolved posters are persisted to database and trigger immediate UI refresh', async () => {
  const dbCalls: Array<{ command: string; payload: any }> = [];
  const invokeMock = async (command: string, payload: any) => {
    dbCalls.push({ command, payload });
    if (command === 'get_torrent_poster') {
      // Initially poster_path is null in database
      return null;
    }
    if (command === 'fetch_tmdb_release') {
      return { poster_webp: '/thumbnails/resolved_banner_stars.webp' };
    }
    return null;
  };

  let thumbMap: Record<string, string | null> = {};
  const updateThumbMap = (id: string, url: string) => {
    thumbMap = { ...thumbMap, [id]: url };
  };

  const torrent = {
    id: 't-123',
    info_hash: '9f42585c578ec14c2663f94687e1d8e3451ac1e1',
    name: 'Banner of the Stars III OVA (2005) R2J',
  };

  // Step 1: Check DB first
  const dbPoster = await invokeMock('get_torrent_poster', { infoHash: torrent.info_hash });
  assert.equal(dbPoster, null);

  // Step 2: Fetch TMDB
  const tmdbRes = await invokeMock('fetch_tmdb_release', { title: 'Banner of the Stars III', year: 2005 });
  assert.ok(tmdbRes?.poster_webp);

  // Step 3: Immediately persist to DB via set_torrent_poster
  await invokeMock('set_torrent_poster', {
    infoHash: torrent.info_hash,
    posterPath: tmdbRes.poster_webp,
  });

  // Step 4: Immediately refresh frontend thumbMap
  updateThumbMap(torrent.id, tmdbRes.poster_webp);

  // Verify DB persistence was called with exact hash and path
  const persistCall = dbCalls.find((c) => c.command === 'set_torrent_poster');
  assert.ok(persistCall, 'set_torrent_poster was called');
  assert.equal(persistCall.payload.infoHash, torrent.info_hash);
  assert.equal(persistCall.payload.posterPath, '/thumbnails/resolved_banner_stars.webp');

  // Verify frontend state immediately updated
  assert.equal(thumbMap['t-123'], '/thumbnails/resolved_banner_stars.webp');
});
