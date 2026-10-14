//! Remote engine client: talks to a headless `nebulae-server` instance over
//! HTTP (token-authenticated) instead of running a local engine. The desktop
//! interface is unchanged; the engine behind it runs on the NAS/server.

use crate::commands::torrent::TorrentItem;
use crate::engine::traits::{EngineHook, EngineStatus, TorrentEngine};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub struct RemoteServerEngine {
    base_url: String,
    token: String,
    client: reqwest::blocking::Client,
    kill_switch: AtomicBool,
}

impl RemoteServerEngine {
    pub fn new(base_url: String, token: String) -> Self {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self {
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            token: token.trim().to_string(),
            client,
            kill_switch: AtomicBool::new(false),
        }
    }

    fn send(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<reqwest::blocking::Response, String> {
        let response = self
            .client
            .request(method, format!("{}{}", self.base_url, path))
            .bearer_auth(&self.token)
            .send()
            .map_err(|e| format!("nebulae-server: request failed: {e}"))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(format!(
                "nebulae-server: HTTP {status}: {}",
                body.lines().next().unwrap_or("no details")
            ));
        }
        Ok(response)
    }

    fn unit(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<String>,
    ) -> Result<(), String> {
        let request = self
            .client
            .request(method, format!("{}{}", self.base_url, path))
            .bearer_auth(&self.token);
        let request = match body {
            Some(b) => request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(b),
            None => request,
        };
        let response = request
            .send()
            .map_err(|e| format!("nebulae-server: request failed: {e}"))?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().unwrap_or_default();
            return Err(format!(
                "nebulae-server: HTTP {status}: {}",
                text.lines().next().unwrap_or("no details")
            ));
        }
        Ok(())
    }
}

impl TorrentEngine for RemoteServerEngine {
    fn list_torrents(&self) -> Result<Vec<TorrentItem>, String> {
        let items = self
            .send(reqwest::Method::GET, "/api/v1/torrents")?
            .json::<Vec<TorrentItem>>()
            .map_err(|e| format!("nebulae-server: bad torrent list: {e}"))?;
        Ok(items)
    }

    fn add_torrent(&self, source: &str, download_dir: Option<&str>) -> Result<String, String> {
        let payload = serde_json::json!({
            "source": source,
            "download_dir": download_dir,
        });
        // The server bridge discovers the new hash on the next list refresh
        // (same as the direct rtorrent engine).
        self.unit(
            reqwest::Method::POST,
            "/api/v1/torrents",
            Some(payload.to_string()),
        )
        .map(|_| String::new())
    }

    fn add_torrent_from_bytes(
        &self,
        bytes: &[u8],
        download_dir: Option<&str>,
    ) -> Result<String, String> {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        let payload = serde_json::json!({
            "source_b64": STANDARD.encode(bytes),
            "download_dir": download_dir,
        });
        self.unit(
            reqwest::Method::POST,
            "/api/v1/torrents",
            Some(payload.to_string()),
        )
        .map(|_| String::new())
    }

    fn remove_torrent(&self, id: &str, delete_files: bool) -> Result<(), String> {
        let flag = if delete_files { "true" } else { "false" };
        self.unit(
            reqwest::Method::DELETE,
            &format!("/api/v1/torrents/{id}?delete_files={flag}"),
            None,
        )
    }

    fn pause_torrent(&self, id: &str) -> Result<(), String> {
        self.unit(
            reqwest::Method::POST,
            &format!("/api/v1/torrents/{id}/pause"),
            None,
        )
    }

    fn resume_torrent(&self, id: &str) -> Result<(), String> {
        self.unit(
            reqwest::Method::POST,
            &format!("/api/v1/torrents/{id}/resume"),
            None,
        )
    }

    fn pause_all(&self) -> Result<(), String> {
        self.unit(reqwest::Method::POST, "/api/v1/torrents/pause-all", None)
    }

    fn resume_all(&self) -> Result<(), String> {
        self.unit(reqwest::Method::POST, "/api/v1/torrents/resume-all", None)
    }

    // The kill switch lives on the server (it owns the torrents); the local
    // flag mirrors server state so the desktop UI stays honest.
    fn trigger_kill_switch(&self) -> Result<(), String> {
        self.unit(
            reqwest::Method::POST,
            "/api/v1/kill-switch",
            Some(serde_json::json!({"enable": true}).to_string()),
        )?;
        self.kill_switch.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn reset_kill_switch(&self) -> Result<(), String> {
        self.unit(
            reqwest::Method::POST,
            "/api/v1/kill-switch",
            Some(serde_json::json!({"enable": false}).to_string()),
        )?;
        self.kill_switch.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_kill_switch_active(&self) -> bool {
        self.kill_switch.load(Ordering::SeqCst)
    }

    fn engine_status(&self) -> EngineStatus {
        let status = self
            .send(reqwest::Method::GET, "/api/v1/status")
            .and_then(|r| {
                r.json::<EngineStatus>()
                    .map_err(|e| format!("nebulae-server: bad status: {e}"))
            });
        match status {
            Ok(s) => {
                self.kill_switch
                    .store(s.kill_switch_engaged, Ordering::SeqCst);
                s
            }
            // Unreachable server reports itself honestly: zero counts, switch
            // state from the last successful call.
            Err(_) => EngineStatus {
                active_torrents: 0,
                paused_torrents: 0,
                kill_switch_engaged: self.kill_switch.load(Ordering::SeqCst),
                bound_interface: None,
            },
        }
    }

    fn supports_hook(&self, hook: EngineHook) -> bool {
        matches!(
            hook,
            EngineHook::PauseResumeKillSwitch | EngineHook::PrivateTorrentRestrictions
        )
    }

    fn force_recheck(&self, id: &str) -> Result<(), String> {
        self.unit(
            reqwest::Method::POST,
            &format!("/api/v1/torrents/{id}/recheck"),
            None,
        )
    }

    fn set_location(&self, id: &str, new_path: &str) -> Result<(), String> {
        let payload = serde_json::json!({ "path": new_path });
        self.unit(
            reqwest::Method::POST,
            &format!("/api/v1/torrents/{id}/location"),
            Some(payload.to_string()),
        )
    }

    fn set_torrent_limits(
        &self,
        id: &str,
        dl_kib: u32,
        ul_kib: u32,
        ratio_limit: f32,
    ) -> Result<(), String> {
        let payload = serde_json::json!({
            "dl_kib": dl_kib,
            "ul_kib": ul_kib,
            "ratio_limit": ratio_limit,
        });
        self.unit(
            reqwest::Method::POST,
            &format!("/api/v1/torrents/{id}/limits"),
            Some(payload.to_string()),
        )
    }

    // The following have no wire format yet in the server bridge; they fail
    // loudly instead of pretending.
    fn reannounce(&self, _id: &str) -> Result<(), String> {
        Err("nebulae-server (preview): reannounce is not available in remote mode yet".to_string())
    }

    fn set_category(&self, _id: &str, _category: &str) -> Result<(), String> {
        Err("nebulae-server (preview): categories are not available in remote mode yet".to_string())
    }

    fn toggle_sequential_download(&self, _id: &str) -> Result<bool, String> {
        Err(
            "nebulae-server (preview): sequential download is not available in remote mode yet"
                .to_string(),
        )
    }

    fn toggle_first_last_piece_priority(&self, _id: &str) -> Result<bool, String> {
        Err("nebulae-server (preview): first/last piece priority is not available in remote mode yet".to_string())
    }

    fn toggle_super_seeding(&self, _id: &str) -> Result<bool, String> {
        Err(
            "nebulae-server (preview): super-seeding is not available in remote mode yet"
                .to_string(),
        )
    }

    fn set_file_priority(&self, _id: &str, _file_index: u32, _priority: u8) -> Result<(), String> {
        Err(
            "nebulae-server (preview): per-file priority is not available in remote mode yet"
                .to_string(),
        )
    }

    fn rename_torrent(&self, _id: &str, _new_name: &str) -> Result<(), String> {
        Err("nebulae-server (preview): renaming is not available in remote mode yet".to_string())
    }

    fn rename_torrent_file(
        &self,
        _id: &str,
        _file_index: u32,
        _new_file_name: &str,
    ) -> Result<(), String> {
        Err(
            "nebulae-server (preview): file renaming is not available in remote mode yet"
                .to_string(),
        )
    }

    fn rename_torrent_folder(
        &self,
        _id: &str,
        _old_folder_path: &str,
        _new_folder_name: &str,
    ) -> Result<(), String> {
        Err(
            "nebulae-server (preview): folder renaming is not available in remote mode yet"
                .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// One-shot HTTP/1.1 fake server: asserts the bearer token, replies with
    /// `body`. Returns the bound address.
    fn spawn_fake_server(token: &'static str, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let mut raw = Vec::new();
                // Read until end of headers (GET/POST with small bodies).
                loop {
                    let read = stream.read(&mut buf).unwrap_or(0);
                    if read == 0 {
                        break;
                    }
                    raw.extend_from_slice(&buf[..read]);
                    if raw.windows(4).any(|w| w == b"\r\n\r\n") && raw.len() > 200 {
                        break;
                    }
                    if raw.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&raw).to_lowercase();
                let authorized = text.contains(&format!("authorization: bearer {token}"));
                let status = if authorized {
                    "200 OK"
                } else {
                    "401 Unauthorized"
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    const STATUS_JSON: &str = r#"{"active_torrents":2,"paused_torrents":1,"kill_switch_engaged":false,"bound_interface":null}"#;

    #[test]
    fn test_engine_status_round_trip() {
        let url = spawn_fake_server("tok-123", STATUS_JSON);
        let engine = RemoteServerEngine::new(url, "tok-123".to_string());
        let status = engine.engine_status();
        assert_eq!(status.active_torrents, 2);
        assert_eq!(status.paused_torrents, 1);
        assert!(!status.kill_switch_engaged);
        assert!(!engine.is_kill_switch_active());
    }

    #[test]
    fn test_wrong_token_fails_loudly() {
        let url = spawn_fake_server("tok-123", STATUS_JSON);
        let engine = RemoteServerEngine::new(url, "wrong-token".to_string());
        let status = engine.engine_status();
        // Unreachable/unauthorized degrades to a zeroed, honest status.
        assert_eq!(status.active_torrents, 0);
        // list_torrents must surface the error, not pretend the list is empty.
        assert!(engine.list_torrents().is_err());
    }

    #[test]
    fn test_list_torrents_round_trip() {
        let body = r#"[{"id":"ABC123","name":"Tokyo Story (1953)","total_bytes":1000000,
            "downloaded_bytes":500000,"uploaded_bytes":1000,"download_rate":1200,"upload_rate":300,
            "progress":0.5,"status":"Downloading","eta_seconds":417,"seeds":3,"peers":7,"ratio":0.001,
            "category":"","is_media":false,"poster_url":null,"info_hash":"ABC123","save_path":"/data/downloads",
            "added_on":1700000000,"completed_on":null,"comment":"","created_by":"rtorrent",
            "piece_size":1048576,"total_pieces":1,"sequential_download":false,"first_last_piece_prio":false,
            "super_seeding":false,"tags":[],"files":[],"trackers":[],"download_limit_kib":0,"upload_limit_kib":0}]"#;
        let url = spawn_fake_server("tok-123", body);
        let engine = RemoteServerEngine::new(url, "tok-123".to_string());
        let list = engine.list_torrents().expect("list must succeed");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Tokyo Story (1953)");
        assert_eq!(list[0].status, "Downloading");
        assert_eq!(list[0].peers, 7);
    }

    #[test]
    fn test_preview_methods_fail_loudly() {
        let engine = RemoteServerEngine::new("http://127.0.0.1:1".to_string(), "t".to_string());
        assert!(engine.reannounce("x").is_err());
        assert!(engine.set_category("x", "y").is_err());
        assert!(engine.toggle_super_seeding("x").is_err());
        assert!(engine.rename_torrent("x", "y").is_err());
    }
}
