import assert from 'node:assert/strict';
import { describe, test } from 'node:test';
import {
  canUnlockTrackerCredential,
  getTrackerCredentialMessage,
  getTrackerCredentialView,
} from '../src/lib/adcCredentialState.ts';
import type { AdcTrackerProfile } from '../src/types/tracker.ts';

function profile(
  credential_status: AdcTrackerProfile['credential_status'],
  account_configured = true,
): AdcTrackerProfile {
  return {
    username: 'synthetic-user',
    masked_passkey: '••••••',
    account_configured,
    credential_status,
    is_linked: credential_status === 'available',
    announce_status: '',
    ratio: 1,
    uploaded_bytes: 0,
    downloaded_bytes: 0,
    freeleech_active: false,
    bonus_points: 0,
    required_seed_time_hours: 72,
  };
}

describe('ADC credential recovery UI state', () => {
  test('routes missing and unconfigured accounts to linking', () => {
    assert.equal(getTrackerCredentialView(null, false), 'link');
    assert.equal(
      getTrackerCredentialView(profile('not_linked', false), false),
      'link',
    );
  });

  test('routes configured unavailable credentials to recovery and relinking to form', () => {
    const unavailable = profile('credential_access_denied');
    assert.equal(getTrackerCredentialView(unavailable, false), 'recovery');
    assert.equal(getTrackerCredentialView(unavailable, true), 'link');
  });

  test('lets a linked account replace a credential rejected by ADC', () => {
    const linked = profile('available');
    assert.equal(getTrackerCredentialView(linked, false), 'dashboard');
    assert.equal(getTrackerCredentialView(linked, true), 'link');
  });

  test('offers unlock only for credentials that may need authorization', () => {
    assert.equal(canUnlockTrackerCredential('credential_required'), false);
    assert.equal(canUnlockTrackerCredential('credential_access_denied'), true);
    assert.equal(canUnlockTrackerCredential('credential_storage_unavailable'), true);
    assert.equal(canUnlockTrackerCredential('credential_storage_error'), true);
    assert.equal(canUnlockTrackerCredential('available'), false);
  });

  test('shows distinct missing, denied, and unavailable guidance', () => {
    assert.match(
      getTrackerCredentialMessage('credential_required'),
      /credential is missing.*Relink/,
    );
    assert.match(
      getTrackerCredentialMessage('credential_access_denied'),
      /Unlock Keychain or relink/,
    );
    assert.match(
      getTrackerCredentialMessage('credential_storage_unavailable'),
      /secure credential store is unavailable/,
    );
  });
});
