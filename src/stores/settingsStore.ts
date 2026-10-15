import { create } from 'zustand';
import { UserSettings } from '../types';
import { invoke } from '@tauri-apps/api/core';

interface SettingsState {
  settings: UserSettings;
  isLoading: boolean;
  /** Session engine mode reported by the backend: 'local' (librqbit/mock)
   *  or 'rtorrent_server'. The GUI hides options the engine cannot fulfill. */
  engineMode: 'local' | 'rtorrent_server';
  loadSettings: () => Promise<void>;
  fetchEngineMode: () => Promise<void>;
  saveSettings: (updated: Partial<UserSettings>) => Promise<void>;
  addRecentDirectory: (dir: string) => Promise<void>;
}

export const defaultSettings: UserSettings = {
  // 1. Behavior
  language: 'en-US',
  theme: 'dark',
  start_on_boot: false,
  start_minimized: false,
  minimize_to_tray: true,
  close_to_tray: true,
  notify_on_download_finished: true,
  notify_on_torrent_added: true,
  inhibit_sleep_when_active: true,
  confirm_torrent_deletion: true,
  confirm_exit_when_active: true,
  save_log_to_disk: true,
  max_log_size_mib: 10,
  auto_update_enabled: true,
  enable_splash: true,
  splash_message: 'Consider joining our community\nasiandvdclub.org',
  splash_url: 'https://asiandvdclub.org',

  // 2. Downloads
  download_directory: '~/Downloads',
  recent_download_directories: ['~/Downloads'],
  temp_incomplete_directory: '',
  use_temp_incomplete_dir: false,
  copy_torrent_files_to: '',
  copy_finished_torrents_to: '',
  watched_folder_directory: '',
  auto_management_mode: true,
  do_not_start_automatically: false,
  show_torrent_content_dialog: true,
  append_neb_extension: true,
  preallocate_disk_space: false,
  run_external_program_on_finished: false,
  external_program_command: '',

  // 3. Connection
  peer_connection_protocol: 'Both',
  listening_port: 51413,
  randomize_port_on_startup: false,
  enable_upnp: true,
  max_global_connections: 500,
  max_connections_per_torrent: 100,
  max_global_upload_slots: 8,
  max_upload_slots_per_torrent: 4,
  proxy_type: 'None',
  proxy_host: '',
  proxy_port: 8080,
  proxy_auth_enabled: false,
  proxy_username: '',
  proxy_password: '',
  proxy_peer_connections: true,
  proxy_torrents: true,
  proxy_rss: true,
  proxy_disable_unsupported: false,
  ip_filtering_enabled: false,
  ip_filter_path: '',
  ip_filter_apply_to_trackers: false,

  // 4. Speed
  max_download_speed_kib: 0,
  max_upload_speed_kib: 0,
  alt_download_speed_kib: 1024,
  alt_upload_speed_kib: 256,
  alt_rate_limits_enabled: false,
  alt_schedule_enabled: false,
  alt_schedule_from_time: '08:00',
  alt_schedule_to_time: '20:00',
  alt_schedule_days: 'Every day',
  rate_limit_utp: true,
  rate_limit_overhead: false,
  rate_limit_lan_peers: false,

  // 5. BitTorrent
  enable_dht: true,
  enable_pex: true,
  enable_lsd: true,
  encryption_mode: 'Prefer',
  anonymous_mode: false,
  max_active_downloads: 3,
  max_active_uploads: 3,
  max_active_torrents: 5,
  ignore_slow_torrents_in_queue: false,
  slow_torrent_dl_threshold_kib: 2,
  slow_torrent_ul_threshold_kib: 2,
  max_ratio_limit: -1.0,
  max_seeding_time_minutes: -1,
  seeding_limit_action: 'Pause',
  default_trackers_list: '',

  // 6. RSS
  rss_auto_refresh_enabled: true,
  rss_refresh_interval_minutes: 30,
  rss_max_articles_per_feed: 50,
  rss_auto_downloader_enabled: false,

  // 7. Web UI
  webui_enabled: false,
  webui_address: '127.0.0.1',
  webui_port: 8080,
  webui_username: 'admin',
  webui_password: 'nebulae',
  webui_https_enabled: false,
  webui_cert_path: '',
  webui_key_path: '',
  webui_bypass_localhost: true,

  // 8. Advanced
  network_interface_binding: 'Any interface',
  optional_ip_address_binding: 'All IPv4 and IPv6 addresses',
  disk_cache_size_mib: -1,
  disk_cache_ttl_seconds: 60,
  async_io_threads: 4,
  file_pool_size: 100,
  stop_tracker_timeout_seconds: 5,
  outgoing_ports_min: 0,
  outgoing_ports_max: 0,
  coalesce_reads_writes: true,
  embedded_tracker_enabled: false,
  embedded_tracker_port: 9000,

  // 9. rTorrent Server (Nebulae Server bridge)
  rtorrent_server_enabled: false,
  rtorrent_server_url: '',
  rtorrent_server_token: '',

  // 10. BitTorrent Engine
  engine: 'transmission',
};

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: defaultSettings,
  isLoading: false,
  engineMode: 'local',

  fetchEngineMode: async () => {
    try {
      const mode = await invoke<string>('get_engine_mode');
      if (mode === 'rtorrent_server' || mode === 'local') {
        set({ engineMode: mode });
      }
    } catch {
      // Keep the default; the GUI stays fully enabled.
    }
  },

  loadSettings: async () => {
    set({ isLoading: true });
    try {
      const data = await invoke<UserSettings>('load_user_settings');
      set({ settings: { ...defaultSettings, ...data }, isLoading: false });
    } catch {
      set({ settings: defaultSettings, isLoading: false });
    }
  },

  saveSettings: async (updated: Partial<UserSettings>) => {
    const current = get().settings;
    const merged = { ...current, ...updated };
    set({ settings: merged });
    try {
      await invoke('save_user_settings', { settings: merged });
    } catch {
      // Fallback
    }
  },

  addRecentDirectory: async (dir: string) => {
    const trimmed = dir.trim();
    if (!trimmed) return;
    const currentRecents = get().settings.recent_download_directories || [];
    const filtered = currentRecents.filter((d) => d !== trimmed);
    const updatedRecents = [trimmed, ...filtered].slice(0, 6);
    await get().saveSettings({
      download_directory: trimmed,
      recent_download_directories: updatedRecents,
    });
  },
}));
