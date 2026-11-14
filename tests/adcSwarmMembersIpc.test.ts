/**
 * Mocked frontend IPC-path tests. These exercise the production helper used by
 * PeersTabPanel, with a controlled invoke function and synthetic responses.
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
  lookupSwarmMembers,
  shouldCacheSwarmMembers,
  toSwarmMemberMap,
} from '../src/utils/adcSwarmMembersIpc.ts';

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

    // PeersTabPanel increments adcRetry, uses the changed fetch key, then
    // calls this same production helper again.
    const retried = await lookupSwarmMembers(INFO_HASH, PEERS, invoke);
    assert.equal(retried.kind, 'result');
    if (retried.kind === 'result') assert.equal(retried.result.status, 'no_matching_peers');
    assert.equal(calls.length, 2);
    assert.deepEqual(calls[0], calls[1]);
  });

  test('matched results populate the peer-address lookup used by rendering', async () => {
    const member = {
      ip: '203.0.113.17',
      port: 51413,
      username: 'synthetic-member',
      user_id: 7,
      profile_url: 'https://example.invalid/profile/7',
      ratio: 1.5,
      uploaded_bytes: 1000,
      downloaded_bytes: 500,
      bonus_points: 2,
    };
    const members = toSwarmMemberMap([member]);
    assert.equal(members.get('203.0.113.17:51413')?.username, 'synthetic-member');
    assert.equal(shouldCacheSwarmMembers('matched'), true);
  });
});
