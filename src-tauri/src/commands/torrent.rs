use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TorrentItem {
    pub id: String,
    pub name: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub uploaded_bytes: u64,
    pub download_rate: u64,
    pub upload_rate: u64,
    pub progress: f32,
    pub status: String,
    pub eta_seconds: Option<u64>,
    pub seeds: u32,
    pub peers: u32,
    pub ratio: f32,
    pub category: String,
    pub is_media: bool,
    pub poster_url: Option<String>,
}

#[tauri::command]
pub fn list_torrents() -> Result<Vec<TorrentItem>, String> {
    // Scaffold initial representative items for UI preview & mode verification
    Ok(vec![
        TorrentItem {
            id: "sample-1".to_string(),
            name: "Tokyo Story (1953) [4K Restoration]".to_string(),
            total_bytes: 34_500_000_000,
            downloaded_bytes: 34_500_000_000,
            uploaded_bytes: 52_000_000_000,
            download_rate: 0,
            upload_rate: 1_250_000,
            progress: 1.0,
            status: "Seeding".to_string(),
            eta_seconds: None,
            seeds: 42,
            peers: 8,
            ratio: 1.51,
            category: "Movies".to_string(),
            is_media: true,
            poster_url: None,
        },
        TorrentItem {
            id: "sample-2".to_string(),
            name: "Seven Samurai (1954) [Criterion Remaster]".to_string(),
            total_bytes: 48_200_000_000,
            downloaded_bytes: 26_510_000_000,
            uploaded_bytes: 4_300_000_000,
            download_rate: 14_800_000,
            upload_rate: 850_000,
            progress: 0.55,
            status: "Downloading".to_string(),
            eta_seconds: Some(1460),
            seeds: 88,
            peers: 24,
            ratio: 0.16,
            category: "Movies".to_string(),
            is_media: true,
            poster_url: None,
        },
        TorrentItem {
            id: "sample-3".to_string(),
            name: "Arch Linux 2026.10 x86_64 Installation ISO".to_string(),
            total_bytes: 950_000_000,
            downloaded_bytes: 950_000_000,
            uploaded_bytes: 2_100_000_000,
            download_rate: 0,
            upload_rate: 320_000,
            progress: 1.0,
            status: "Completed".to_string(),
            eta_seconds: None,
            seeds: 154,
            peers: 12,
            ratio: 2.21,
            category: "Software".to_string(),
            is_media: false,
            poster_url: None,
        },
    ])
}

#[tauri::command]
pub fn add_torrent_stub(source: String, _download_dir: Option<String>) -> Result<String, String> {
    Ok(format!("Intake queued for source (stub): {}", if source.starts_with("magnet:") { "magnet link" } else { "file" }))
}

#[tauri::command]
pub fn pause_torrent_stub(id: String) -> Result<(), String> {
    println!("Stub pause: {id}");
    Ok(())
}

#[tauri::command]
pub fn resume_torrent_stub(id: String) -> Result<(), String> {
    println!("Stub resume: {id}");
    Ok(())
}
