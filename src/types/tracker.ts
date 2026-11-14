export interface AdcTrackerProfile {
  username: string;
  masked_passkey: string;
  account_configured: boolean;
  credential_status:
    | 'not_linked'
    | 'available'
    | 'credential_required'
    | 'credential_access_denied'
    | 'credential_storage_unavailable'
    | 'credential_storage_error';
  is_linked: boolean;
  last_announce_time?: number;
  next_announce_countdown_sec?: number;
  announce_status: string;
  ratio: number;
  uploaded_bytes: number;
  downloaded_bytes: number;
  freeleech_active: boolean;
  bonus_points: number;
  required_seed_time_hours: number;
}

export interface SeedTimeRecord {
  info_hash: string;
  torrent_name: string;
  seeding_time_seconds: number;
  required_seeding_seconds: number;
  is_shield_satisfied: boolean;
}
