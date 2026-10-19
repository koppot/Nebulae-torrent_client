use crate::commands::torrent::{TorrentFileItem, TorrentTrackerItem};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RawMetainfo {
    pub announce: Option<String>,
    #[serde(rename = "announce-list")]
    pub announce_list: Option<Vec<Vec<String>>>,
    pub comment: Option<String>,
    #[serde(rename = "created by")]
    pub created_by: Option<String>,
    #[serde(rename = "creation date")]
    pub creation_date: Option<u64>,
    pub info: RawInfo,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RawInfo {
    pub name: Option<String>,
    #[serde(rename = "piece length")]
    pub piece_length: Option<u32>,
    #[serde(with = "serde_bytes", default)]
    pub pieces: Vec<u8>,
    pub length: Option<u64>,
    pub files: Option<Vec<RawFile>>,
    pub private: Option<u8>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RawFile {
    pub length: u64,
    pub path: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedTorrent {
    pub name: String,
    pub total_bytes: u64,
    pub piece_size: u32,
    pub total_pieces: u32,
    pub info_hash: String,
    pub raw_info_hash: [u8; 20],
    pub is_private: bool,
    pub is_multi_file: bool,
    pub files: Vec<TorrentFileItem>,
    pub trackers: Vec<TorrentTrackerItem>,
    pub comment: String,
    pub created_by: String,
}

impl ParsedTorrent {
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let bytes = std::fs::read(path.as_ref())
            .map_err(|e| format!("Failed to read .torrent file: {e}"))?;
        Self::parse_bytes(&bytes)
    }

    pub fn parse_bytes(bytes: &[u8]) -> Result<Self, String> {
        let metainfo: RawMetainfo = serde_bencode::from_bytes(bytes)
            .map_err(|e| format!("Failed to decode bencode torrent metainfo: {e}"))?;

        // Compute info_hash: SHA1 of bencoded 'info' dict
        let info_bytes = serde_bencode::to_bytes(&metainfo.info)
            .map_err(|e| format!("Failed to re-encode info dict for hash calculation: {e}"))?;

        let mut hasher = Sha1::new();
        hasher.update(&info_bytes);
        let hash_result = hasher.finalize();
        let mut raw_info_hash = [0u8; 20];
        raw_info_hash.copy_from_slice(&hash_result);
        let info_hash = hex::encode(raw_info_hash);

        let name = metainfo
            .info
            .name
            .clone()
            .unwrap_or_else(|| "Unknown Torrent".to_string());

        let piece_size = metainfo.info.piece_length.unwrap_or(1_048_576);
        let total_pieces = (metainfo.info.pieces.len() / 20) as u32;
        let is_private = metainfo.info.private.unwrap_or(0) == 1;

        let mut files = Vec::new();
        let total_bytes: u64;
        let is_multi_file: bool;

        if let Some(raw_files) = &metainfo.info.files {
            is_multi_file = true;
            let mut sum = 0u64;
            for (idx, f) in raw_files.iter().enumerate() {
                let file_path = f.path.join("/");
                sum += f.length;
                files.push(TorrentFileItem {
                    index: idx as u32,
                    name: file_path,
                    size_bytes: f.length,
                    progress: 0.0,
                    priority: 1,
                });
            }
            total_bytes = sum;
        } else {
            is_multi_file = false;
            let len = metainfo.info.length.unwrap_or(0);
            total_bytes = len;
            files.push(TorrentFileItem {
                index: 0,
                name: name.clone(),
                size_bytes: len,
                progress: 0.0,
                priority: 1,
            });
        }

        // Collect all unique tracker URLs from announce and announce-list
        let mut tracker_urls = Vec::new();
        if let Some(ann) = &metainfo.announce {
            if !ann.trim().is_empty() {
                tracker_urls.push((0u32, ann.trim().to_string()));
            }
        }
        if let Some(list) = &metainfo.announce_list {
            for (tier_idx, tier) in list.iter().enumerate() {
                for url in tier {
                    if !url.trim().is_empty() && !tracker_urls.iter().any(|(_, u)| u == url.trim())
                    {
                        tracker_urls.push((tier_idx as u32, url.trim().to_string()));
                    }
                }
            }
        }

        let trackers = tracker_urls
            .into_iter()
            .map(|(tier, url)| TorrentTrackerItem {
                tier,
                url,
                status: "Queued".to_string(),
                seeds: None,
                peers: None,
                downloaded: 0,
                message: "Awaiting announce".to_string(),
                last_announce: None,
                next_announce: None,
            })
            .collect();

        Ok(ParsedTorrent {
            name,
            total_bytes,
            piece_size,
            total_pieces,
            info_hash,
            raw_info_hash,
            is_private,
            is_multi_file,
            files,
            trackers,
            comment: metainfo.comment.unwrap_or_default(),
            created_by: metainfo
                .created_by
                .unwrap_or_else(|| "Nebulae Metainfo".to_string()),
        })
    }
}

pub mod hex {
    pub fn encode<T: AsRef<[u8]>>(data: T) -> String {
        data.as_ref().iter().map(|b| format!("{:02x}", b)).collect()
    }
}

/// Perform an HTTP/HTTPS tracker announce to fetch real seed and peer counts.
pub async fn announce_http_tracker(
    tracker_url: &str,
    info_hash: &[u8; 20],
    peer_id: &[u8; 20],
    port: u16,
    left_bytes: u64,
) -> Result<(u32, u32), String> {
    if !tracker_url.starts_with("http://") && !tracker_url.starts_with("https://") {
        return Err("Only HTTP/HTTPS trackers supported directly".to_string());
    }

    // Binary URL-encode info_hash and peer_id
    let mut encoded_hash = String::with_capacity(60);
    for byte in info_hash {
        encoded_hash.push_str(&format!("%{:02x}", byte));
    }

    let mut encoded_peer_id = String::with_capacity(60);
    for byte in peer_id {
        encoded_peer_id.push_str(&format!("%{:02x}", byte));
    }

    let separator = if tracker_url.contains('?') { "&" } else { "?" };
    let full_url = format!(
        "{}{}info_hash={}&peer_id={}&port={}&uploaded=0&downloaded=0&left={}&compact=1&event=started",
        tracker_url, separator, encoded_hash, encoded_peer_id, port, left_bytes
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        // FULL certificate/hostname validation, everywhere: an invalid or
        // misconfigured tracker cert fails the announce loudly instead of
        // being silently accepted (fail-closed contract).
        .user_agent("qBittorrent/4.6.5")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let resp = client
        .get(&full_url)
        .send()
        .await
        .map_err(|e| format!("Tracker request error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Tracker returned HTTP {}", resp.status()));
    }

    let body = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read tracker response body: {e}"))?;

    #[derive(Debug, Deserialize)]
    struct TrackerResponse {
        #[serde(rename = "failure reason")]
        failure_reason: Option<String>,
        complete: Option<u32>,
        incomplete: Option<u32>,
    }

    let tr_resp: TrackerResponse = serde_bencode::from_bytes(&body)
        .map_err(|e| format!("Failed to parse tracker bencode response: {e}"))?;

    if let Some(err) = tr_resp.failure_reason {
        return Err(format!("Tracker failure: {err}"));
    }

    let seeds = tr_resp.complete.unwrap_or(0);
    let peers = tr_resp.incomplete.unwrap_or(0);
    Ok((seeds, peers))
}

/// Create physical download directories and placeholder files on the filesystem.
pub fn ensure_torrent_storage(save_path: &Path, parsed: &ParsedTorrent) -> Result<PathBuf, String> {
    let content_dir = if parsed.is_multi_file {
        save_path.join(&parsed.name)
    } else {
        save_path.to_path_buf()
    };

    std::fs::create_dir_all(&content_dir).map_err(|e| {
        format!(
            "Failed to create download folder '{}': {e}",
            content_dir.display()
        )
    })?;

    // Create subdirectories for files if any
    for file in &parsed.files {
        let file_path = content_dir.join(&file.name);
        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                let _ = std::fs::create_dir_all(parent);
            }
        }

        // Touch placeholder file so user sees it in Finder
        if !file_path.exists() {
            let _ = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&file_path);
        }
    }

    Ok(content_dir)
}

/// Create a real .torrent file from a source file or directory.
pub fn build_torrent_file(
    source: &Path,
    output_dir_or_file: &Path,
    piece_size_kib: u32,
    trackers: &[String],
    is_private: bool,
    comment: &str,
    created_by: &str,
) -> Result<PathBuf, String> {
    if !source.exists() {
        return Err(format!("Source path '{}' does not exist", source.display()));
    }

    let piece_length = (piece_size_kib.max(16) as usize) * 1024;
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "content".to_string());

    let raw_info = if source.is_file() {
        let file_len = std::fs::metadata(source)
            .map_err(|e| format!("Failed to read metadata for '{}': {e}", source.display()))?
            .len();

        let mut f = std::fs::File::open(source)
            .map_err(|e| format!("Failed to open file '{}': {e}", source.display()))?;

        let mut pieces = Vec::new();
        let mut buf = vec![0u8; piece_length];
        use std::io::Read;
        loop {
            let mut read_in_piece = 0;
            while read_in_piece < piece_length {
                let n = f
                    .read(&mut buf[read_in_piece..])
                    .map_err(|e| format!("Error reading source file: {e}"))?;
                if n == 0 {
                    break;
                }
                read_in_piece += n;
            }
            if read_in_piece == 0 {
                break;
            }
            let mut hasher = Sha1::new();
            hasher.update(&buf[..read_in_piece]);
            pieces.extend_from_slice(&hasher.finalize());
            if read_in_piece < piece_length {
                break;
            }
        }

        RawInfo {
            name: Some(name.clone()),
            piece_length: Some(piece_length as u32),
            pieces,
            length: Some(file_len),
            files: None,
            private: Some(if is_private { 1 } else { 0 }),
        }
    } else if source.is_dir() {
        fn collect_files(
            root: &Path,
            dir: &Path,
            list: &mut Vec<(PathBuf, Vec<String>, u64)>,
        ) -> Result<(), String> {
            let entries = std::fs::read_dir(dir)
                .map_err(|e| format!("Failed to read directory '{}': {e}", dir.display()))?;
            for entry in entries {
                let entry = entry.map_err(|e| format!("Directory read error: {e}"))?;
                let path = entry.path();
                let fname = entry.file_name().to_string_lossy().to_string();
                if fname.starts_with('.') {
                    continue; // Skip hidden files like .DS_Store
                }
                if path.is_dir() {
                    collect_files(root, &path, list)?;
                } else if path.is_file() {
                    let rel = path
                        .strip_prefix(root)
                        .map_err(|e| format!("Strip prefix error: {e}"))?;
                    let comps: Vec<String> = rel
                        .iter()
                        .map(|c| c.to_string_lossy().to_string())
                        .collect();
                    let len = entry
                        .metadata()
                        .map_err(|e| format!("File metadata error: {e}"))?
                        .len();
                    list.push((path, comps, len));
                }
            }
            Ok(())
        }

        let mut file_list = Vec::new();
        collect_files(source, source, &mut file_list)?;
        if file_list.is_empty() {
            return Err("Cannot create torrent from an empty directory".to_string());
        }
        file_list.sort_by(|a, b| a.1.cmp(&b.1));

        let mut pieces = Vec::new();
        let mut buf = vec![0u8; piece_length];
        let mut buf_pos = 0;

        use std::io::Read;
        for (fpath, _, _) in &file_list {
            let mut file = std::fs::File::open(fpath)
                .map_err(|e| format!("Failed to open '{}': {e}", fpath.display()))?;
            loop {
                let n = file
                    .read(&mut buf[buf_pos..])
                    .map_err(|e| format!("Read error on '{}': {e}", fpath.display()))?;
                if n == 0 {
                    break;
                }
                buf_pos += n;
                if buf_pos == piece_length {
                    let mut hasher = Sha1::new();
                    hasher.update(&buf);
                    pieces.extend_from_slice(&hasher.finalize());
                    buf_pos = 0;
                }
            }
        }
        if buf_pos > 0 {
            let mut hasher = Sha1::new();
            hasher.update(&buf[..buf_pos]);
            pieces.extend_from_slice(&hasher.finalize());
        }

        let raw_files: Vec<RawFile> = file_list
            .into_iter()
            .map(|(_, path, length)| RawFile { length, path })
            .collect();

        RawInfo {
            name: Some(name.clone()),
            piece_length: Some(piece_length as u32),
            pieces,
            length: None,
            files: Some(raw_files),
            private: Some(if is_private { 1 } else { 0 }),
        }
    } else {
        return Err("Source path must be a regular file or directory".to_string());
    };

    let announce = trackers.first().cloned();
    let announce_list = if trackers.len() > 1 {
        Some(trackers.iter().map(|t| vec![t.clone()]).collect())
    } else {
        None
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let raw_meta = RawMetainfo {
        announce,
        announce_list,
        comment: if comment.trim().is_empty() {
            None
        } else {
            Some(comment.trim().to_string())
        },
        created_by: if created_by.trim().is_empty() {
            None
        } else {
            Some(created_by.trim().to_string())
        },
        creation_date: Some(now),
        info: raw_info,
    };

    let bencode_bytes = serde_bencode::to_bytes(&raw_meta)
        .map_err(|e| format!("Failed to bencode metainfo: {e}"))?;

    let target_file_path = if output_dir_or_file.is_dir()
        || output_dir_or_file
            .extension()
            .map(|e| e != "torrent" && e != "adc")
            .unwrap_or(true)
    {
        std::fs::create_dir_all(output_dir_or_file).map_err(|e| {
            format!(
                "Failed to create output directory '{}': {e}",
                output_dir_or_file.display()
            )
        })?;
        output_dir_or_file.join(format!("{}.torrent", name))
    } else {
        if let Some(parent) = output_dir_or_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        output_dir_or_file.to_path_buf()
    };

    std::fs::write(&target_file_path, &bencode_bytes).map_err(|e| {
        format!(
            "Failed to write .torrent file to '{}': {e}",
            target_file_path.display()
        )
    })?;

    Ok(target_file_path)
}
