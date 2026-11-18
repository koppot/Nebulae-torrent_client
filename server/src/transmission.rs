//! Transmission RPC client for nebulae-server.
//!
//! Wraps Transmission's JSON-RPC API (spec:
//! https://github.com/transmission/transmission/blob/main/docs/rpc-spec.md)
//! into async methods matching the nebulae-server REST contract.

use crate::TorrentItem as ApiTorrentItem;
use crate::{TorrentFileItem, TorrentTrackerItem};
use reqwest::header::CONTENT_TYPE;
use serde_json::{json, Value};
use std::sync::RwLock;
use tracing::debug;

/// Transmission RPC session-ID header name.
const SESSION_ID_HEADER: &str = "X-Transmission-Session-Id";

pub struct TransmissionClient {
    url: String,
    client: reqwest::Client,
    auth: Option<(String, String)>,
    session_id: RwLock<String>,
}

impl TransmissionClient {
    pub fn new(url: &str, user: Option<&str>, pass: Option<&str>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("HTTP client build");

        Self {
            url: url.to_string(),
            client,
            auth: user.map(|u| (u.to_string(), pass.unwrap_or("").to_string())),
            session_id: RwLock::new(String::new()),
        }
    }

    /// Send a Transmission RPC request, handling the 409 session-id handshake.
    async fn rpc(&self, method: &str, arguments: Option<Value>) -> Result<Value, String> {
        let body = json!({
            "method": method,
            "arguments": arguments.unwrap_or(json!({})),
        });

        for attempt in 0..2 {
            let session_id = self.session_id.read().map_err(|e| e.to_string())?.clone();

            let mut req = self
                .client
                .post(&self.url)
                .header(CONTENT_TYPE, "application/json")
                .header(SESSION_ID_HEADER, &session_id)
                .json(&body);

            if let Some((ref user, ref pass)) = self.auth {
                req = req.basic_auth(user, Some(pass));
            }

            let response = req.send().await.map_err(|e| format!("Transmission RPC error: {e}"))?;

            if response.status().as_u16() == 409 {
                // Extract new session ID from the 409 response.
                if let Some(new_id) = response.headers().get(SESSION_ID_HEADER) {
                    let new_id = new_id.to_str().unwrap_or("").to_string();
                    debug!("Transmission session ID updated: {new_id}");
                    if let Ok(mut guard) = self.session_id.write() {
                        *guard = new_id;
                    }
                }
                if attempt == 0 {
                    continue;
                }
                return Err("Transmission RPC: repeated 409 session-id rejection".to_string());
            }

            if !response.status().is_success() {
                let status = response.status();
                let body_text = response.text().await.unwrap_or_default();
                return Err(format!("Transmission RPC HTTP {status}: {body_text}"));
            }

            let result: Value = response.json().await.map_err(|e| format!("parse error: {e}"))?;

            if result.get("result").and_then(|r| r.as_str()) != Some("success") {
                let msg = result
                    .get("result")
                    .and_then(|r| r.as_str())
                    .unwrap_or("unknown error");
                return Err(format!("Transmission RPC: {msg}"));
            }

            return Ok(result.get("arguments").cloned().unwrap_or(json!({})));
        }

        Err("Transmission RPC: max retries exceeded".to_string())
    }

    /// List all torrents with full details.
    pub async fn list_torrents(&self) -> Result<Vec<ApiTorrentItem>, String> {
        let fields = json!([
            "id", "name", "hashString", "totalSize", "downloadedEver",
            "uploadedEver", "rateDownload", "rateUpload", "percentDone",
            "status", "eta", "peersGettingFromUs", "peersSendingToUs",
            "uploadRatio", "downloadDir", "addedDate", "doneDate",
            "comment", "creator", "pieceSize", "pieceCount",
            "isPrivate", "labels",
            "files", "fileStats", "trackerStats",
            "downloadLimit", "downloadLimited", "uploadLimit", "uploadLimited",
            "bandwidthPriority", "seedRatioLimit", "seedRatioMode",
            "sequentialDownload"
        ]);

        let args = self
            .rpc("torrent-get", Some(json!({ "fields": fields })))
            .await?;

        let torrents = args
            .get("torrents")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();

        let items: Vec<ApiTorrentItem> = torrents
            .into_iter()
            .filter_map(|t| parse_torrent(&t))
            .collect();

        Ok(items)
    }

    /// Add a torrent by magnet URI, URL, or base64-encoded .torrent file.
    pub async fn add_torrent(
        &self,
        source: &str,
        download_dir: Option<&str>,
    ) -> Result<String, String> {
        let mut args = json!({});

        if source.starts_with("magnet:") || source.starts_with("http://") || source.starts_with("https://") {
            args["filename"] = json!(source);
        } else {
            // Assume base64-encoded metainfo.
            args["metainfo"] = json!(source);
        }

        if let Some(dir) = download_dir {
            args["download-dir"] = json!(dir);
        }

        let result = self.rpc("torrent-add", Some(args)).await?;

        // Transmission returns either "torrent-added" or "torrent-duplicate".
        let added = result
            .get("torrent-added")
            .or_else(|| result.get("torrent-duplicate"));

        match added {
            Some(t) => {
                let hash = t
                    .get("hashString")
                    .and_then(|h| h.as_str())
                    .unwrap_or("")
                    .to_string();
                Ok(hash)
            }
            None => Err("Transmission did not return torrent info after add".to_string()),
        }
    }

    /// Pause a torrent by info hash.
    pub async fn pause_torrent(&self, hash: &str) -> Result<(), String> {
        let id = self.resolve_id(hash).await?;
        self.rpc("torrent-stop", Some(json!({ "ids": [id] }))).await?;
        Ok(())
    }

    /// Resume a torrent by info hash.
    pub async fn resume_torrent(&self, hash: &str) -> Result<(), String> {
        let id = self.resolve_id(hash).await?;
        self.rpc("torrent-start", Some(json!({ "ids": [id] }))).await?;
        Ok(())
    }

    /// Pause all torrents.
    pub async fn pause_all(&self) -> Result<(), String> {
        self.rpc("torrent-stop", None).await?;
        Ok(())
    }

    /// Resume all torrents.
    pub async fn resume_all(&self) -> Result<(), String> {
        self.rpc("torrent-start", None).await?;
        Ok(())
    }

    /// Remove a torrent by info hash.
    pub async fn remove_torrent(&self, hash: &str, delete_files: bool) -> Result<(), String> {
        let id = self.resolve_id(hash).await?;
        self.rpc(
            "torrent-remove",
            Some(json!({
                "ids": [id],
                "delete-local-data": delete_files,
            })),
        )
        .await?;
        Ok(())
    }

    /// Force reannounce a torrent.
    pub async fn reannounce(&self, hash: &str) -> Result<(), String> {
        let id = self.resolve_id(hash).await?;
        self.rpc("torrent-reannounce", Some(json!({ "ids": [id] }))).await?;
        Ok(())
    }

    /// Force recheck a torrent.
    pub async fn recheck(&self, hash: &str) -> Result<(), String> {
        let id = self.resolve_id(hash).await?;
        self.rpc("torrent-verify", Some(json!({ "ids": [id] }))).await?;
        Ok(())
    }

    /// Set per-torrent speed/ratio limits.
    pub async fn set_limits(
        &self,
        hash: &str,
        dl_kib: u32,
        ul_kib: u32,
        ratio_limit: f32,
    ) -> Result<(), String> {
        let id = self.resolve_id(hash).await?;
        let mut args = json!({ "ids": [id] });

        if dl_kib > 0 {
            args["downloadLimited"] = json!(true);
            args["downloadLimit"] = json!(dl_kib);
        } else {
            args["downloadLimited"] = json!(false);
        }

        if ul_kib > 0 {
            args["uploadLimited"] = json!(true);
            args["uploadLimit"] = json!(ul_kib);
        } else {
            args["uploadLimited"] = json!(false);
        }

        if ratio_limit > 0.0 {
            args["seedRatioMode"] = json!(1); // 1 = use this ratio
            args["seedRatioLimit"] = json!(ratio_limit);
        } else {
            args["seedRatioMode"] = json!(0); // 0 = global default
        }

        self.rpc("torrent-set", Some(args)).await?;
        Ok(())
    }

    /// Resolve an info hash to Transmission's internal numeric ID.
    async fn resolve_id(&self, hash: &str) -> Result<i64, String> {
        let args = self
            .rpc(
                "torrent-get",
                Some(json!({
                    "ids": [hash],
                    "fields": ["id", "hashString"],
                })),
            )
            .await?;

        let torrents = args
            .get("torrents")
            .and_then(|t| t.as_array())
            .ok_or_else(|| format!("torrent not found: {hash}"))?;

        if torrents.is_empty() {
            return Err(format!("torrent not found: {hash}"));
        }

        torrents[0]
            .get("id")
            .and_then(|id| id.as_i64())
            .ok_or_else(|| format!("invalid torrent ID for hash: {hash}"))
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Map Transmission status codes to human-readable labels.
fn transmission_status(code: i64) -> &'static str {
    match code {
        0 => "Paused",
        1 => "Queued for checking",
        2 => "Checking",
        3 => "Queued for downloading",
        4 => "Downloading",
        5 => "Queued for seeding",
        6 => "Seeding",
        _ => "Unknown",
    }
}

/// Determine if a torrent name looks like media content.
fn is_media_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    // Common video/disc indicators.
    lower.contains("bdmv")
        || lower.contains("video_ts")
        || lower.ends_with(".mkv")
        || lower.ends_with(".mp4")
        || lower.ends_with(".avi")
        || lower.ends_with(".m2ts")
        || lower.ends_with(".iso")
}

/// Parse a Transmission torrent JSON object into the API response type.
fn parse_torrent(t: &Value) -> Option<ApiTorrentItem> {
    let _id = t.get("id")?.as_i64()?;
    let name = t.get("name")?.as_str().unwrap_or("").to_string();
    let hash = t.get("hashString")?.as_str().unwrap_or("").to_string();
    let total_size = t.get("totalSize").and_then(|v| v.as_u64()).unwrap_or(0);
    let downloaded = t.get("downloadedEver").and_then(|v| v.as_u64()).unwrap_or(0);
    let uploaded = t.get("uploadedEver").and_then(|v| v.as_u64()).unwrap_or(0);
    let dl_rate = t.get("rateDownload").and_then(|v| v.as_u64()).unwrap_or(0);
    let ul_rate = t.get("rateUpload").and_then(|v| v.as_u64()).unwrap_or(0);
    let percent = t.get("percentDone").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let status_code = t.get("status").and_then(|v| v.as_i64()).unwrap_or(0);
    let eta = t.get("eta").and_then(|v| v.as_i64()).and_then(|e| if e > 0 { Some(e as u64) } else { None });
    let ratio = t.get("uploadRatio").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let save_path = t.get("downloadDir").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let added = t.get("addedDate").and_then(|v| v.as_u64()).unwrap_or(0);
    let done = t.get("doneDate").and_then(|v| v.as_u64()).and_then(|d| if d > 0 { Some(d) } else { None });
    let comment = t.get("comment").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let creator = t.get("creator").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let piece_size = t.get("pieceSize").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let piece_count = t.get("pieceCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let labels: Vec<String> = t
        .get("labels")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|l| l.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Count seeds and peers from trackerStats.
    let (seeds, peers) = t
        .get("trackerStats")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter().fold((0u32, 0u32), |(s, p), ts| {
                let sc = ts.get("seederCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let pc = ts.get("leecherCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                (s.max(sc), p.max(pc))
            })
        })
        .unwrap_or((0, 0));

    // Parse files.
    let files: Vec<TorrentFileItem> = {
        let file_list = t.get("files").and_then(|v| v.as_array());
        let stat_list = t.get("fileStats").and_then(|v| v.as_array());
        match (file_list, stat_list) {
            (Some(fl), Some(sl)) => fl
                .iter()
                .zip(sl.iter())
                .enumerate()
                .map(|(i, (f, s))| {
                    let size = f.get("length").and_then(|v| v.as_u64()).unwrap_or(0);
                    let completed = f.get("bytesCompleted").and_then(|v| v.as_u64()).unwrap_or(0);
                    let progress = if size > 0 { completed as f32 / size as f32 } else { 0.0 };
                    let priority = s.get("priority").and_then(|v| v.as_i64()).unwrap_or(0);
                    TorrentFileItem {
                        index: i as u32,
                        name: f.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        size_bytes: size,
                        progress,
                        priority: match priority {
                            -1 => 0, // low
                            0 => 1,  // normal
                            1 => 2,  // high
                            _ => 1,
                        },
                    }
                })
                .collect(),
            _ => Vec::new(),
        }
    };

    // Parse trackers.
    let trackers: Vec<TorrentTrackerItem> = t
        .get("trackerStats")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|ts| {
                    let announce = ts.get("announce").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let has_announced = ts.get("hasAnnounced").and_then(|v| v.as_bool()).unwrap_or(false);
                    let last_result = ts.get("lastAnnounceResult").and_then(|v| v.as_str()).unwrap_or("");
                    let status_str = if !has_announced {
                        "Not contacted yet"
                    } else if last_result == "Success" || last_result.is_empty() {
                        "Working"
                    } else {
                        "Error"
                    };

                    TorrentTrackerItem {
                        tier: ts.get("tier").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                        url: announce,
                        status: status_str.to_string(),
                        seeds: ts.get("seederCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                        peers: ts.get("leecherCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                        downloaded: ts.get("downloadCount").and_then(|v| v.as_u64()).unwrap_or(0),
                        message: ts
                            .get("lastAnnounceResult")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    // Speed limits.
    let dl_limited = t.get("downloadLimited").and_then(|v| v.as_bool()).unwrap_or(false);
    let dl_limit = if dl_limited {
        t.get("downloadLimit").and_then(|v| v.as_u64()).unwrap_or(0) as u32
    } else {
        0
    };
    let ul_limited = t.get("uploadLimited").and_then(|v| v.as_bool()).unwrap_or(false);
    let ul_limit = if ul_limited {
        t.get("uploadLimit").and_then(|v| v.as_u64()).unwrap_or(0) as u32
    } else {
        0
    };

    // Determine completed status.
    let status_str = if percent >= 1.0 && status_code == 0 {
        "Completed"
    } else {
        transmission_status(status_code)
    };

    Some(ApiTorrentItem {
        id: hash.clone(),
        name: name.clone(),
        total_bytes: total_size,
        downloaded_bytes: downloaded,
        uploaded_bytes: uploaded,
        download_rate: dl_rate,
        upload_rate: ul_rate,
        progress: percent,
        status: status_str.to_string(),
        eta_seconds: eta,
        seeds,
        peers,
        ratio,
        category: labels.first().cloned().unwrap_or_default(),
        is_media: is_media_name(&name),
        poster_url: None,
        info_hash: hash,
        save_path,
        added_on: added,
        completed_on: done,
        comment,
        created_by: creator,
        piece_size,
        total_pieces: piece_count,
        sequential_download: t.get("sequentialDownload").and_then(|v| v.as_bool()).unwrap_or(false),
        first_last_piece_prio: false,
        super_seeding: false,
        tags: labels,
        files,
        trackers,
        download_limit_kib: dl_limit,
        upload_limit_kib: ul_limit,
    })
}
