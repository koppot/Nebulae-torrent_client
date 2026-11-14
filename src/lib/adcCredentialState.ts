import type { AdcTrackerProfile } from '../types';

export type TrackerCredentialView = 'link' | 'recovery' | 'dashboard';

export function getTrackerCredentialView(
  profile: AdcTrackerProfile | null,
  isRelinking: boolean,
): TrackerCredentialView {
  if (isRelinking || (!profile?.is_linked && !profile?.account_configured)) {
    return 'link';
  }
  if (profile?.account_configured && !profile.is_linked) {
    return 'recovery';
  }
  return 'dashboard';
}

export function canUnlockTrackerCredential(
  status: AdcTrackerProfile['credential_status'],
): boolean {
  return (
    status === 'credential_access_denied' ||
    status === 'credential_storage_unavailable' ||
    status === 'credential_storage_error'
  );
}

export function getTrackerCredentialMessage(
  status: AdcTrackerProfile['credential_status'],
): string {
  switch (status) {
    case 'credential_required':
      return 'The account is configured, but its Keychain credential is missing. Relink the account to restore ADC features.';
    case 'credential_access_denied':
      return 'Keychain access was denied or needs authorization. Choose Unlock Keychain or relink the account.';
    case 'credential_storage_unavailable':
    case 'credential_storage_error':
      return 'The secure credential store is unavailable. Retry Unlock Keychain or relink the account.';
    default:
      return 'The ADC credential is not available.';
  }
}
