pub const CREATE_SETTINGS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

pub const CREATE_TORRENT_METADATA_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS torrent_metadata (
    info_hash TEXT PRIMARY KEY,
    display_title TEXT,
    category TEXT,
    labels TEXT,
    is_watched INTEGER DEFAULT 0,
    notes TEXT,
    added_at INTEGER,
    completed_on INTEGER,
    save_path TEXT,
    download_limit_kib INTEGER DEFAULT 0,
    upload_limit_kib INTEGER DEFAULT 0,
    ratio_limit REAL DEFAULT -1.0,
    sequential_download INTEGER DEFAULT 0,
    first_last_piece_prio INTEGER DEFAULT 0,
    super_seeding INTEGER DEFAULT 0
);
"#;

pub const CREATE_VPN_PROFILES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS vpn_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    protocol TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    interface_address TEXT NOT NULL,
    dns_servers TEXT NOT NULL,
    requires_auth INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL
);
"#;

pub const CREATE_SEED_TIME_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS seed_time_records (
    info_hash TEXT PRIMARY KEY,
    torrent_name TEXT NOT NULL,
    seeding_time_seconds INTEGER DEFAULT 0,
    required_seeding_seconds INTEGER DEFAULT 0,
    is_shield_satisfied INTEGER DEFAULT 0
);
"#;
