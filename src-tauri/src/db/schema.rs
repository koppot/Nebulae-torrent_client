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
    added_at INTEGER
);
"#;
