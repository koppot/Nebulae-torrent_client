export type ViewMode = 'media' | 'manager';

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
  seeds: number;
  peers: number;
  downloaded: number;
  message: string;
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
}

export interface TorrentItem {
  id: string;
  name: string;
  total_bytes: number;
  downloaded_bytes: number;
  uploaded_bytes: number;
  download_rate: number;
  upload_rate: number;
  progress: number;
  status: 'Downloading' | 'Seeding' | 'Paused' | 'Completed' | 'Checking' | 'Error';
  eta_seconds?: number;
  seeds: number;
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
  tags: string[];
  files: TorrentFileItem[];
  trackers: TorrentTrackerItem[];
  download_limit_kib: number;
  upload_limit_kib: number;
  ratio_limit: number;
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

export * from './peering';
export * from './connectivity';
export * from './vpn';
export * from './tracker';
