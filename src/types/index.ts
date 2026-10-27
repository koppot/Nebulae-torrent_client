export type ViewMode = 'media' | 'manager' | 'compact';

// Drag-dropped File objects carry the absolute native path in a non-standard
// `path` property provided by the webview (Tauri), absent from the DOM type.
export type FileWithPath = File & { path?: string };

export interface TorrentFileItem {
  index: number;
  name: string;
  size_bytes: number;
  progress: number;
  priority: number; // 0: DoNotDownload, 1: Normal, 2: High, 3: Maximum
}

export interface TorrentTrackerItem {
  tier: number;
  url: string;
  status: string;
  // null = unknown (scrape unavailable/failed) - rendered as a dash
  seeds: number | null;
  peers: number | null;
  downloaded: number;
  message: string;
  // Unix timestamps (seconds). null = never announced / not yet scheduled.
  last_announce: number | null;
  next_announce: number | null;
}

export interface InspectTorrentResult {
  name: string;
  total_bytes: number;
  piece_size: number;
  total_pieces: number;
  info_hash: string;
  is_private: boolean;
  comment: string;
  created_by: string;
  files: TorrentFileItem[];
  trackers: TorrentTrackerItem[];
  metainfo_bytes?: number[];
}

export interface TorrentContentFileMatch {
  index: number;
  expected_rel_path: string;
  size_bytes: number;
  matched_rel_path: string | null;
}

export interface TorrentContentMatch {
  torrent_name: string;
  is_multi_file: boolean;
  files: TorrentContentFileMatch[];
  matched_count: number;
}

export interface TorrentRenameEntry {
  from_rel: string;
  to_rel: string;
}

export interface ParsedMediaMetadata {
  raw_title: string;
  clean_title: string;
  year?: number;
  resolution?: string;
  source?: string;
  codec?: string;
  audio?: string;
  season?: number;
  episode?: number;
  edition?: string;
  release_group?: string;
}

export type DiscType = 'DvdVideo' | 'DvdAudio' | 'BluRay' | 'UhdBluRay' | 'IsoImage' | 'Unknown';
export type DiscLayerSpec = 'Dvd5' | 'Dvd9' | 'Bd25' | 'Bd50' | 'UhdBd66' | 'UhdBd100' | 'Unknown';

export interface DiscStructureInfo {
  disc_type: DiscType;
  layer_spec: DiscLayerSpec;
  has_menu: boolean;
  root_path: string;
  play_target: string;
  cover_image?: string | null;
  total_bytes: number;
  title: string;
  disc_number?: number | null;
  total_discs?: number | null;
}

export interface VolumeSpaceInfo {
  mount_point: string;
  total_bytes: number;
  available_bytes: number;
  used_bytes: number;
  used_percent: number;
}

export interface CacheCleanupReport {
  scanned_directories: string[];
  files_scanned: number;
  files_removed: number;
  bytes_reclaimed: number;
  dry_run: boolean;
}

export interface OrganizeMediaReport {
  source_path: string;
  destination_path: string;
  files_moved: number;
  bytes_moved: number;
  success: boolean;
  message: string;
}

export interface ThumbnailResult {
  thumbnail_path?: string | null;
  is_disc_artwork: boolean;
  source: string;
}

export interface TorrentItem {
  id: string;
  name: string;
  total_bytes: number;
  downloaded_bytes: number;
  uploaded_bytes: number;
  download_rate: number;
  upload_rate: number;
  // Raw instantaneous engine-reported rates, preserved separately from the
  // since-transfer-began averages for hover tooltips.
  current_download_rate?: number;
  current_upload_rate?: number;
  // Cumulative ACTIVE seconds per direction (Transmission secondsDownloading /
  // secondsSeeding). Denominators of the since-transfer-began average rates;
  // 0 when the engine cannot report them (frontend falls back to added_on).
  seconds_downloading: number;
  seconds_seeding: number;
  progress: number;
  status: 'Downloading' | 'Seeding' | 'Paused' | 'Completed' | 'Checking' | 'Queued for checking' | 'Error';
  eta_seconds?: number;
  // null = unknown (no successful tracker scrape yet) - rendered as a dash
  seeds: number | null;
  peers: number;
  ratio: number;
  category: string;
  is_media: boolean;
  poster_url?: string;
  info_hash: string;
  save_path: string;
  added_on: number;
  completed_on?: number;
  comment: string;
  created_by: string;
  piece_size: number;
  total_pieces: number;
  sequential_download: boolean;
  first_last_piece_prio: boolean;
  super_seeding: boolean;
  bandwidthPriority?: number; // 1 = high, 0 = normal, -1 = low
  peerLimit?: number;
  racingMode?: boolean;
  errorString?: string;
  tags: string[];
  files: TorrentFileItem[];
  trackers: TorrentTrackerItem[];
  download_limit_kib: number;
  upload_limit_kib: number;
  ratio_limit: number;
  media_meta?: ParsedMediaMetadata;
  disc_info?: DiscStructureInfo;
}

export interface UserSettings {
  // 1. Behavior / General
  language: string;
  theme: 'dark' | 'light';
  start_on_boot: boolean;
  start_minimized: boolean;
  minimize_to_tray: boolean;
  close_to_tray: boolean;
  notify_on_download_finished: boolean;
  notify_on_torrent_added: boolean;
  inhibit_sleep_when_active: boolean;
  confirm_torrent_deletion: boolean;
  confirm_exit_when_active: boolean;
  save_log_to_disk: boolean;
  max_log_size_mib: number;
  auto_update_enabled: boolean;

  enable_splash: boolean;
  splash_message: string;
  splash_url: string;

  // 2. Downloads
  download_directory: string;
  recent_download_directories: string[];
  temp_incomplete_directory: string;
  use_temp_incomplete_dir: boolean;
  copy_torrent_files_to: string;
  copy_finished_torrents_to: string;
  watched_folder_directory: string;
  auto_management_mode: boolean;
  do_not_start_automatically: boolean;
  show_torrent_content_dialog: boolean;
  append_neb_extension: boolean;
  preallocate_disk_space: boolean;
  run_external_program_on_finished: boolean;
  external_program_command: string;

  // 3. Connection
  peer_connection_protocol: 'Both' | 'TCP' | 'uTP';
  listening_port: number;
  randomize_port_on_startup: boolean;
  enable_upnp: boolean;
  max_global_connections: number;
  max_connections_per_torrent: number;
  max_global_upload_slots: number;
  max_upload_slots_per_torrent: number;
  proxy_type: 'None' | 'HTTP' | 'SOCKS4' | 'SOCKS5';
  proxy_host: string;
  proxy_port: number;
  proxy_auth_enabled: boolean;
  proxy_username: string;
  proxy_password: string;
  proxy_peer_connections: boolean;
  proxy_torrents: boolean;
  proxy_rss: boolean;
  proxy_disable_unsupported: boolean;
  ip_filtering_enabled: boolean;
  ip_filter_path: string;
  ip_filter_apply_to_trackers: boolean;

  // 4. Speed
  max_download_speed_kib: number;
  max_upload_speed_kib: number;
  alt_download_speed_kib: number;
  alt_upload_speed_kib: number;
  alt_rate_limits_enabled: boolean;
  alt_schedule_enabled: boolean;
  alt_schedule_from_time: string;
  alt_schedule_to_time: string;
  alt_schedule_days: string;
  rate_limit_utp: boolean;
  rate_limit_overhead: boolean;
  rate_limit_lan_peers: boolean;

  // 5. BitTorrent
  enable_dht: boolean;
  enable_pex: boolean;
  enable_lsd: boolean;
  encryption_mode: 'Prefer' | 'Require' | 'Disable';
  anonymous_mode: boolean;
  max_active_downloads: number;
  max_active_uploads: number;
  max_active_torrents: number;
  ignore_slow_torrents_in_queue: boolean;
  slow_torrent_dl_threshold_kib: number;
  slow_torrent_ul_threshold_kib: number;
  max_ratio_limit: number;
  max_seeding_time_minutes: number;
  seeding_limit_action: 'Pause' | 'Remove' | 'RemoveAndFiles' | 'SuperSeeding';
  default_trackers_list: string;

  // 6. RSS
  rss_auto_refresh_enabled: boolean;
  rss_refresh_interval_minutes: number;
  rss_max_articles_per_feed: number;
  rss_auto_downloader_enabled: boolean;

  // 7. Web UI
  webui_enabled: boolean;
  webui_address: string;
  webui_port: number;
  webui_username: string;
  webui_password: string;
  webui_https_enabled: boolean;
  webui_cert_path: string;
  webui_key_path: string;
  webui_bypass_localhost: boolean;

  // 8. Advanced
  network_interface_binding: string;
  optional_ip_address_binding: string;
  disk_cache_size_mib: number;
  disk_cache_ttl_seconds: number;
  async_io_threads: number;
  file_pool_size: number;
  stop_tracker_timeout_seconds: number;
  outgoing_ports_min: number;
  outgoing_ports_max: number;
  coalesce_reads_writes: boolean;
  embedded_tracker_enabled: boolean;
  embedded_tracker_port: number;

  // 9. rTorrent Server (Nebulae Server bridge) — applies on app restart
  rtorrent_server_enabled: boolean;
  rtorrent_server_url: string;
  rtorrent_server_token: string;

  // 10. BitTorrent Engine
  engine?: 'transmission';

  // 11. Racing Mode — maximizes piece acquisition and peer connectivity
  racing_mode_enabled: boolean;
  super_seeding_enabled: boolean;
  /** Per-torrent racing preset for torrents added from now on; defaults to true. */
  auto_racing_new_downloads: boolean;

  // 12. Engine Identification
  engine_identity_profile: 'transmission' | 'nebulae' | 'custom';
  custom_user_agent: string;
  custom_peer_id_prefix: string;

  // 13. Media Mode
  media_mode_enabled: boolean;
  media_mode_decision_made: boolean;
  blackout_other_displays_in_fullscreen?: boolean;
}

export interface EngineIdentityInfo {
  profile: 'transmission' | 'nebulae' | 'custom';
  configured_user_agent: string;
  configured_peer_id_prefix: string;
  active_user_agent: string;
  active_peer_id_prefix: string;
  bundled_transmission_version: string;
  bundled_transmission_user_agent: string;
  bundled_transmission_peer_id_prefix: string;
  nebulae_version: string;
  nebulae_user_agent: string;
  nebulae_peer_id_prefix: string;
  supports_runtime_overrides: boolean;
  restart_required: boolean;
  status_message: string;
  version_encoding_supported?: boolean;
  version_encoding_limitation?: string | null;
}

export interface CreateTorrentParams {
  source_path: string;
  output_path: string;
  piece_size_kib: number;
  trackers: string[];
  web_seeds: string[];
  is_private: boolean;
  comment: string;
  created_by: string;
  start_seeding: boolean;
}

export interface DiagnosticSummary {
  app_name: string;
  executable_name: string;
  version: string;
  os: string;
  arch: string;
  database_status: string;
  engine_status: string;
  internal_storage_status: string;
  total_logs_size_bytes: number;
}

export interface AssociationStatus {
  torrent_files: boolean;
  adc_files: boolean;
  magnet_protocol: boolean;
  adc_protocol: boolean;
  platform: string;
  message: string;
}

export interface DiscPlayerStatus {
  player_type: string;
  player_name: string;
  player_path?: string | null;
  supports_dvd_menu: boolean;
  supports_bluray_menu: boolean;
  java_home?: string | null;
  has_jvm: boolean;
  bd_j_supported: boolean;
  note?: string | null;
}

export interface InspectTorrentResult {
  name: string;
  total_bytes: number;
  piece_size: number;
  total_pieces: number;
  info_hash: string;
  is_private: boolean;
  comment: string;
  created_by: string;
  files: TorrentFileItem[];
  trackers: TorrentTrackerItem[];
  metainfo_bytes?: number[];
}

export * from './peering';
export * from './connectivity';
export * from './vpn';
export * from './tracker';

