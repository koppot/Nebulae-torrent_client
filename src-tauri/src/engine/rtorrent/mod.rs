//! Nebulae Server engine: drives a remote rtorrent daemon over SCGI.
//!
//! The Nebulae interface stays exactly the same; the engine behind it runs on
//! a server (Unraid, TrueNAS, any Docker host) instead of locally. This
//! implements the shared [`TorrentEngine`] trait against rtorrent's XML-RPC
//! command set (`d.*` commands).

pub mod rpc;
pub mod scgi;

use crate::commands::torrent::{TorrentFileItem, TorrentItem};
use crate::engine::traits::{EngineHook, EngineStatus, TorrentEngine};
use scgi::{call, ScgiTarget};
use std::sync::atomic::{AtomicBool, Ordering};

/// rtorrent reports ratios as per-mille (x1000).
const RTORRENT_RATIO_SCALE: f32 = 1000.0;

pub struct RtorrentRemoteEngine {
    target: ScgiTarget,
    kill_switch: AtomicBool,
}

impl RtorrentRemoteEngine {
    pub fn new(target: ScgiTarget) -> Self {
        Self {
            target,
            kill_switch: AtomicBool::new(false),
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

        let added_on = text(13).parse::<u64>().unwrap_or(0);

        Ok(TorrentItem {
            id: text(0),
            name: text(1),
            total_bytes,
            downloaded_bytes,
            uploaded_bytes: int(5).max(0) as u64,
            download_rate,
            upload_rate,
            progress,
            status,
            eta_seconds,
            seeds: int(11).max(0) as u32,
            peers: int(12).max(0) as u32,
            ratio,
            category: String::new(),
            is_media: false,
            poster_url: None,
            info_hash: text(0),
            save_path: text(2),
            added_on,
            completed_on: None,
            comment: String::new(),
            created_by: "rtorrent".to_string(),
            piece_size: int(14).max(0) as u32,
            total_pieces: int(15).max(0) as u32,
            sequential_download: false,
            first_last_piece_prio: false,
            super_seeding: false,
            tags: Vec::new(),
            files: Vec::<TorrentFileItem>::new(),
            trackers: Vec::new(),
            download_limit_kib: 0,
            upload_limit_kib: 0,
        })
    }
}

/// Columns requested in every `d.multicall2` row. Order must match
/// [`RtorrentRemoteEngine::map_torrent_row`].
const LIST_COLUMNS: [&str; 16] = [
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
    "d.seeds_connected=",
    "d.peers_connected=",
    "d.custom=addtime",
    "d.piece_size=",
    "d.size_pieces=",
];

impl TorrentEngine for RtorrentRemoteEngine {
    fn list_torrents(&self) -> Result<Vec<TorrentItem>, String> {
        let params: Vec<rpc::RpcValue> = std::iter::once(rpc::RpcValue::Str(String::new()))
            .chain(
                LIST_COLUMNS
                    .iter()
                    .map(|c| rpc::RpcValue::Str(c.to_string())),
            )
            .collect();
        let response = self.rpc("d.multicall2", &params)?;
        let rows = response
            .as_array()
            .ok_or_else(|| "rtorrent: unexpected multicall response".to_string())?;
        rows.iter()
            .map(|row| {
                let cells = row
                    .as_array()
                    .ok_or_else(|| "rtorrent: malformed multicall row".to_string())?;
                Self::map_torrent_row(cells)
            })
            .collect()
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
        self.rpc(method, &[rpc::RpcValue::Str(String::new()), param])?;
        if let Some(dir) = download_dir {
            // New torrents land in the requested directory; existing ones keep theirs.
            // (Directory is applied per-add via d.directory.set on the newly loaded item
            // is not addressable here without its hash; rtorrent picks it up from the
            // session config. Recorded as a known preview limitation.)
            let _ = dir;
        }
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

    fn set_torrent_limits(
        &self,
        id: &str,
        dl_kib: u32,
        ul_kib: u32,
        _ratio_limit: f32,
    ) -> Result<(), String> {
        self.rpc(
            "d.throttle.down.set",
            &[Self::torrent_hash(id), rpc::RpcValue::Int(dl_kib as i64)],
        )?;
        self.rpc(
            "d.throttle.up.set",
            &[Self::torrent_hash(id), rpc::RpcValue::Int(ul_kib as i64)],
        )
        .map(|_| ())
    }

    // Preview limitations: the commands below either have no rtorrent equivalent
    // or need per-file RPC iteration scheduled for the next milestone.
    fn reannounce(&self, _id: &str) -> Result<(), String> {
        Err("rtorrent: reannounce is not wired in the nebulae-server preview".to_string())
    }

    fn set_category(&self, _id: &str, _category: &str) -> Result<(), String> {
        Err("rtorrent: categories are not supported (no label mapping yet)".to_string())
    }

    fn toggle_sequential_download(&self, _id: &str) -> Result<bool, String> {
        Err("rtorrent: sequential download is not wired in the preview".to_string())
    }

    fn toggle_first_last_piece_priority(&self, _id: &str) -> Result<bool, String> {
        Err("rtorrent: first/last piece priority has no rtorrent equivalent".to_string())
    }

    fn toggle_super_seeding(&self, _id: &str) -> Result<bool, String> {
        Err("rtorrent: super-seeding is not available".to_string())
    }

    fn set_file_priority(&self, _id: &str, _file_index: u32, _priority: u8) -> Result<(), String> {
        Err("rtorrent: per-file priority is not wired in the preview".to_string())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// A fake rtorrent SCGI endpoint: validates the request is well-formed and
    /// replies with one canned multicall row.
    fn spawn_fake_server() -> (ScgiTarget, std::sync::Arc<AtomicBool>) {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = std::sync::Arc::new(AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&served);

        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = vec![0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                // Minimal SCGI validation: contains netstring colon+comma.
                let request = &buf[..n];
                if request.contains(&b':')
                    && request.contains(&b',')
                    && !flag.load(Ordering::SeqCst)
                {
                    let response = r#"<?xml version="1.0"?><methodResponse><params><param><value><array><data>
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
                            <value><string>1728800000</string></value>
                            <value><i8>4194304</i8></value>
                            <value><i8>8225</i8></value>
                        </data></array></value>
                    </data></array></value></param></params></methodResponse>"#;
                    let _ = stream.write_all(response.as_bytes());
                }
                flag.store(true, Ordering::SeqCst);
            }
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
        assert_eq!(t.id, "BF4A97C0");
        assert_eq!(t.name, "Tokyo Story (1953)");
        assert_eq!(t.status, "Seeding"); // state=1, complete=1
        assert_eq!(t.ratio, 1.506); // rtorrent per-mille 1506
        assert_eq!(t.upload_rate, 1_200_000);
        assert_eq!(t.seeds, 42);
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
    fn test_preview_methods_report_honestly() {
        let engine = RtorrentRemoteEngine::new(ScgiTarget::Tcp("127.0.0.1".into(), 1));
        assert!(engine.toggle_super_seeding("x").is_err());
        assert!(engine.reannounce("x").is_err());
        assert!(engine.set_file_priority("x", 0, 1).is_err());
        assert!(engine.supports_hook(EngineHook::PauseResumeKillSwitch));
        assert!(!engine.supports_hook(EngineHook::SocketInterfaceBinding));
    }
}
