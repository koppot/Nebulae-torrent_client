export type ViewMode = 'media' | 'manager';

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
}

export interface UserSettings {
  download_directory: string;
  max_download_speed_kib: number;
  max_upload_speed_kib: number;
  theme: 'dark' | 'light';
  enable_splash: boolean;
  splash_message: string;
  splash_url: string;
  listening_port: number;
  enable_upnp: boolean;
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
