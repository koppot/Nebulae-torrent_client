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
    super_seeding INTEGER DEFAULT 0,
    racing_override INTEGER,
    -- Feature 6: user-defined listing name (authoritative over provider title)
    user_display_title TEXT,
    user_title_locked INTEGER DEFAULT 0,
    -- Feature 3: manual poster ordering (NULL = newly added, sorted last)
    sort_order INTEGER,
    poster_path TEXT,
    -- Feature 8/9: metadata provider attribution
    provider TEXT,
    provider_id TEXT,
    -- Feature 5: tracks movement to External section
    is_external INTEGER DEFAULT 0,
    external_moved_at INTEGER,
    -- Feature 5: consecutive missed sync count for removal detection
    sync_miss_count INTEGER DEFAULT 0,
    -- qBittorrent import last checked timestamp
    last_checked INTEGER
);
"#;

pub const CREATE_SNATCHED_RECORDS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS snatched_records (
    info_hash TEXT PRIMARY KEY,
    torrent_name TEXT NOT NULL,
    size_bytes INTEGER DEFAULT 0,
    snatched_at INTEGER,
    save_path TEXT,
    display_title TEXT,
    poster_path TEXT,
    provider TEXT,
    provider_id TEXT,
    external_moved_at INTEGER
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

pub const CREATE_MANUAL_PEER_BLOCKS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS manual_peer_blocks (
    ip TEXT PRIMARY KEY,
    blocked_at_epoch_ms INTEGER NOT NULL
);
"#;

pub const CREATE_PEER_HISTORY_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS peer_history (
    torrent_hash TEXT NOT NULL,
    peer_key TEXT NOT NULL,
    ip TEXT NOT NULL,
    port INTEGER NOT NULL,
    client_name TEXT NOT NULL DEFAULT '',
    first_seen_epoch_ms INTEGER NOT NULL,
    last_seen_epoch_ms INTEGER NOT NULL,
    last_peer_download_bytes INTEGER NOT NULL DEFAULT 0,
    last_peer_upload_bytes INTEGER NOT NULL DEFAULT 0,
    downloaded_delta_bytes INTEGER NOT NULL DEFAULT 0,
    uploaded_delta_bytes INTEGER NOT NULL DEFAULT 0,
    counter_resets INTEGER NOT NULL DEFAULT 0,
    was_blocked INTEGER NOT NULL DEFAULT 0,
    country_code TEXT,
    adc_user_id INTEGER,
    adc_username TEXT,
    adc_profile_url TEXT,
    adc_account_revision INTEGER,
    adc_matched_at_epoch_ms INTEGER,
    PRIMARY KEY (torrent_hash, peer_key)
);
CREATE INDEX IF NOT EXISTS idx_peer_history_last_seen
    ON peer_history(last_seen_epoch_ms DESC);
"#;

pub const CREATE_ADC_PEER_CACHE_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS adc_peer_cache (
    account_revision INTEGER NOT NULL,
    info_hash TEXT NOT NULL,
    peer_key TEXT NOT NULL,
    ip TEXT NOT NULL,
    port INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    username TEXT NOT NULL,
    ratio REAL NOT NULL,
    uploaded_bytes INTEGER NOT NULL,
    downloaded_bytes INTEGER NOT NULL,
    bonus_points REAL NOT NULL,
    avatar_url TEXT,
    profile_url TEXT,
    matched_at_epoch_ms INTEGER NOT NULL,
    PRIMARY KEY (account_revision, info_hash, peer_key)
);
CREATE INDEX IF NOT EXISTS idx_adc_peer_cache_lookup
    ON adc_peer_cache(account_revision, info_hash);
CREATE INDEX IF NOT EXISTS idx_adc_peer_cache_matched_at
    ON adc_peer_cache(matched_at_epoch_ms DESC);
"#;
