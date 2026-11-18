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

/// The per-user data directory that holds the "Nebulae" folder. Normally the
/// platform standard one; `NEBULAE_DATA_DIR` redirects it so a test build can
/// run beside an installed app without touching its database, thumbnails or
/// engine state (Windows ignores %APPDATA% overrides, so HOME-style
/// redirection is not available there).
pub fn user_data_dir() -> Option<PathBuf> {
    std::env::var_os("NEBULAE_DATA_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::data_dir)
}

impl AppPaths {
    pub fn init() -> Result<Self, String> {
        // Resolve platform standard app data directory for "Nebulae"
        let data_dir = user_data_dir()
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

/// Resolves a user-entered local folder to an absolute path. Supports `~`,
/// `~/sub` and `~\\sub`; everything else must already be absolute so a
/// relative value is never silently interpreted against the process working
/// directory (which is `/` for a launched macOS app).
pub fn resolve_local_dir_with_home(
    raw: &str,
    home: Option<&std::path::Path>,
) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("No folder was provided".to_string());
    }
    let home_required = || home.ok_or_else(|| "Cannot resolve home directory".to_string());
    let resolved = if trimmed == "~" {
        home_required()?.to_path_buf()
    } else if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        home_required()?.join(rest)
    } else {
        PathBuf::from(trimmed)
    };
    if !resolved.is_absolute() {
        return Err(format!("'{trimmed}' is not an absolute folder path"));
    }
    Ok(resolved)
}

pub fn resolve_local_dir(raw: &str) -> Result<PathBuf, String> {
    resolve_local_dir_with_home(raw, dirs::home_dir().as_deref())
}

/// Confirms `dir` accepts new files by creating and removing a probe file, so
/// read-only mounts and permission failures surface before an engine add.
pub fn ensure_writable_dir(dir: &std::path::Path) -> Result<(), String> {
    let probe = dir.join(format!(".nebulae-write-test-{}", std::process::id()));
    fs::File::create(&probe).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&probe);
    Ok(())
}

/// Resolves, creates if missing, and verifies a local download destination.
pub fn prepare_local_download_dir(raw: &str) -> Result<PathBuf, String> {
    let path = resolve_local_dir(raw)?;
    if path.exists() {
        if !path.is_dir() {
            return Err(format!("'{}' is not a directory", path.display()));
        }
    } else {
        fs::create_dir_all(&path)
            .map_err(|e| format!("'{}' cannot be created: {e}", path.display()))?;
    }
    ensure_writable_dir(&path).map_err(|e| format!("'{}' is not writable: {e}", path.display()))?;
    Ok(path)
}

/// Resolves the bundled application resource directory across all supported platforms,
/// respecting Tauri's packaging conventions:
/// - macOS: Contents/Resources
/// - Windows: adjacent to executable
/// - Linux: /usr/lib/Nebulae (deb/rpm) or $APPDIR/usr/lib/Nebulae (AppImage)
pub fn resolve_bundled_resource_dir() -> PathBuf {
    if let Ok(p) = std::env::var("NEBULAE_RESOURCE_DIR") {
        return PathBuf::from(p);
    }
    if let Ok(appdir) = std::env::var("APPDIR") {
        let appdir_p = PathBuf::from(appdir);
        let candidate = appdir_p.join("usr/lib/Nebulae");
        if candidate.exists() {
            return candidate;
        }
        let candidate_share = appdir_p.join("usr/share/nebulae");
        if candidate_share.exists() {
            return candidate_share;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            // macOS bundle: Contents/MacOS/nebulae -> Contents/Resources
            if let Some(contents) = exe_dir.parent() {
                let mac_res = contents.join("Resources");
                if mac_res.exists() {
                    return mac_res;
                }
            }
            // Next to executable (Windows / dev)
            if exe_dir.join("resources").exists() {
                return exe_dir.to_path_buf();
            }
            // Linux: /usr/bin/nebulae -> /usr/lib/Nebulae
            if let Some(prefix) = exe_dir.parent() {
                let linux_lib = prefix.join("lib/Nebulae");
                if linux_lib.exists() {
                    return linux_lib;
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let sys_path = PathBuf::from("/usr/lib/Nebulae");
        if sys_path.exists() {
            return sys_path;
        }
    }
    PathBuf::from("resources")
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
        let cleaned = if let Some(stripped) = decoded.strip_prefix("localhost/") {
            stripped
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

    #[test]
    fn test_resolve_local_dir_expands_home_notation() {
        let home = std::env::temp_dir().join("home dir ünï");
        let h = Some(home.as_path());
        assert_eq!(resolve_local_dir_with_home("~", h).unwrap(), home);
        assert_eq!(
            resolve_local_dir_with_home("  ~/Downloads ", h).unwrap(),
            home.join("Downloads")
        );
        assert_eq!(
            resolve_local_dir_with_home("~\\Downloads\\Étoile", h).unwrap(),
            home.join("Downloads\\Étoile")
        );
    }

    #[test]
    fn test_resolve_local_dir_preserves_absolute_and_rejects_invalid() {
        let abs = std::env::temp_dir().join("My Downloads ★");
        let abs_str = abs.to_string_lossy().to_string();
        assert_eq!(resolve_local_dir_with_home(&abs_str, None).unwrap(), abs);
        assert!(resolve_local_dir_with_home("", None).is_err());
        assert!(resolve_local_dir_with_home("   ", None).is_err());
        assert!(resolve_local_dir_with_home("Downloads", None).is_err());
        assert!(resolve_local_dir_with_home("./Downloads", None).is_err());
        assert!(resolve_local_dir_with_home("~/Downloads", None)
            .unwrap_err()
            .contains("home directory"));
    }

    #[test]
    fn test_prepare_local_download_dir_creates_and_rejects_files() {
        let base = std::env::temp_dir().join(format!("nebulae_paths_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let nested = base.join("a b").join("c");
        let ok = prepare_local_download_dir(&nested.to_string_lossy()).unwrap();
        assert!(ok.is_dir());

        let file = base.join("occupied.txt");
        fs::write(&file, b"x").unwrap();
        let err = prepare_local_download_dir(&file.to_string_lossy()).unwrap_err();
        assert!(err.contains("not a directory"));
        let _ = fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn test_prepare_local_download_dir_reports_unwritable_directory() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("nebulae_ro_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        fs::set_permissions(&base, fs::Permissions::from_mode(0o555)).unwrap();
        let result = prepare_local_download_dir(&base.to_string_lossy());
        fs::set_permissions(&base, fs::Permissions::from_mode(0o755)).unwrap();
        let _ = fs::remove_dir_all(&base);
        // Root ignores mode bits, so only assert when the probe is actually denied.
        if let Err(e) = result {
            assert!(e.contains("not writable"), "unexpected error: {e}");
        }
    }
}
