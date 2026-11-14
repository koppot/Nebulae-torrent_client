/**
 * Mocked frontend IPC-path tests and ADC Peers cache / continuous display tests.
 *
 * Run with:
 *   node --experimental-strip-types --test tests/adcSwarmMembersIpc.test.ts
 */
import assert from 'node:assert/strict';
import { describe, test } from 'node:test';
import {
  ADC_SWARM_MEMBERS_COMMAND,
  getAdcSwarmNotice,
  getSwarmMembersCacheKey,
  getSwarmMembersFetchKey,
  isValidAdcProfileUrl,
  lookupSwarmMembers,
  normalizeProfileUrl,
  shouldCacheSwarmMembers,
  toSwarmMemberMap,
} from '../src/utils/adcSwarmMembersIpc.ts';
import {
  clearAllSwarmCache,
  getCooldownRemaining,
  getResolvedSwarm,
  invalidateOnRevisionChange,
  registerCooldownListener,
  setAccountCooldown,
  storeSwarmResults,
} from '../src/utils/swarmMembersCache.ts';

const INFO_HASH = '340313449696d11fbfec8982c5055f90459c3ece';
const PEERS = [{ ip: '203.0.113.17', port: 51413 }];

const noMatches = {
  status: 'no_matching_peers',
  supported: true,
  matches: [],
  error: null,
};

const networkFailure = {
  status: 'network_error',
  supported: false,
  matches: [],
  error: 'network_error',
};

describe('Mocked frontend ADC swarm-members IPC path', () => {
  test('valid frontend invocation uses the canonical command and argument object', async () => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const invoke = async (command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      return noMatches;
    };

    const outcome = await lookupSwarmMembers(INFO_HASH, PEERS, invoke);

    assert.equal(outcome.kind, 'result');
    assert.deepEqual(calls, [{
      command: 'adc_swarm_members',
      args: { infoHash: INFO_HASH, peers: PEERS },
    }]);
    assert.equal(calls[0].command, ADC_SWARM_MEMBERS_COMMAND);
    assert.equal('info_hash' in calls[0].args, false);
  });

  test('IPC rejection becomes the internal-error state without exposing the error object', async () => {
    const outcome = await lookupSwarmMembers(INFO_HASH, PEERS, async () => {
      throw new Error('synthetic raw request details');
    });

    assert.deepEqual(outcome, { kind: 'ipc_error' });
    assert.equal(
      getAdcSwarmNotice('ipc_error'),
      'Internal error communicating with the peer-information service.',
    );
    assert.equal(shouldCacheSwarmMembers('ipc_error'), false);
  });

  test('transport failure is a resolved Rust status with the network-error message', async () => {
    const outcome = await lookupSwarmMembers(INFO_HASH, PEERS, async () => networkFailure);

    assert.deepEqual(outcome, { kind: 'result', result: networkFailure });
    assert.equal(
      getAdcSwarmNotice(networkFailure.status),
      'Could not reach the ADC peer-information service.',
    );
    assert.equal(shouldCacheSwarmMembers(networkFailure.status), false);
  });

  test('retry changes the production fetch key and invokes again after a failure', async () => {
    const cacheKey = getSwarmMembersCacheKey(INFO_HASH, 3, PEERS);
    const firstFetchKey = getSwarmMembersFetchKey(cacheKey, 0);
    const retryFetchKey = getSwarmMembersFetchKey(cacheKey, 1);
    assert.notEqual(retryFetchKey, firstFetchKey);

    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    let attempt = 0;
    const invoke = async (command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      attempt += 1;
      return attempt === 1 ? networkFailure : noMatches;
    };

    const failed = await lookupSwarmMembers(INFO_HASH, PEERS, invoke);
    assert.equal(failed.kind, 'result');
    if (failed.kind === 'result') {
      assert.equal(failed.result.status, 'network_error');
      assert.equal(shouldCacheSwarmMembers(failed.result.status), false);
    }

    const retried = await lookupSwarmMembers(INFO_HASH, PEERS, invoke);
    assert.equal(retried.kind, 'result');
    if (retried.kind === 'result') assert.equal(retried.result.status, 'no_matching_peers');
    assert.equal(calls.length, 2);
    assert.deepEqual(calls[0], calls[1]);
  });

  test('matched results populate the peer-address lookup and normalize profile URL', async () => {
    const member = {
      ip: '203.0.113.17',
      port: 51413,
      username: 'synthetic-member',
      user_id: 7,
      profile_url: 'https://asiandvdclub.org/user/7',
      ratio: 1.5,
      uploaded_bytes: 1000,
      downloaded_bytes: 500,
      bonus_points: 2,
    };
    const members = toSwarmMemberMap([member]);
    const resolved = members.get('203.0.113.17:51413');
    assert.equal(resolved?.username, 'synthetic-member');
    assert.equal(resolved?.profile_url, 'https://asiandvdclub.org/profile/7');
    assert.equal(shouldCacheSwarmMembers('matched'), true);
  });
});

describe('ADC Profile URL Normalization and Security Validation', () => {
  test('rewrites legacy /user/<id> to canonical /profile/<id>', () => {
    assert.equal(
      normalizeProfileUrl('https://asiandvdclub.org/user/42', 42),
      'https://asiandvdclub.org/profile/42',
    );
  });

  test('preserves valid canonical /profile/<id>', () => {
    assert.equal(
      normalizeProfileUrl('https://asiandvdclub.org/profile/108', 108),
      'https://asiandvdclub.org/profile/108',
    );
  });

  test('falls back to userId profile URL when raw URL is empty or malformed', () => {
    assert.equal(
      normalizeProfileUrl('', 77),
      'https://asiandvdclub.org/profile/77',
    );
    assert.equal(
      normalizeProfileUrl(null, 77),
      'https://asiandvdclub.org/profile/77',
    );
    assert.equal(
      normalizeProfileUrl('not-a-valid-url', 77),
      'https://asiandvdclub.org/profile/77',
    );
  });

  test('rejects external/arbitrary domains and returns fallback', () => {
    assert.equal(
      normalizeProfileUrl('https://evil.example.com/user/12', 12),
      'https://asiandvdclub.org/profile/12',
    );
  });

  test('validates strict HTTPS and asiandvdclub.org host for profile opening', () => {
    assert.equal(isValidAdcProfileUrl('https://asiandvdclub.org/profile/42'), true);
    assert.equal(isValidAdcProfileUrl('https://asiandvdclub.org/profile/99999'), true);
    assert.equal(isValidAdcProfileUrl('http://asiandvdclub.org/profile/42'), false); // HTTP rejected
    assert.equal(isValidAdcProfileUrl('https://attacker.org/profile/42'), false); // External host rejected
    assert.equal(isValidAdcProfileUrl('javascript:alert(1)'), false); // Protocol injection rejected
  });
});

describe('ADC Swarm Continuous Display and Rate-Limit Resilience', () => {
  test('previously resolved members are returned immediately from cache', () => {
    clearAllSwarmCache();
    const peer1 = { ip: '198.51.100.1', port: 6881 };
    const peer2 = { ip: '198.51.100.2', port: 6882 };

    const member1 = {
      ip: peer1.ip,
      port: peer1.port,
      username: 'Alice',
      user_id: 101,
      profile_url: 'https://asiandvdclub.org/user/101',
      ratio: 2.5,
      uploaded_bytes: 2000,
      downloaded_bytes: 800,
      bonus_points: 10,
    };

    storeSwarmResults(INFO_HASH, 1, [peer1, peer2], {
      status: 'matched',
      supported: true,
      matches: [member1],
      error: null,
    });

    // Lookup with same peers should immediately return member1 from cache
    const { cachedMap, unresolvedPeers } = getResolvedSwarm(INFO_HASH, 1, [peer1, peer2]);
    assert.equal(cachedMap.size, 1);
    assert.equal(cachedMap.get('198.51.100.1:6881')?.username, 'Alice');
    assert.equal(cachedMap.get('198.51.100.1:6881')?.profile_url, 'https://asiandvdclub.org/profile/101');

    // peer2 was negative-cached, so no unresolved peers remain
    assert.equal(unresolvedPeers.length, 0);
  });

  test('rate-limited response does NOT clear previously resolved members', () => {
    clearAllSwarmCache();
    const peer1 = { ip: '198.51.100.10', port: 5000 };
    const member = {
      ip: peer1.ip,
      port: peer1.port,
      username: 'Bob',
      user_id: 202,
      profile_url: 'https://asiandvdclub.org/profile/202',
      ratio: 1.2,
      uploaded_bytes: 500,
      downloaded_bytes: 400,
      bonus_points: 5,
    };

    // Initial successful resolution
    storeSwarmResults(INFO_HASH, 1, [peer1], {
      status: 'matched',
      supported: true,
      matches: [member],
      error: null,
    });

    // Simulate subsequent rate-limited result
    storeSwarmResults(INFO_HASH, 1, [peer1], {
      status: 'rate_limited',
      supported: true,
      matches: [],
      error: null,
    });

    // Cooldown is active
    assert.ok(getCooldownRemaining() > 0);

    // Member data is STILL preserved in cache
    const { cachedMap } = getResolvedSwarm(INFO_HASH, 1, [peer1]);
    assert.equal(cachedMap.size, 1);
    assert.equal(cachedMap.get('198.51.100.10:5000')?.username, 'Bob');
  });

  test('credential revision change invalidates prior cached identities', () => {
    clearAllSwarmCache();
    const peer1 = { ip: '198.51.100.20', port: 51413 };
    const member = {
      ip: peer1.ip,
      port: peer1.port,
      username: 'Carol',
      user_id: 303,
      profile_url: 'https://asiandvdclub.org/profile/303',
      ratio: 3.0,
      uploaded_bytes: 9000,
      downloaded_bytes: 3000,
      bonus_points: 50,
    };

    storeSwarmResults(INFO_HASH, 1, [peer1], {
      status: 'matched',
      supported: true,
      matches: [member],
      error: null,
    });

    // User switches account / updates credentials (revision becomes 2)
    invalidateOnRevisionChange(2);

    // Prior cached data under revision 1 is cleared to prevent cross-account identity leaks
    const { cachedMap, unresolvedPeers } = getResolvedSwarm(INFO_HASH, 2, [peer1]);
    assert.equal(cachedMap.size, 0);
    assert.equal(unresolvedPeers.length, 1);
  });

  test('cooldown registration notifies listener when cooldown expires', async () => {
    clearAllSwarmCache();
    let triggered = false;

    const unregister = registerCooldownListener(() => {
      triggered = true;
    });

    // Set a tiny cooldown of 20ms for testing
    setAccountCooldown(20);
    assert.ok(getCooldownRemaining() > 0);

    await new Promise((resolve) => setTimeout(resolve, 35));
    assert.equal(triggered, true);
    assert.equal(getCooldownRemaining(), 0);

    unregister();
  });
});
