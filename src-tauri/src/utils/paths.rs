use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub base_data_dir: PathBuf,
    pub internal_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub diagnostics_dir: PathBuf,
    pub db_dir: PathBuf,
    pub db_file: PathBuf,
    pub cache_dir: PathBuf,
    pub engine_dir: PathBuf,
    pub debug_dir: PathBuf,
}

impl AppPaths {
    pub fn init() -> Result<Self, String> {
        // Resolve platform standard app data directory for "Nebulae"
        let data_dir = dirs::data_dir()
            .ok_or_else(|| "Failed to resolve platform standard data directory".to_string())?
            .join("Nebulae");

        let internal_dir = data_dir.join("internal");
        let logs_dir = internal_dir.join("logs");
        let diagnostics_dir = internal_dir.join("diagnostics");
        let db_dir = internal_dir.join("db");
        let db_file = db_dir.join("nebulae.db");
        let cache_dir = internal_dir.join("cache");
        let engine_dir = internal_dir.join("engine-state");
        let debug_dir = internal_dir.join("debug");

        // Ensure all internal isolated directories exist
        fs::create_dir_all(&logs_dir).map_err(|e| format!("Failed to create logs dir: {e}"))?;
        fs::create_dir_all(&diagnostics_dir)
            .map_err(|e| format!("Failed to create diagnostics dir: {e}"))?;
        fs::create_dir_all(&db_dir).map_err(|e| format!("Failed to create db dir: {e}"))?;
        fs::create_dir_all(&cache_dir).map_err(|e| format!("Failed to create cache dir: {e}"))?;
        fs::create_dir_all(&engine_dir).map_err(|e| format!("Failed to create engine dir: {e}"))?;
        fs::create_dir_all(&debug_dir).map_err(|e| format!("Failed to create debug dir: {e}"))?;

        Ok(Self {
            base_data_dir: data_dir,
            internal_dir,
            logs_dir,
            diagnostics_dir,
            db_dir,
            db_file,
            cache_dir,
            engine_dir,
            debug_dir,
        })
    }

    /// Returns (and does not create) the poster image cache directory.
    /// Callers are responsible for calling `fs::create_dir_all` before writing.
    pub fn poster_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("posters")
    }
}

/// Decodes standard base64 data into raw bytes.
pub fn decode_base64_data(input: &str) -> Result<Vec<u8>, String> {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for &b in input.as_bytes() {
        if b == b'=' || b.is_ascii_whitespace() {
            continue;
        }
        let val = TABLE
            .iter()
            .position(|&c| c == b)
            .ok_or_else(|| "Invalid base64 character".to_string())? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

/// Normalizes source paths by removing surrounding quotes, expanding `~`, and handling `file://` URIs.
pub fn normalize_source_path(source: &str) -> PathBuf {
    let mut s = source.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s = &s[1..s.len() - 1];
    }
    if let Some(stripped) = s.strip_prefix("file://") {
        let decoded = percent_decode_path(stripped);
        let cleaned = if decoded.starts_with("localhost/") {
            &decoded["localhost/".len()..]
        } else {
            &decoded
        };
        let final_path = if cleaned.len() >= 3
            && cleaned.starts_with('/')
            && cleaned.as_bytes()[1].is_ascii_alphabetic()
            && cleaned.as_bytes()[2] == b':'
        {
            &cleaned[1..]
        } else {
            cleaned
        };
        return PathBuf::from(final_path);
    }
    if let Some(stripped) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    if s == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(s)
}

fn percent_decode_path(input: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            if let (Some(h1), Some(h2)) = (chars.next(), chars.next()) {
                if let Ok(val) =
                    u8::from_str_radix(std::str::from_utf8(&[h1, h2]).unwrap_or(""), 16)
                {
                    bytes.push(val);
                    continue;
                }
                bytes.push(b'%');
                bytes.push(h1);
                bytes.push(h2);
                continue;
            }
        }
        bytes.push(b);
    }
    String::from_utf8_lossy(&bytes).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_source_path_file_uri_unix() {
        let p = normalize_source_path("file:///home/user/downloads/sample.torrent");
        assert_eq!(p, PathBuf::from("/home/user/downloads/sample.torrent"));
    }

    #[test]
    fn test_normalize_source_path_file_uri_windows_drive() {
        let p = normalize_source_path("file:///C:/Users/dev/sample.torrent");
        assert_eq!(p, PathBuf::from("C:/Users/dev/sample.torrent"));

        let p2 = normalize_source_path("file:///V:/Blu-ray/disc.iso");
        assert_eq!(p2, PathBuf::from("V:/Blu-ray/disc.iso"));
    }

    #[test]
    fn test_normalize_source_path_file_uri_localhost() {
        let p = normalize_source_path("file://localhost/C:/Users/dev/sample.torrent");
        assert_eq!(p, PathBuf::from("C:/Users/dev/sample.torrent"));
    }

    #[test]
    fn test_normalize_source_path_percent_encoded() {
        let p = normalize_source_path("file:///home/user/test%20space.torrent");
        assert_eq!(p, PathBuf::from("/home/user/test space.torrent"));
    }

    #[test]
    fn test_normalize_source_path_quotes() {
        let p = normalize_source_path("\"/tmp/sample.torrent\"");
        assert_eq!(p, PathBuf::from("/tmp/sample.torrent"));
    }
}
