//! Nebulae Server engine: drives a remote rtorrent daemon over SCGI.
//!
//! The Nebulae interface stays exactly the same; the engine behind it runs on
//! a server (Unraid, TrueNAS, any Docker host) instead of locally. This
//! implements the shared [`TorrentEngine`] trait against rtorrent's XML-RPC
//! command set (`d.*` commands).
//!
//! RPC compatibility: rtorrent 0.16 removed the deprecated `get_`-prefixed
//! command aliases and changed the `d.multicall2`/`f.multicall` signatures
//! (they gained an extra leading target/filter argument). The engine probes
//! `system.client_version` once and gates call shapes accordingly; the
//! container image ships rtorrent 0.9.8.

pub mod rpc;
pub mod scgi;

use crate::commands::torrent::{TorrentFileItem, TorrentItem, TorrentTrackerItem};
use crate::engine::traits::{EngineHook, EngineStatus, TorrentEngine};
use scgi::{call, ScgiTarget};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// rtorrent reports ratios as per-mille (x1000).
const RTORRENT_RATIO_SCALE: f32 = 1000.0;

pub struct RtorrentRemoteEngine {
    target: ScgiTarget,
    kill_switch: AtomicBool,
    /// (major, minor) from `system.client_version`, probed on first use.
    version: OnceLock<(u32, u32)>,
}

impl RtorrentRemoteEngine {
    pub fn new(target: ScgiTarget) -> Self {
        Self {
            target,
            kill_switch: AtomicBool::new(false),
            version: OnceLock::new(),
        }
    }

    fn rpc(&self, method: &str, params: &[rpc::RpcValue]) -> Result<rpc::RpcValue, String> {
        let body = rpc::build_call(method, params);
        let raw = call(&self.target, body.as_bytes())?;
        rpc::parse_response(
            std::str::from_utf8(&raw)
                .map_err(|e| format!("rtorrent: response is not utf-8: {e}"))?,
        )
    }

    fn torrent_hash(id: &str) -> rpc::RpcValue {
        rpc::RpcValue::Str(id.to_string())
    }

    /// Probed once; falls back to the 0.9.x call shapes (the container
    /// default) when the version string cannot be read.
    fn rtorrent_version(&self) -> (u32, u32) {
        *self.version.get_or_init(|| {
            let parsed = self
                .rpc("system.client_version", &[])
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .and_then(|s| {
                    let mut parts = s.split('.');
                    let maj = parts.next()?.parse::<u32>().ok()?;
                    let min = parts.next()?.parse::<u32>().ok()?;
                    Some((maj, min))
                });
            parsed.unwrap_or((0, 9))
        })
    }

    fn is_modern_rpc(&self) -> bool {
        let (maj, min) = self.rtorrent_version();
        maj > 0 || min >= 16
    }

    /// One row of `d.multicall2` -> a UI-facing TorrentItem.
    fn map_torrent_row(row: &[rpc::RpcValue]) -> Result<TorrentItem, String> {
        // Column order must match LIST_COLUMNS below.
        let text = |i: usize| {
            row.get(i)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        let int = |i: usize| row.get(i).and_then(|v| v.as_int()).unwrap_or(0);

        let total_bytes = int(3).max(0) as u64;
        let downloaded_bytes = int(4).max(0) as u64;
        let download_rate = int(6).max(0) as u64;
        let upload_rate = int(7).max(0) as u64;
        let state_running = int(9) == 1;
        let complete = int(10) == 1;
        let ratio = int(8).max(0) as f32 / RTORRENT_RATIO_SCALE;

        let status = match (state_running, complete) {
            (true, true) => "Seeding".to_string(),
            (true, false) => "Downloading".to_string(),
            (false, true) => "Completed".to_string(),
            (false, false) => "Paused".to_string(),
        };

        let progress = if total_bytes > 0 {
            (downloaded_bytes as f32 / total_bytes as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let eta_seconds = if state_running && !complete && download_rate > 0 {
            Some(total_bytes.saturating_sub(downloaded_bytes) / download_rate)
        } else {
            None
        };

        let id = text(0).to_lowercase();
        let name = text(1);
        let is_media = matches!(
            std::path::Path::new(&name)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .as_deref(),
            Some("mkv" | "mp4" | "avi" | "mov" | "iso" | "wmv" | "flv" | "m2ts")
        );

        Ok(TorrentItem {
            id,
            name: name.clone(),
            total_bytes,
            downloaded_bytes,
            uploaded_bytes: int(5).max(0) as u64,
            download_rate,
            upload_rate,
            progress,
            status,
            eta_seconds,
            seeds: Some(int(11).max(0) as u32),
            peers: int(12).max(0) as u32,
            ratio,
            // rtorrent's custom1 field is the de-facto label/category slot
            // (same one ruTorrent uses).
            category: text(13),
            is_media,
            poster_url: None,
            info_hash: text(0).to_lowercase(),
            save_path: text(2),
            added_on: int(14).max(0) as u64,
            completed_on: None,
            comment: String::new(),
            created_by: "rtorrent".to_string(),
            piece_size: int(15).max(0) as u32,
            total_pieces: int(16).max(0) as u32,
            sequential_download: false,
            first_last_piece_prio: false,
            super_seeding: false,
            tags: Vec::new(),
            files: Vec::new(),
            trackers: Vec::new(),
            download_limit_kib: 0,
            upload_limit_kib: 0,
            ratio_limit: 0.0,
            media_meta: if is_media {
                Some(crate::utils::media_parser::MediaTitleParser::parse(&name))
            } else {
                None
            },
        })
    }

    /// File rows for one torrent. 0.16 added a filter argument to
    /// `f.multicall` (verified against 0.16.25; empty string selects all).
    fn fetch_files(&self, hash: &str) -> Result<Vec<TorrentFileItem>, String> {
        let columns: [&str; 5] = if self.is_modern_rpc() {
            [
                "f.path=",
                "f.priority=",
                "f.size_bytes=",
                "f.completed_chunks=",
                "f.size_chunks=",
            ]
        } else {
            [
                "f.get_path=",
                "f.get_priority=",
                "f.get_size_bytes=",
                "f.get_completed_chunks=",
                "f.get_size_chunks=",
            ]
        };
        let mut params = vec![Self::torrent_hash(hash)];
        if self.is_modern_rpc() {
            params.push(rpc::RpcValue::Str(String::new()));
        }
        for column in columns {
            params.push(rpc::RpcValue::Str(column.to_string()));
        }
        let rows = self
            .rpc("f.multicall", &params)?
            .as_array()
            .ok_or_else(|| "rtorrent: malformed file multicall".to_string())?
            .to_vec();

        let files: Vec<TorrentFileItem> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let cells = row.as_array().ok_or("rtorrent: malformed file row")?;
                let int = |i: usize| cells.get(i).and_then(|v| v.as_int()).unwrap_or(0);
                let size_chunks = int(4).max(0);
                let progress = if size_chunks > 0 {
                    ((int(3).max(0) as f32) / (size_chunks as f32)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                Ok(TorrentFileItem {
                    index: index as u32,
                    name: cells
                        .first()
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    size_bytes: int(2).max(0) as u64,
                    progress,
                    priority: int(1).clamp(0, 3) as u8,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(files)
    }

    /// Tracker rows for one torrent. Command names differ per version
    /// (0.9.x uses the `t.get_*` aliases).
    fn fetch_trackers(&self, hash: &str) -> Result<Vec<TorrentTrackerItem>, String> {
        let columns: [&str; 6] = if self.is_modern_rpc() {
            [
                "t.url=",
                "t.group=",
                "t.is_enabled=",
                "t.scrape_complete=",
                "t.scrape_incomplete=",
                "t.scrape_downloaded=",
            ]
        } else {
            [
                "t.get_url=",
                "t.get_group=",
                "t.get_enabled=",
                "t.get_scrape_complete=",
                "t.get_scrape_incomplete=",
                "t.get_scrape_downloaded=",
            ]
        };
        let mut params = vec![Self::torrent_hash(hash)];
        for column in columns {
            params.push(rpc::RpcValue::Str(column.to_string()));
        }
        let rows = self
            .rpc("t.multicall", &params)?
            .as_array()
            .ok_or_else(|| "rtorrent: malformed tracker multicall".to_string())?
            .to_vec();

        let trackers = rows
            .iter()
            .map(|row| {
                let cells = row.as_array().ok_or("rtorrent: malformed tracker row")?;
                let text = |i: usize| {
                    cells
                        .get(i)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                };
                let int = |i: usize| cells.get(i).and_then(|v| v.as_int()).unwrap_or(0);
                Ok(TorrentTrackerItem {
                    tier: int(1).max(0) as u32,
                    url: text(0),
                    status: if int(2) == 1 { "Enabled" } else { "Disabled" }.to_string(),
                    seeds: Some(int(3).max(0) as u32),
                    peers: Some(int(4).max(0) as u32),
                    downloaded: int(5).max(0) as u64,
                    message: String::new(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(trackers)
    }

    /// rtorrent only accepts a throttle change on an inactive download; stop,
    /// assign, and restore the previous state.
    fn assign_throttle(&self, id: &str, name: &str) -> Result<(), String> {
        let was_active = self
            .rpc("d.state", &[Self::torrent_hash(id)])?
            .as_int()
            .unwrap_or(0)
            == 1;
        if was_active {
            self.rpc("d.stop", &[Self::torrent_hash(id)])?;
        }
        let result = self.rpc(
            "d.throttle_name.set",
            &[Self::torrent_hash(id), rpc::RpcValue::Str(name.to_string())],
        );
        if was_active {
            self.rpc("d.start", &[Self::torrent_hash(id)])?;
        }
        result.map(|_| ())
    }
}

/// Columns requested in every `d.multicall2` row. Order must match
/// [`RtorrentRemoteEngine::map_torrent_row`]. All names verified against
/// rtorrent 0.16.25 and valid on 0.9.8.
const LIST_COLUMNS: [&str; 17] = [
    "d.hash=",
    "d.name=",
    "d.base_path=",
    "d.size_bytes=",
    "d.completed_bytes=",
    "d.up.total=",
    "d.down.rate=",
    "d.up.rate=",
    "d.ratio=",
    "d.state=",
    "d.complete=",
    "d.peers_complete=",
    "d.peers_connected=",
    "d.custom1=",
    "d.timestamp.started=",
    "d.chunk_size=",
    "d.size_chunks=",
];

impl TorrentEngine for RtorrentRemoteEngine {
    fn list_torrents(&self) -> Result<Vec<TorrentItem>, String> {
        // 0.16+ signature: (target, view, columns...); 0.9.x: (view, columns...).
        let mut params = vec![rpc::RpcValue::Str(String::new())];
        if self.is_modern_rpc() {
            params.push(rpc::RpcValue::Str("main".to_string()));
        }
        for column in LIST_COLUMNS {
            params.push(rpc::RpcValue::Str(column.to_string()));
        }
        let response = self.rpc("d.multicall2", &params)?;
        let rows = response
            .as_array()
            .ok_or_else(|| "rtorrent: unexpected multicall response".to_string())?;

        let mut items: Vec<TorrentItem> = rows
            .iter()
            .map(|row| {
                let cells = row
                    .as_array()
                    .ok_or_else(|| "rtorrent: malformed multicall row".to_string())?;
                Self::map_torrent_row(cells)
            })
            .collect::<Result<Vec<_>, String>>()?;

        // Detail passes: one extra sub-millisecond call per torrent on the
        // local SCGI socket. Best effort: a faulted detail call leaves the
        // arrays empty instead of failing the whole list refresh.
        for item in &mut items {
            if let Ok(files) = self.fetch_files(&item.id) {
                item.files = files;
            }
            if let Ok(trackers) = self.fetch_trackers(&item.id) {
                item.trackers = trackers;
            }
        }
        Ok(items)
    }

    fn add_torrent(&self, source: &str, download_dir: Option<&str>) -> Result<String, String> {
        // Magnets and URLs are loaded directly; local files are sent as raw
        // bencode bytes via XML-RPC <base64> (never a lossy UTF-8 string).
        let (method, param) = if source.starts_with("magnet:")
            || source.starts_with("http://")
            || source.starts_with("https://")
        {
            ("load.start", rpc::RpcValue::Str(source.to_string()))
        } else {
            let bytes = std::fs::read(source)
                .map_err(|e| format!("rtorrent: cannot read torrent file: {e}"))?;
            ("load.raw_start", rpc::RpcValue::Bytes(bytes))
        };
        // Load commands accept appended keylines: per-add destination works
        // without knowing the new item's hash (verified against 0.16.25).
        let mut params = vec![rpc::RpcValue::Str(String::new()), param];
        if let Some(dir) = download_dir {
            if !dir.trim().is_empty() {
                params.push(rpc::RpcValue::Str(format!(
                    "d.directory.set={}",
                    dir.trim()
                )));
            }
        }
        self.rpc(method, &params)?;
        // rtorrent does not return the new hash from load.*; the UI discovers it on the
        // next list refresh.
        Ok(String::new())
    }

    fn add_torrent_from_bytes(
        &self,
        bytes: &[u8],
        download_dir: Option<&str>,
    ) -> Result<String, String> {
        // Stage the bencode to a temp file and reuse the file path path
        // (load.raw_start with <base64> bytes).
        let staging = std::env::temp_dir().join(format!(
            "nebulae-upload-{}.torrent",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::write(&staging, bytes)
            .map_err(|e| format!("rtorrent: cannot stage uploaded torrent: {e}"))?;
        let result = self.add_torrent(&staging.to_string_lossy(), download_dir);
        let _ = std::fs::remove_file(&staging);
        result
    }

    fn remove_torrent(&self, id: &str, delete_files: bool) -> Result<(), String> {
        if delete_files {
            // Deleting downloaded data is not exposed over rtorrent's RPC; the container
            // is expected to manage data retention. Removing the torrent only.
            return Err(
                "rtorrent: deleting downloaded data over RPC is not supported; erase only"
                    .to_string(),
            );
        }
        self.rpc("d.erase", &[Self::torrent_hash(id)])?;
        Ok(())
    }

    fn pause_torrent(&self, id: &str) -> Result<(), String> {
        self.rpc("d.stop", &[Self::torrent_hash(id)]).map(|_| ())
    }

    fn resume_torrent(&self, id: &str) -> Result<(), String> {
        self.rpc("d.start", &[Self::torrent_hash(id)]).map(|_| ())
    }

    fn pause_all(&self) -> Result<(), String> {
        for item in self.list_torrents()? {
            if item.status == "Downloading" || item.status == "Seeding" {
                self.pause_torrent(&item.id)?;
            }
        }
        Ok(())
    }

    fn resume_all(&self) -> Result<(), String> {
        for item in self.list_torrents()? {
            if item.status == "Paused" {
                self.resume_torrent(&item.id)?;
            }
        }
        Ok(())
    }

    // Kill switch: rtorrent has no native switch, so Nebulae owns the state and
    // enforces it by stopping every active torrent. Reset clears the flag; resuming
    // previous activity is a deliberate user action.
    fn trigger_kill_switch(&self) -> Result<(), String> {
        self.pause_all()?;
        self.kill_switch.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn reset_kill_switch(&self) -> Result<(), String> {
        self.kill_switch.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_kill_switch_active(&self) -> bool {
        self.kill_switch.load(Ordering::SeqCst)
    }

    fn engine_status(&self) -> EngineStatus {
        let torrents = self.list_torrents().unwrap_or_default();
        EngineStatus {
            active_torrents: torrents
                .iter()
                .filter(|t| t.status == "Downloading" || t.status == "Seeding")
                .count(),
            paused_torrents: torrents.iter().filter(|t| t.status == "Paused").count(),
            kill_switch_engaged: self.is_kill_switch_active(),
            // Binding happens on the server side (rtorrent/container config).
            bound_interface: None,
        }
    }

    fn supports_hook(&self, hook: EngineHook) -> bool {
        matches!(
            hook,
            EngineHook::PauseResumeKillSwitch | EngineHook::PrivateTorrentRestrictions
        )
    }

    fn force_recheck(&self, id: &str) -> Result<(), String> {
        self.rpc("d.check_hash", &[Self::torrent_hash(id)])
            .map(|_| ())
    }

    fn set_location(&self, id: &str, new_path: &str) -> Result<(), String> {
        self.rpc(
            "d.directory.set",
            &[
                Self::torrent_hash(id),
                rpc::RpcValue::Str(new_path.to_string()),
            ],
        )
        .map(|_| ())
    }

    fn reannounce(&self, id: &str) -> Result<(), String> {
        // Verified present in rtorrent 0.16.25 and 0.9.x.
        self.rpc("d.tracker_announce", &[Self::torrent_hash(id)])
            .map(|_| ())
    }

    fn set_category(&self, id: &str, category: &str) -> Result<(), String> {
        // custom1 is rtorrent's label slot (ruTorrent-compatible).
        self.rpc(
            "d.custom1.set",
            &[
                Self::torrent_hash(id),
                rpc::RpcValue::Str(category.to_string()),
            ],
        )
        .map(|_| ())
    }

    fn set_torrent_limits(
        &self,
        id: &str,
        dl_kib: u32,
        ul_kib: u32,
        _ratio_limit: f32,
    ) -> Result<(), String> {
        if dl_kib == 0 && ul_kib == 0 {
            // Unassigning the throttle returns the torrent to the global limits.
            return self.assign_throttle(id, "");
        }
        if self.is_modern_rpc() {
            // rtorrent 0.16 removed the runtime throttle-group RPC commands
            // (throttle.group.insert / throttle.*.max.set); per-torrent rate
            // assignment is only possible on 0.9.x.
            return Err(
                "rtorrent 0.16+ no longer supports per-torrent rate limits over RPC; use the global limits in the container config or rtorrent 0.9.x".to_string(),
            );
        }
        // 0.9.x: named throttle groups are created on demand.
        let group = format!("nebulae_thr_{}_{}", dl_kib, ul_kib);
        // Faults if the group already exists; safe to ignore, the rate-set
        // calls below (re)apply the rates every time.
        let _ = self.rpc(
            "throttle.group.insert",
            &[rpc::RpcValue::Str(group.clone())],
        );
        self.rpc(
            "throttle.down.max.set",
            &[
                rpc::RpcValue::Str(group.clone()),
                rpc::RpcValue::Int(dl_kib as i64 * 1024),
            ],
        )?;
        self.rpc(
            "throttle.up.max.set",
            &[
                rpc::RpcValue::Str(group.clone()),
                rpc::RpcValue::Int(ul_kib as i64 * 1024),
            ],
        )?;
        self.assign_throttle(id, &group)
    }

    fn toggle_first_last_piece_priority(&self, id: &str) -> Result<bool, String> {
        // rtorrent's "prioritize table of contents" = first/last piece of
        // every file (verified on 0.16.25; present since libtorrent 0.13).
        let current = self
            .rpc("file.prioritize_toc", &[Self::torrent_hash(id)])?
            .as_int()
            .unwrap_or(0)
            == 1;
        let new_state = if current { "0" } else { "1" };
        self.rpc(
            "file.prioritize_toc.set",
            &[
                Self::torrent_hash(id),
                rpc::RpcValue::Str(new_state.to_string()),
            ],
        )?;
        self.rpc("d.update_priorities", &[Self::torrent_hash(id)])?;
        Ok(!current)
    }

    fn set_file_priority(&self, id: &str, file_index: u32, priority: u8) -> Result<(), String> {
        // 0.9.x target form (hash, index, value) as used by ruTorrent.
        // rtorrent 0.16.25 currently faults this shape upstream; that fault
        // passes through honestly instead of being swallowed.
        self.rpc(
            "f.priority.set",
            &[
                Self::torrent_hash(id),
                rpc::RpcValue::Str(file_index.to_string()),
                rpc::RpcValue::Str(priority.to_string()),
            ],
        )?;
        self.rpc("d.update_priorities", &[Self::torrent_hash(id)])
            .map(|_| ())
    }

    // rtorrent has no equivalent for the following; they fail loudly rather
    // than pretend.
    fn toggle_sequential_download(&self, _id: &str) -> Result<bool, String> {
        Err("rtorrent: sequential download is not supported by rtorrent".to_string())
    }

    fn toggle_super_seeding(&self, _id: &str) -> Result<bool, String> {
        Err("rtorrent: super-seeding is not available".to_string())
    }

    fn rename_torrent(&self, _id: &str, _new_name: &str) -> Result<(), String> {
        Err("rtorrent: display name is derived from metadata; rename unsupported".to_string())
    }

    fn rename_torrent_file(
        &self,
        _id: &str,
        _file_index: u32,
        _new_file_name: &str,
    ) -> Result<(), String> {
        Err("rtorrent: file rename requires server-side filesystem access".to_string())
    }

    fn rename_torrent_folder(
        &self,
        _id: &str,
        _old_folder_path: &str,
        _new_folder_name: &str,
    ) -> Result<(), String> {
        Err("rtorrent: folder rename requires server-side filesystem access".to_string())
    }

    fn export_torrent_bytes(&self, _id: &str) -> Result<Vec<u8>, String> {
        Err("rtorrent: exporting .torrent bytes is not supported in remote mode".to_string())
    }

    fn get_peer_telemetry(
        &self,
        _id: &str,
    ) -> Result<Vec<crate::models::peering::PeerTelemetry>, String> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// A fake rtorrent SCGI endpoint: validates the request is well-formed and
    /// answers by method name (version probe, list, file/tracker multicalls).
    fn spawn_fake_server() -> (ScgiTarget, std::sync::Arc<AtomicBool>) {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = std::sync::Arc::new(AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&served);

        std::thread::spawn(move || loop {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).unwrap_or(0);
            // Minimal SCGI validation: contains netstring colon+comma.
            let request = &buf[..n];
            let response = if request.contains(&b':') && request.contains(&b',') {
                let text = String::from_utf8_lossy(request);
                if text.contains("system.client_version") {
                    "<?xml version=\"1.0\"?><methodResponse><params><param><value><string>0.9.8</string></value></param></params></methodResponse>".to_string()
                } else if text.contains("d.multicall2") {
                    r#"<?xml version="1.0"?><methodResponse><params><param><value><array><data>
                        <value><array><data>
                            <value><string>BF4A97C0</string></value>
                            <value><string>Tokyo Story (1953)</string></value>
                            <value><string>/srv/data/Tokyo Story</string></value>
                            <value><i8>34500000000</i8></value>
                            <value><i8>34500000000</i8></value>
                            <value><i8>52000000000</i8></value>
                            <value><i8>0</i8></value>
                            <value><i8>1200000</i8></value>
                            <value><i8>1506</i8></value>
                            <value><i8>1</i8></value>
                            <value><i8>1</i8></value>
                            <value><i8>42</i8></value>
                            <value><i8>8</i8></value>
                            <value><string>movies</string></value>
                            <value><i8>1728800000</i8></value>
                            <value><i8>4194304</i8></value>
                            <value><i8>8225</i8></value>
                        </data></array></value>
                    </data></array></value></param></params></methodResponse>"#
                        .to_string()
                } else {
                    // f.multicall / t.multicall: empty detail arrays
                    "<?xml version=\"1.0\"?><methodResponse><params><param><value><array><data></data></array></value></param></params></methodResponse>".to_string()
                }
            } else {
                String::new()
            };
            let _ = stream.write_all(response.as_bytes());
            flag.store(true, Ordering::SeqCst);
        });

        (ScgiTarget::Tcp("127.0.0.1".into(), port), served)
    }

    #[test]
    fn test_list_torrents_maps_rtorrent_row() {
        let (target, _) = spawn_fake_server();
        let engine = RtorrentRemoteEngine::new(target);

        let torrents = engine.list_torrents().unwrap();
        assert_eq!(torrents.len(), 1);
        let t = &torrents[0];
        assert_eq!(t.id, "bf4a97c0"); // hashes normalized to lowercase
        assert_eq!(t.info_hash, "bf4a97c0");
        assert_eq!(t.name, "Tokyo Story (1953)");
        assert_eq!(t.status, "Seeding"); // state=1, complete=1
        assert_eq!(t.ratio, 1.506); // rtorrent per-mille 1506
        assert_eq!(t.upload_rate, 1_200_000);
        assert_eq!(t.seeds, Some(42));
        assert_eq!(t.category, "movies"); // d.custom1
        assert_eq!(t.added_on, 1_728_800_000); // d.timestamp.started
        assert_eq!(t.total_pieces, 8225);
        assert_eq!(t.piece_size, 4_194_304);
    }

    #[test]
    fn test_kill_switch_flag() {
        let engine = RtorrentRemoteEngine::new(ScgiTarget::Tcp("127.0.0.1".into(), 1));
        assert!(!engine.is_kill_switch_active());
        engine.kill_switch.store(true, Ordering::SeqCst);
        assert!(engine.is_kill_switch_active());
        engine.reset_kill_switch().unwrap();
        assert!(!engine.is_kill_switch_active());
    }

    #[test]
    fn test_unsupported_methods_fail_loudly() {
        let engine = RtorrentRemoteEngine::new(ScgiTarget::Tcp("127.0.0.1".into(), 1));
        assert!(engine.toggle_super_seeding("x").is_err());
        assert!(engine.toggle_sequential_download("x").is_err());
        assert!(engine.rename_torrent("x", "y").is_err());
        assert!(engine.supports_hook(EngineHook::PauseResumeKillSwitch));
        assert!(!engine.supports_hook(EngineHook::SocketInterfaceBinding));
    }
}
