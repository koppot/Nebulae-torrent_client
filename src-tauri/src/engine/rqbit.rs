use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use librqbit::api::TorrentIdOrHash;
use librqbit::{
    AddTorrent, AddTorrentOptions, ManagedTorrent, Session, SessionOptions,
    SessionPersistenceConfig, TorrentStatsState,
};

use crate::commands::torrent::{TorrentFileItem, TorrentItem, TorrentTrackerItem};
use crate::db::{Database, TorrentDbMetadata};
use crate::engine::traits::{EngineHook, EngineStatus, TorrentEngine};
use crate::utils::paths::AppPaths;

/// Normalizes a user-provided download directory before it reaches librqbit.
///
/// Expands a leading `~`, anchors relative paths to the user's home directory
/// (the process CWD is `/` for bundled GUI apps, which is a read-only system
/// volume on macOS and used to surface as EROFS deep inside librqbit), and
/// pre-creates the directory so permission problems fail here with a clear
/// message instead of `Read-only file system (os error 30)`.
fn normalize_download_dir(download_dir: Option<&str>) -> Result<Option<String>, String> {
    let Some(raw) = download_dir.map(str::trim).filter(|d| !d.is_empty()) else {
        return Ok(None);
    };

    let home = || dirs::home_dir().ok_or_else(|| "Cannot resolve home directory".to_string());

    let expanded = if raw == "~" {
        home()?
    } else if let Some(rest) = raw.strip_prefix("~/") {
        home()?.join(rest)
    } else {
        PathBuf::from(raw)
    };

    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        home()?.join(expanded)
    };

    std::fs::create_dir_all(&absolute).map_err(|e| {
        format!(
            "Cannot use save destination '{}': {e}. Pick another folder under Save Destination.",
            absolute.display()
        )
    })?;

    Ok(Some(absolute.to_string_lossy().to_string()))
}

/// Resolves the effective output folder for a torrent add.
///
/// librqbit writes multi-file torrent contents flat into an explicitly
/// provided `output_folder` (its default sub-foldering only applies when
/// `output_folder` is not set), so the torrent's root name is joined here
/// to keep the standard `<save destination>/<torrent name>/...` layout.
/// Unsafe torrent names keep the flat layout instead of escaping the
/// destination. Single-file torrents land directly in the destination.
fn resolve_output_folder(
    download_dir: Option<&str>,
    torrent_name: Option<&str>,
    is_multi_file: bool,
) -> Result<Option<String>, String> {
    let Some(base) = normalize_download_dir(download_dir)? else {
        return Ok(None);
    };
    let Some(name) = torrent_name.filter(|_| is_multi_file) else {
        return Ok(Some(base));
    };
    let name_path = PathBuf::from(name);
    if name_path.is_absolute()
        || !name_path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return Ok(Some(base));
    }
    let joined = PathBuf::from(&base).join(name_path);
    std::fs::create_dir_all(&joined).map_err(|e| {
        format!(
            "Cannot create '{}': {e}. Pick another folder under Save Destination.",
            joined.display()
        )
    })?;
    Ok(Some(joined.to_string_lossy().to_string()))
}

/// Instance lock: identifies the process currently owning the engine
/// (listening port + persistence files) and the app version that wrote it.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct InstanceLock {
    pid: u32,
    version: String,
}

fn instance_lock_file(engine_dir: &std::path::Path) -> PathBuf {
    engine_dir.join("engine.lock")
}

/// Returns the process name for a pid, or None if the process does not exist.
fn process_name(pid: u32) -> Option<String> {
    if cfg!(windows) {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let first = stdout.trim().lines().next()?.trim();
        if first.is_empty() || first.to_lowercase().contains("info:") {
            return None;
        }
        let name = first.trim_matches('"').split("\",\"").next()?.to_string();
        (!name.is_empty()).then_some(name)
    } else {
        let out = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!stdout.is_empty()).then_some(stdout)
    }
}

fn name_looks_like_nebulae(name: &str) -> bool {
    let lower = name.to_lowercase();
    let base = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
    base.contains("nebulae")
}

/// Terminates a previous Nebulae instance that is still alive (closing the
/// window only hides it to the tray). When a new version is installed and
/// launched, the old daemon must not keep holding the listening port and
/// persistence files - the new instance takes over automatically.
///
/// PIDs can be recycled after a crash, so the kill only happens when the
/// recorded process actually looks like Nebulae; stale locks are discarded.
fn take_over_previous_instance(engine_dir: &std::path::Path) {
    let lock_path = instance_lock_file(engine_dir);
    let Ok(content) = std::fs::read_to_string(&lock_path) else {
        return;
    };
    let Ok(lock) = serde_json::from_str::<InstanceLock>(&content) else {
        let _ = std::fs::remove_file(&lock_path);
        return;
    };
    let current = std::process::id();
    if lock.pid == current || lock.pid == 0 {
        return;
    }

    let Some(name) = process_name(lock.pid) else {
        // Previous instance already exited; refresh the lock below.
        return;
    };
    if !name_looks_like_nebulae(&name) {
        // Pid was recycled by an unrelated process; drop the stale lock.
        let _ = std::fs::remove_file(&lock_path);
        return;
    }

    let kill = if cfg!(windows) {
        std::process::Command::new("taskkill")
            .args(["/F", "/PID", &lock.pid.to_string()])
            .status()
    } else {
        std::process::Command::new("kill")
            .args(["-TERM", &lock.pid.to_string()])
            .status()
    };

    if kill.map(|s| s.success()).unwrap_or(false) {
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(250));
            if process_name(lock.pid).is_none() {
                break;
            }
        }
        tracing::info!(
            "Automatically terminated the previously active Nebulae instance (pid {}, version {}) so this install can take over the engine",
            lock.pid,
            lock.version
        );
    }
}

fn write_instance_lock(engine_dir: &std::path::Path) {
    let lock = InstanceLock {
        pid: std::process::id(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&lock) {
        let _ = std::fs::write(instance_lock_file(engine_dir), json);
    }
}

#[derive(Debug, Clone)]
pub struct PendingMagnet {
    pub raw_url: String,
    pub display_name: String,
    pub added_on: u64,
    pub download_dir: Option<String>,
    pub trackers: Vec<String>,
}

pub struct RqbitTorrentEngine {
    session: Arc<Session>,
    db: Arc<Database>,
    paths: Arc<AppPaths>,
    kill_switch_active: Arc<AtomicBool>,
    bound_interface: Arc<Mutex<Option<String>>>,
    metadata_cache: Arc<Mutex<HashMap<String, TorrentDbMetadata>>>,
    pending_magnets: Arc<Mutex<HashMap<String, PendingMagnet>>>,
}

impl RqbitTorrentEngine {
    pub fn new(
        paths: Arc<AppPaths>,
        db: Arc<Database>,
        default_download_dir: PathBuf,
        listening_port: u16,
    ) -> Result<Self, String> {
        let _ = std::fs::create_dir_all(&paths.engine_dir);
        let _ = std::fs::create_dir_all(&default_download_dir);

        // A previous instance may still be alive in the tray/menu bar (closing
        // the window does not quit Nebulae), for example after installing an
        // update. Terminate it so this instance owns the listening port
        // (DHT UDP socket) and the persistence files, then take over.
        take_over_previous_instance(&paths.engine_dir);

        let mut session_opts = SessionOptions {
            persistence: Some(SessionPersistenceConfig::Json {
                folder: Some(paths.engine_dir.clone()),
            }),
            fastresume: true,
            ..Default::default()
        };

        let listen_opts = librqbit::ListenerOptions {
            listen_addr: (std::net::Ipv4Addr::UNSPECIFIED, listening_port).into(),
            announce_port: if listening_port > 0 {
                Some(listening_port)
            } else {
                None
            },
            enable_upnp_port_forwarding: false, // Phase 4 handles UPnP explicitly
            ..Default::default()
        };
        session_opts.listen = Some(listen_opts);

        let session = tauri::async_runtime::block_on(async {
            Session::new_with_opts(default_download_dir, session_opts).await
        })
        .map_err(|e| format!("Failed to initialize librqbit session: {e:#}"))?;

        // Record this instance's lock only after the session bound
        // successfully; stale locks from crashed processes are replaced on
        // the next start.
        write_instance_lock(&paths.engine_dir);

        let cached_meta = db.list_all_torrent_metadata().unwrap_or_default();

        Ok(Self {
            session,
            db,
            paths,
            kill_switch_active: Arc::new(AtomicBool::new(false)),
            bound_interface: Arc::new(Mutex::new(None)),
            metadata_cache: Arc::new(Mutex::new(cached_meta)),
            pending_magnets: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn session(&self) -> &Arc<Session> {
        &self.session
    }

    pub fn paths(&self) -> &Arc<AppPaths> {
        &self.paths
    }

    pub fn db(&self) -> &Arc<Database> {
        &self.db
    }

    fn get_handle(&self, id: &str) -> Result<Arc<ManagedTorrent>, String> {
        let tid_or_hash = TorrentIdOrHash::parse(id)
            .map_err(|e| format!("Invalid torrent identifier '{id}': {e}"))?;

        self.session
            .get(tid_or_hash)
            .ok_or_else(|| format!("Torrent with id '{id}' not found"))
    }

    fn is_media_path(path: &str) -> bool {
        let lower = path.to_lowercase();
        lower.ends_with(".mkv")
            || lower.ends_with(".mp4")
            || lower.ends_with(".avi")
            || lower.ends_with(".ts")
            || lower.ends_with(".m2ts")
            || lower.ends_with(".iso")
            || lower.ends_with(".vob")
    }
}

impl TorrentEngine for RqbitTorrentEngine {
    fn list_torrents(&self) -> Result<Vec<TorrentItem>, String> {
        let cached = self.metadata_cache.lock().unwrap().clone();
        let is_ks = self.kill_switch_active.load(Ordering::SeqCst);

        let mut items = self.session.with_torrents(|torrents| {
            let mut result = Vec::new();

            for (tid, handle) in torrents {
                let info_hash = handle.info_hash().as_string();
                let stats = handle.stats();
                let meta = cached.get(&info_hash);

                let default_name = format!("Torrent {tid}");
                let name = handle
                    .name()
                    .or_else(|| meta.and_then(|m| m.display_title.clone()))
                    .unwrap_or(default_name);

                let only_files = handle.only_files();

                let mut files = Vec::new();
                let mut trackers = Vec::new();
                let mut piece_size = 1_048_576;
                let mut total_pieces = 0;
                let comment = String::new();

                let _ = handle.with_metadata(|m| {
                    piece_size = m.info.lengths().default_piece_length();
                    total_pieces = m.info.lengths().total_pieces();

                    for (idx, fi) in m.file_infos.iter().enumerate() {
                        let fname = fi.relative_filename.to_string_lossy().to_string();
                        let size_bytes = fi.len;
                        let prog_bytes = stats.file_progress.get(idx).copied().unwrap_or(0);
                        let prog = if size_bytes > 0 {
                            (prog_bytes as f64 / size_bytes as f64).min(1.0) as f32
                        } else {
                            1.0
                        };
                        let prio = if only_files
                            .as_ref()
                            .map(|of| of.contains(&idx))
                            .unwrap_or(true)
                        {
                            1
                        } else {
                            0
                        };
                        files.push(TorrentFileItem {
                            index: idx as u32,
                            name: fname,
                            size_bytes,
                            progress: prog,
                            priority: prio,
                        });
                    }
                });

                // Extract trackers from handle.shared().trackers
                for (tier, tr_url) in handle.shared().trackers.iter().enumerate() {
                    trackers.push(TorrentTrackerItem {
                        tier: tier as u32,
                        url: tr_url.to_string(),
                        status: "Working".to_string(),
                        seeds: 0,
                        peers: stats
                            .live
                            .as_ref()
                            .map(|l| l.snapshot.peer_stats.live)
                            .unwrap_or(0),
                        downloaded: 0,
                        message: "Announce OK".to_string(),
                    });
                }

                if files.is_empty() {
                    files.push(TorrentFileItem {
                        index: 0,
                        name: name.clone(),
                        size_bytes: stats.total_bytes,
                        progress: if stats.total_bytes > 0 {
                            (stats.progress_bytes as f64 / stats.total_bytes as f64).min(1.0) as f32
                        } else {
                            0.0
                        },
                        priority: 1,
                    });
                }

                let is_media = files.iter().any(|f| Self::is_media_path(&f.name));

                let progress = if stats.total_bytes > 0 {
                    (stats.progress_bytes as f64 / stats.total_bytes as f64).min(1.0) as f32
                } else if stats.finished {
                    1.0
                } else {
                    0.0
                };

                let status = if is_ks || handle.is_paused() {
                    "Paused".to_string()
                } else if stats.finished {
                    "Seeding".to_string()
                } else if handle.with_metadata(|_| ()).is_err() && stats.total_bytes == 0 {
                    "Downloading Metadata".to_string()
                } else {
                    match stats.state {
                        TorrentStatsState::Initializing { .. } => "Checking".to_string(),
                        TorrentStatsState::Paused => "Paused".to_string(),
                        TorrentStatsState::Live => "Downloading".to_string(),
                        TorrentStatsState::Error => "Error".to_string(),
                    }
                };

                let ratio = if stats.progress_bytes > 0 {
                    (stats.uploaded_bytes as f64 / stats.progress_bytes as f64) as f32
                } else {
                    0.0
                };

                let (dl_rate, ul_rate) = if let Some(live) = &stats.live {
                    (live.download_speed.as_bytes(), live.upload_speed.as_bytes())
                } else {
                    (0, 0)
                };

                let eta = if dl_rate > 0 && stats.total_bytes > stats.progress_bytes {
                    Some((stats.total_bytes - stats.progress_bytes) / dl_rate)
                } else {
                    None
                };

                let seeds = if stats.finished { 1 } else { 0 };
                let peers = stats
                    .live
                    .as_ref()
                    .map(|l| l.snapshot.peer_stats.live)
                    .unwrap_or(0);

                let category = meta
                    .and_then(|m| m.category.clone())
                    .unwrap_or_else(|| "General".to_string());
                let save_path = handle.output_folder().to_string_lossy().to_string();
                let added_on = meta.map(|m| m.added_at).unwrap_or(0);
                let completed_on = meta.and_then(|m| m.completed_on);
                let seq = meta.map(|m| m.sequential_download).unwrap_or(false);
                let flp = meta.map(|m| m.first_last_piece_prio).unwrap_or(false);
                let ss = meta.map(|m| m.super_seeding).unwrap_or(false);
                let dl_limit = meta.map(|m| m.download_limit_kib).unwrap_or(0);
                let ul_limit = meta.map(|m| m.upload_limit_kib).unwrap_or(0);
                let ratio_limit = meta.map(|m| m.ratio_limit).unwrap_or(0.0);

                let tags = meta
                    .and_then(|m| m.labels.as_ref())
                    .map(|l| {
                        l.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_else(|| vec!["Active".to_string()]);

                let media_meta = if is_media || category == "Movies" {
                    Some(crate::utils::media_parser::MediaTitleParser::parse(&name))
                } else {
                    None
                };

                result.push(TorrentItem {
                    id: info_hash.clone(),
                    name,
                    total_bytes: stats.total_bytes,
                    downloaded_bytes: stats.progress_bytes,
                    uploaded_bytes: stats.uploaded_bytes,
                    download_rate: dl_rate,
                    upload_rate: ul_rate,
                    progress,
                    status,
                    eta_seconds: eta,
                    seeds,
                    peers,
                    ratio,
                    category,
                    is_media,
                    poster_url: None,
                    info_hash,
                    save_path,
                    added_on,
                    completed_on,
                    comment,
                    created_by: "Nebulae Client".to_string(),
                    piece_size,
                    total_pieces,
                    sequential_download: seq,
                    first_last_piece_prio: flp,
                    super_seeding: ss,
                    tags,
                    files,
                    trackers,
                    download_limit_kib: dl_limit,
                    upload_limit_kib: ul_limit,
                    ratio_limit,
                    media_meta,
                });
            }

            result
        });

        // Include any pending magnet links that are currently resolving metadata over peer wire
        if let Ok(pending_lock) = self.pending_magnets.lock() {
            let seen: std::collections::HashSet<String> =
                items.iter().map(|t| t.info_hash.clone()).collect();
            for (p_hash, p_info) in pending_lock.iter() {
                if !seen.contains(p_hash) {
                    let mut trackers = Vec::new();
                    for (tier, tr_url) in p_info.trackers.iter().enumerate() {
                        trackers.push(TorrentTrackerItem {
                            tier: tier as u32,
                            url: tr_url.clone(),
                            status: "Connecting".to_string(),
                            seeds: 0,
                            peers: 0,
                            downloaded: 0,
                            message: "Resolving metadata via peer exchange (BEP 9)...".to_string(),
                        });
                    }

                    items.push(TorrentItem {
                        id: p_hash.clone(),
                        name: p_info.display_name.clone(),
                        total_bytes: 0,
                        downloaded_bytes: 0,
                        uploaded_bytes: 0,
                        download_rate: 0,
                        upload_rate: 0,
                        progress: 0.0,
                        status: "Downloading Metadata".to_string(),
                        eta_seconds: None,
                        seeds: 0,
                        peers: 0,
                        ratio: 0.0,
                        category: "General".to_string(),
                        is_media: false,
                        poster_url: None,
                        info_hash: p_hash.clone(),
                        save_path: p_info.download_dir.clone().unwrap_or_default(),
                        added_on: p_info.added_on,
                        completed_on: None,
                        comment: "Magnet Link".to_string(),
                        created_by: "Nebulae Metainfo Tool".to_string(),
                        piece_size: 1_048_576,
                        total_pieces: 0,
                        sequential_download: false,
                        first_last_piece_prio: false,
                        super_seeding: false,
                        tags: vec!["Magnet".to_string(), "Active".to_string()],
                        files: vec![],
                        trackers,
                        download_limit_kib: 0,
                        upload_limit_kib: 0,
                        ratio_limit: -1.0,
                        media_meta: None,
                    });
                }
            }
        }

        Ok(items)
    }

    fn add_torrent(&self, source: &str, download_dir: Option<&str>) -> Result<String, String> {
        if self.kill_switch_active.load(Ordering::SeqCst) {
            return Err("Cannot add torrent while VPN Kill Switch is active".to_string());
        }

        let trimmed = source.trim();

        // 1. Check if source is base64 encoded
        let b64_candidate = if trimmed.starts_with("data:") {
            trimmed.find(";base64,").map(|idx| &trimmed[idx + 8..])
        } else if let Some(stripped) = trimmed.strip_prefix("base64:") {
            Some(stripped)
        } else if !trimmed.starts_with("magnet:")
            && !trimmed.contains('/')
            && !trimmed.contains('\\')
            && trimmed.len() > 100
        {
            Some(trimmed)
        } else {
            None
        };

        if let Some(b64) = b64_candidate {
            if let Ok(decoded) = crate::engine::mock::decode_base64_data(b64) {
                return self.add_torrent_from_bytes(&decoded, download_dir);
            }
        }

        // For disk-path sources, join the torrent's root name so multi-file
        // content lands in the standard layout (see resolve_output_folder).
        // Magnets cannot know the name at add time; base64 payloads delegate
        // to add_torrent_from_bytes which parses its own bytes.
        let parsed_layout = if !trimmed.starts_with("magnet:") {
            let source_path = crate::engine::mock::normalize_source_path(trimmed);
            crate::engine::metainfo::ParsedTorrent::parse_file(&source_path)
                .ok()
                .map(|p| (p.name, p.is_multi_file))
        } else {
            None
        };
        let output_folder = resolve_output_folder(
            download_dir,
            parsed_layout.as_ref().map(|(n, _)| n.as_str()),
            parsed_layout.as_ref().map(|(_, m)| *m).unwrap_or(false),
        )?;

        let opts = AddTorrentOptions {
            output_folder,
            overwrite: true,
            ..Default::default()
        };

        if trimmed.starts_with("magnet:") {
            let xt_hash = if let Some(xt_idx) = trimmed.find("xt=urn:btih:") {
                let rest = &trimmed[xt_idx + 12..];
                let hash = rest.split('&').next().unwrap_or("");
                hash.to_lowercase()
            } else {
                return Err("Invalid magnet link: missing xt=urn:btih parameter".to_string());
            };

            if xt_hash.len() != 40 && xt_hash.len() != 32 {
                return Err("Invalid magnet link: invalid info hash length".to_string());
            }

            let dn_title = trimmed
                .split("&dn=")
                .nth(1)
                .and_then(|dn| dn.split('&').next())
                .map(|raw| {
                    raw.replace('+', " ")
                        .replace("%20", " ")
                        .replace("%2F", "/")
                        .replace("%3A", ":")
                })
                .unwrap_or_else(|| format!("Magnet {}", &xt_hash[..8.min(xt_hash.len())]));

            let mut trackers = Vec::new();
            for part in trimmed.split("&tr=") {
                if part.starts_with("magnet:") {
                    continue;
                }
                if let Some(tr) = part.split('&').next() {
                    let decoded = tr
                        .replace("%3A", ":")
                        .replace("%2F", "/")
                        .replace("%3F", "?")
                        .replace("%3D", "=")
                        .replace("%26", "&");
                    if decoded.starts_with("http") || decoded.starts_with("udp") {
                        trackers.push(decoded);
                    }
                }
            }

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let meta = TorrentDbMetadata {
                info_hash: xt_hash.clone(),
                display_title: Some(dn_title.clone()),
                category: Some("General".to_string()),
                labels: Some("Active".to_string()),
                is_watched: false,
                notes: None,
                added_at: now,
                completed_on: None,
                save_path: download_dir.map(|s| s.to_string()),
                download_limit_kib: 0,
                upload_limit_kib: 0,
                ratio_limit: -1.0,
                sequential_download: false,
                first_last_piece_prio: false,
                super_seeding: false,
            };

            let _ = self.db.upsert_torrent_metadata(&meta);
            self.metadata_cache
                .lock()
                .unwrap()
                .insert(xt_hash.clone(), meta);

            let pending = PendingMagnet {
                raw_url: trimmed.to_string(),
                display_name: dn_title,
                added_on: now,
                download_dir: download_dir.map(|d| d.to_string()),
                trackers,
            };

            self.pending_magnets
                .lock()
                .unwrap()
                .insert(xt_hash.clone(), pending);

            let session = self.session.clone();
            let pending_map = self.pending_magnets.clone();
            let hash_key = xt_hash.clone();
            let url_str = trimmed.to_string();

            tauri::async_runtime::spawn(async move {
                let add_req = AddTorrent::from_url(&url_str);
                match session.add_torrent(add_req, Some(opts)).await {
                    Ok(_) => {
                        if let Ok(mut lock) = pending_map.lock() {
                            lock.remove(&hash_key);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Magnet resolution ended for {hash_key}: {e:#}");
                    }
                }
            });

            return Ok(xt_hash);
        }

        let add = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            AddTorrent::from_url(trimmed)
        } else {
            let path = crate::engine::mock::normalize_source_path(trimmed);
            if path.exists() {
                let path_str = path.to_string_lossy().to_string();
                AddTorrent::from_local_filename(&path_str)
                    .map_err(|e| format!("Invalid local torrent file '{}': {e}", path.display()))?
            } else {
                return Err(format!("Torrent source not found on disk: {trimmed}"));
            }
        };

        let res = tauri::async_runtime::block_on(async {
            self.session.add_torrent(add, Some(opts)).await
        })
        .map_err(|e| format!("Failed to add torrent to librqbit session: {e:#}"))?;

        let handle = res
            .into_handle()
            .ok_or_else(|| "Torrent added but handle was not created".to_string())?;

        let info_hash = handle.info_hash().as_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let display_title = handle.name();

        let meta = TorrentDbMetadata {
            info_hash: info_hash.clone(),
            display_title,
            category: Some("General".to_string()),
            labels: Some("Active".to_string()),
            is_watched: false,
            notes: None,
            added_at: now,
            completed_on: None,
            save_path: Some(handle.output_folder().to_string_lossy().to_string()),
            download_limit_kib: 0,
            upload_limit_kib: 0,
            ratio_limit: -1.0,
            sequential_download: false,
            first_last_piece_prio: false,
            super_seeding: false,
        };

        let _ = self.db.upsert_torrent_metadata(&meta);
        self.metadata_cache
            .lock()
            .unwrap()
            .insert(info_hash.clone(), meta);

        Ok(info_hash)
    }

    fn add_torrent_from_bytes(
        &self,
        bytes: &[u8],
        download_dir: Option<&str>,
    ) -> Result<String, String> {
        if self.kill_switch_active.load(Ordering::SeqCst) {
            return Err("Cannot add torrent while VPN Kill Switch is active".to_string());
        }

        let parsed_layout = crate::engine::metainfo::ParsedTorrent::parse_bytes(bytes)
            .ok()
            .map(|p| (p.name, p.is_multi_file));
        let output_folder = resolve_output_folder(
            download_dir,
            parsed_layout.as_ref().map(|(n, _)| n.as_str()),
            parsed_layout.as_ref().map(|(_, m)| *m).unwrap_or(false),
        )?;

        let opts = AddTorrentOptions {
            output_folder,
            overwrite: true,
            ..Default::default()
        };

        let add = AddTorrent::TorrentFileBytes(bytes.to_vec().into());

        let res = tauri::async_runtime::block_on(async {
            self.session.add_torrent(add, Some(opts)).await
        })
        .map_err(|e| format!("Failed to add torrent bytes to librqbit session: {e:#}"))?;

        let handle = res
            .into_handle()
            .ok_or_else(|| "Torrent added but handle was not created".to_string())?;

        let info_hash = handle.info_hash().as_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let meta = TorrentDbMetadata {
            info_hash: info_hash.clone(),
            display_title: handle.name(),
            category: Some("General".to_string()),
            labels: Some("Active".to_string()),
            is_watched: false,
            notes: None,
            added_at: now,
            completed_on: None,
            save_path: Some(handle.output_folder().to_string_lossy().to_string()),
            download_limit_kib: 0,
            upload_limit_kib: 0,
            ratio_limit: -1.0,
            sequential_download: false,
            first_last_piece_prio: false,
            super_seeding: false,
        };

        let _ = self.db.upsert_torrent_metadata(&meta);
        self.metadata_cache
            .lock()
            .unwrap()
            .insert(info_hash.clone(), meta);

        Ok(info_hash)
    }

    fn remove_torrent(&self, id: &str, delete_files: bool) -> Result<(), String> {
        let is_pending = if let Ok(mut pending_lock) = self.pending_magnets.lock() {
            pending_lock.remove(id).is_some()
        } else {
            false
        };

        let tid_or_hash = TorrentIdOrHash::parse(id)
            .map_err(|e| format!("Invalid torrent identifier '{id}': {e}"))?;

        // Safety Guard: Check external qBittorrent directory protection
        if delete_files {
            if let Some(handle) = self.session.get(tid_or_hash) {
                let save_str = handle.output_folder().to_string_lossy().to_lowercase();
                if save_str.contains("qbittorrent") || save_str.contains("bt_backup") {
                    return Err("Safety Guard: Torrents and directories managed by external qBittorrent must not be deleted by Nebulae.".to_string());
                }
            }
        }

        let delete_res = tauri::async_runtime::block_on(async {
            self.session.delete(tid_or_hash, delete_files).await
        });

        if let Err(e) = delete_res {
            if !is_pending {
                return Err(format!("Failed to remove torrent from librqbit: {e:#}"));
            }
        }

        let _ = self.db.delete_torrent_metadata(id);
        self.metadata_cache.lock().unwrap().remove(id);

        Ok(())
    }

    fn pause_torrent(&self, id: &str) -> Result<(), String> {
        let handle = self.get_handle(id)?;
        if handle.is_paused() {
            return Ok(());
        }
        match tauri::async_runtime::block_on(async { self.session.pause(&handle).await }) {
            Ok(()) => Ok(()),
            Err(e) if e.to_string().contains("already paused") => Ok(()),
            Err(e) => Err(format!("Failed to pause torrent: {e:#}")),
        }
    }

    fn resume_torrent(&self, id: &str) -> Result<(), String> {
        if self.kill_switch_active.load(Ordering::SeqCst) {
            return Err(
                "Cannot resume torrent: VPN Kill Switch is active (fail-closed)".to_string(),
            );
        }

        let handle = self.get_handle(id)?;
        if !handle.is_paused() {
            return Ok(());
        }
        match tauri::async_runtime::block_on(async { self.session.unpause(&handle).await }) {
            Ok(()) => Ok(()),
            Err(e) if e.to_string().contains("already live") => Ok(()),
            Err(e) => Err(format!("Failed to resume torrent: {e:#}")),
        }
    }

    fn pause_all(&self) -> Result<(), String> {
        self.session.with_torrents(|torrents| {
            for (_, handle) in torrents {
                let _ = tauri::async_runtime::block_on(async { self.session.pause(handle).await });
            }
        });
        Ok(())
    }

    fn resume_all(&self) -> Result<(), String> {
        if self.kill_switch_active.load(Ordering::SeqCst) {
            return Err("Cannot resume transfers: VPN Kill Switch is active".to_string());
        }

        self.session.with_torrents(|torrents| {
            for (_, handle) in torrents {
                let _ =
                    tauri::async_runtime::block_on(async { self.session.unpause(handle).await });
            }
        });
        Ok(())
    }

    fn trigger_kill_switch(&self) -> Result<(), String> {
        self.kill_switch_active.store(true, Ordering::SeqCst);
        let _ = self.pause_all();
        Ok(())
    }

    fn reset_kill_switch(&self) -> Result<(), String> {
        self.kill_switch_active.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_kill_switch_active(&self) -> bool {
        self.kill_switch_active.load(Ordering::SeqCst)
    }

    fn engine_status(&self) -> EngineStatus {
        let (active, paused) = self.session.with_torrents(|torrents| {
            let mut a = 0;
            let mut p = 0;
            for (_, handle) in torrents {
                if handle.is_paused() {
                    p += 1;
                } else {
                    a += 1;
                }
            }
            (a, p)
        });

        let bound = self.bound_interface.lock().unwrap().clone();

        EngineStatus {
            active_torrents: active,
            paused_torrents: paused,
            kill_switch_engaged: self.is_kill_switch_active(),
            bound_interface: bound,
        }
    }

    fn supports_hook(&self, hook: EngineHook) -> bool {
        match hook {
            EngineHook::PrivateTorrentRestrictions => true,
            EngineHook::PauseResumeKillSwitch => true,
            EngineHook::TrackerDhtPexHandling => true,
            EngineHook::PeerTelemetryCollection => true,
            EngineHook::SocketInterfaceBinding => true,
            EngineHook::PieceRequestScheduling => false,
            EngineHook::ConnectionAdmission => false,
            EngineHook::ConnectionEviction => false,
            EngineHook::PerPeerPrioritization => false,
        }
    }

    fn force_recheck(&self, id: &str) -> Result<(), String> {
        let handle = self.get_handle(id)?;
        let _ = tauri::async_runtime::block_on(async { self.session.pause(&handle).await });
        let _ = tauri::async_runtime::block_on(async { self.session.unpause(&handle).await });
        Ok(())
    }

    fn reannounce(&self, id: &str) -> Result<(), String> {
        let _handle = self.get_handle(id)?;
        Ok(())
    }

    fn set_location(&self, id: &str, new_path: &str) -> Result<(), String> {
        let _handle = self.get_handle(id)?;
        let mut cache = self.metadata_cache.lock().unwrap();
        if let Some(meta) = cache.get_mut(id) {
            meta.save_path = Some(new_path.to_string());
            let _ = self.db.upsert_torrent_metadata(meta);
        }
        Ok(())
    }

    fn set_category(&self, id: &str, category: &str) -> Result<(), String> {
        let mut cache = self.metadata_cache.lock().unwrap();
        if let Some(meta) = cache.get_mut(id) {
            meta.category = Some(category.to_string());
            let _ = self.db.upsert_torrent_metadata(meta);
            Ok(())
        } else {
            let new_meta = TorrentDbMetadata {
                info_hash: id.to_string(),
                category: Some(category.to_string()),
                ..Default::default()
            };
            let _ = self.db.upsert_torrent_metadata(&new_meta);
            cache.insert(id.to_string(), new_meta);
            Ok(())
        }
    }

    fn set_torrent_limits(
        &self,
        id: &str,
        dl_kib: u32,
        ul_kib: u32,
        ratio_limit: f32,
    ) -> Result<(), String> {
        let mut cache = self.metadata_cache.lock().unwrap();
        if let Some(meta) = cache.get_mut(id) {
            meta.download_limit_kib = dl_kib;
            meta.upload_limit_kib = ul_kib;
            meta.ratio_limit = ratio_limit;
            let _ = self.db.upsert_torrent_metadata(meta);
            Ok(())
        } else {
            let new_meta = TorrentDbMetadata {
                info_hash: id.to_string(),
                download_limit_kib: dl_kib,
                upload_limit_kib: ul_kib,
                ratio_limit,
                ..Default::default()
            };
            let _ = self.db.upsert_torrent_metadata(&new_meta);
            cache.insert(id.to_string(), new_meta);
            Ok(())
        }
    }

    fn toggle_sequential_download(&self, id: &str) -> Result<bool, String> {
        let mut cache = self.metadata_cache.lock().unwrap();
        if let Some(meta) = cache.get_mut(id) {
            meta.sequential_download = !meta.sequential_download;
            let val = meta.sequential_download;
            let _ = self.db.upsert_torrent_metadata(meta);
            Ok(val)
        } else {
            let new_meta = TorrentDbMetadata {
                info_hash: id.to_string(),
                sequential_download: true,
                ..Default::default()
            };
            let _ = self.db.upsert_torrent_metadata(&new_meta);
            cache.insert(id.to_string(), new_meta);
            Ok(true)
        }
    }

    fn toggle_first_last_piece_priority(&self, id: &str) -> Result<bool, String> {
        let mut cache = self.metadata_cache.lock().unwrap();
        if let Some(meta) = cache.get_mut(id) {
            meta.first_last_piece_prio = !meta.first_last_piece_prio;
            let val = meta.first_last_piece_prio;
            let _ = self.db.upsert_torrent_metadata(meta);
            Ok(val)
        } else {
            let new_meta = TorrentDbMetadata {
                info_hash: id.to_string(),
                first_last_piece_prio: true,
                ..Default::default()
            };
            let _ = self.db.upsert_torrent_metadata(&new_meta);
            cache.insert(id.to_string(), new_meta);
            Ok(true)
        }
    }

    fn toggle_super_seeding(&self, id: &str) -> Result<bool, String> {
        let mut cache = self.metadata_cache.lock().unwrap();
        if let Some(meta) = cache.get_mut(id) {
            meta.super_seeding = !meta.super_seeding;
            let val = meta.super_seeding;
            let _ = self.db.upsert_torrent_metadata(meta);
            Ok(val)
        } else {
            let new_meta = TorrentDbMetadata {
                info_hash: id.to_string(),
                super_seeding: true,
                ..Default::default()
            };
            let _ = self.db.upsert_torrent_metadata(&new_meta);
            cache.insert(id.to_string(), new_meta);
            Ok(true)
        }
    }

    fn set_file_priority(&self, id: &str, file_index: u32, priority: u8) -> Result<(), String> {
        let handle = self.get_handle(id)?;
        let mut only_files: HashSet<usize> = handle
            .only_files()
            .map(|v| v.into_iter().collect())
            .unwrap_or_else(|| {
                let total_files = handle.with_metadata(|m| m.file_infos.len()).unwrap_or(0);
                (0..total_files).collect()
            });

        if priority == 0 {
            only_files.remove(&(file_index as usize));
        } else {
            only_files.insert(file_index as usize);
        }

        tauri::async_runtime::block_on(async {
            self.session.update_only_files(&handle, &only_files).await
        })
        .map_err(|e| format!("Failed to update file priority: {e:#}"))?;

        Ok(())
    }

    fn rename_torrent(&self, id: &str, new_name: &str) -> Result<(), String> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Err("Torrent name cannot be empty".to_string());
        }

        let mut cache = self.metadata_cache.lock().unwrap();
        if let Some(meta) = cache.get_mut(id) {
            meta.display_title = Some(trimmed.to_string());
            let _ = self.db.upsert_torrent_metadata(meta);
            Ok(())
        } else {
            let new_meta = TorrentDbMetadata {
                info_hash: id.to_string(),
                display_title: Some(trimmed.to_string()),
                ..Default::default()
            };
            let _ = self.db.upsert_torrent_metadata(&new_meta);
            cache.insert(id.to_string(), new_meta);
            Ok(())
        }
    }

    fn rename_torrent_file(
        &self,
        id: &str,
        _file_index: u32,
        _new_file_name: &str,
    ) -> Result<(), String> {
        let _handle = self.get_handle(id)?;
        Ok(())
    }

    fn rename_torrent_folder(
        &self,
        id: &str,
        _old_folder_path: &str,
        _new_folder_name: &str,
    ) -> Result<(), String> {
        let _handle = self.get_handle(id)?;
        Ok(())
    }

    fn export_torrent_bytes(&self, id: &str) -> Result<Vec<u8>, String> {
        let handle = self.get_handle(id)?;
        let bytes = handle
            .with_metadata(|meta| meta.torrent_bytes.to_vec())
            .map_err(|_| "Metadata not downloaded yet".to_string())?;
        if bytes.is_empty() {
            return Err("Torrent metainfo bytes are empty".to_string());
        }
        Ok(bytes)
    }

    fn get_peer_telemetry(
        &self,
        id: &str,
    ) -> Result<Vec<crate::models::peering::PeerTelemetry>, String> {
        let handle = self.get_handle(id)?;
        let live = match handle.live() {
            Some(l) => l,
            None => return Ok(vec![]),
        };

        let snapshot =
            live.per_peer_stats_snapshot(librqbit::http_api_types::PeerStatsFilter::default());

        let mut result = Vec::new();

        for (addr_str, stats) in snapshot.peers {
            let socket_addr = match addr_str.parse::<std::net::SocketAddr>() {
                Ok(sa) => sa,
                Err(_) => continue,
            };

            let ip_str = socket_addr.ip().to_string();
            let port = socket_addr.port();
            let is_v6 = socket_addr.is_ipv6();

            let locality = if socket_addr.ip().is_loopback() {
                crate::models::peering::LocalityTier::Lan
            } else {
                match socket_addr.ip() {
                    std::net::IpAddr::V4(ipv4) => {
                        let octets = ipv4.octets();
                        if octets[0] == 10
                            || (octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31)
                            || (octets[0] == 192 && octets[1] == 168)
                        {
                            crate::models::peering::LocalityTier::Lan
                        } else {
                            crate::models::peering::LocalityTier::Global
                        }
                    }
                    std::net::IpAddr::V6(_) => crate::models::peering::LocalityTier::Global,
                }
            };

            let is_utp = stats
                .conn_kind
                .as_ref()
                .map(|k| k.to_string() == "uTP")
                .unwrap_or(false);
            let client_name = stats
                .client_name
                .unwrap_or_else(|| "Unknown Peer".to_string());
            let fetched = stats.counters.fetched_bytes;
            let uploaded = stats.counters.uploaded_bytes;
            let errors = stats.counters.errors;

            let reciprocity = if fetched > 0 {
                (uploaded as f64 / fetched as f64).min(2.0) as f32
            } else {
                0.0
            };

            let stability = if errors > 0 {
                (1.0 / (1.0 + errors as f32 * 0.1)).max(0.1)
            } else {
                1.0
            };

            result.push(crate::models::peering::PeerTelemetry {
                peer_id: format!("peer-{}:{}", ip_str, port),
                ip: ip_str,
                port,
                rtt_ms: (stats.counters.total_time_connecting_ms / 10).max(1) as u32,
                req_resp_latency_ms: 0,
                sustained_dl_bps: 0,
                sustained_ul_bps: 0,
                upload_reciprocity: reciprocity,
                stability_score: stability,
                disconnect_count: 0,
                timeout_count: 0,
                choke_events: 0,
                client_name,
                locality_tier: locality,
                country_code: None,
                asn: None,
                utp: is_utp,
                encryption: true,
                ipv6: is_v6,
                dht: false,
                pex: true,
                connection_age_seconds: 0,
                total_downloaded_bytes: fetched,
                total_uploaded_bytes: uploaded,
            });
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_download_dir, resolve_output_folder};

    #[test]
    fn normalize_download_dir_accepts_absolute() {
        let dir = std::env::temp_dir().join("nebulae-normalize-test");
        let out = normalize_download_dir(Some(dir.to_str().unwrap())).unwrap();
        assert_eq!(out.as_deref(), dir.to_str());
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn normalize_download_dir_expands_tilde() {
        let home = dirs::home_dir().unwrap();
        let out = normalize_download_dir(Some("~/nebulae-tilde-test"))
            .unwrap()
            .unwrap();
        assert_eq!(out, home.join("nebulae-tilde-test").to_string_lossy());
        let _ = std::fs::remove_dir(home.join("nebulae-tilde-test"));
    }

    #[test]
    fn normalize_download_dir_none_and_empty() {
        assert!(normalize_download_dir(None).unwrap().is_none());
        assert!(normalize_download_dir(Some("")).unwrap().is_none());
        assert!(normalize_download_dir(Some("   ")).unwrap().is_none());
    }

    #[test]
    fn resolve_output_folder_single_file_stays_flat() {
        let base = std::env::temp_dir().join("nebulae-resolve-single");
        let out = resolve_output_folder(base.to_str(), Some("DISC.ISO"), false)
            .unwrap()
            .unwrap();
        assert_eq!(out, base.to_string_lossy());
        let _ = std::fs::remove_dir(&base);
    }

    #[test]
    fn resolve_output_folder_multi_file_joins_name() {
        let base = std::env::temp_dir().join("nebulae-resolve-multi");
        let name = "Banner of the Stars III OVA (2005) R2J";
        let out = resolve_output_folder(base.to_str(), Some(name), true)
            .unwrap()
            .unwrap();
        assert_eq!(out, base.join(name).to_string_lossy());
        assert!(base.join(name).is_dir());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn resolve_output_folder_unsafe_name_stays_flat() {
        let base = std::env::temp_dir().join("nebulae-resolve-unsafe");
        for bad in ["../escape", "/etc/passwd", "a/../../escape"] {
            let out = resolve_output_folder(base.to_str(), Some(bad), true)
                .unwrap()
                .unwrap();
            assert_eq!(out, base.to_string_lossy());
        }
        let _ = std::fs::remove_dir(&base);
    }
}
