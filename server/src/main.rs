//! nebulae-server: headless Transmission-backed REST API bridge.
//!
//! Wraps a local Transmission RPC daemon behind a bearer-token-authenticated
//! REST API matching the contract expected by the Nebulae desktop client's
//! `RemoteServerEngine` and `commands/remote_server.rs`.
//!
//! Designed to run standalone or inside the Docker container alongside
//! Transmission.

mod transmission;

use actix_web::{web, App, HttpServer, HttpRequest, HttpResponse};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

// ── CLI ───────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "nebulae-server", about = "Headless Nebulae server bridge")]
struct Cli {
    /// Port to listen on.
    #[arg(long, default_value_t = 9174, env = "NEBULAE_PORT")]
    port: u16,

    /// Bearer token for authentication.
    #[arg(long, env = "NEBULAE_TOKEN")]
    token: String,

    /// Transmission RPC URL (usually http://127.0.0.1:9091/transmission/rpc).
    #[arg(long, default_value = "http://127.0.0.1:9091/transmission/rpc", env = "TRANSMISSION_RPC_URL")]
    transmission_url: String,

    /// Transmission RPC username (optional).
    #[arg(long, default_value = "", env = "TRANSMISSION_USER")]
    transmission_user: String,

    /// Transmission RPC password (optional).
    #[arg(long, default_value = "", env = "TRANSMISSION_PASS")]
    transmission_pass: String,

    /// Listen address.
    #[arg(long, default_value = "0.0.0.0", env = "NEBULAE_BIND")]
    bind: String,
}

// ── App state ─────────────────────────────────────────────────────────────

pub struct AppState {
    token: String,
    transmission: transmission::TransmissionClient,
    kill_switch: AtomicBool,
}

// ── Auth middleware ───────────────────────────────────────────────────────

fn check_auth(req: &HttpRequest, state: &web::Data<Arc<AppState>>) -> Result<(), HttpResponse> {
    let header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if header.strip_prefix("Bearer ").unwrap_or("") == state.token {
        Ok(())
    } else {
        Err(HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "Unauthorized"
        })))
    }
}

// ── Response types (match client contract) ────────────────────────────────

#[derive(Serialize)]
struct StatusResponse {
    name: String,
    version: String,
    engine: String,
    kill_switch_engaged: bool,
}

#[derive(Serialize)]
struct TorrentFileItem {
    index: u32,
    name: String,
    size_bytes: u64,
    progress: f32,
    priority: u8,
}

#[derive(Serialize)]
struct TorrentTrackerItem {
    tier: u32,
    url: String,
    status: String,
    seeds: u32,
    peers: u32,
    downloaded: u64,
    message: String,
}

#[derive(Serialize)]
struct TorrentItem {
    id: String,
    name: String,
    total_bytes: u64,
    downloaded_bytes: u64,
    uploaded_bytes: u64,
    download_rate: u64,
    upload_rate: u64,
    progress: f32,
    status: String,
    eta_seconds: Option<u64>,
    seeds: u32,
    peers: u32,
    ratio: f32,
    category: String,
    is_media: bool,
    poster_url: Option<String>,
    info_hash: String,
    save_path: String,
    added_on: u64,
    completed_on: Option<u64>,
    comment: String,
    created_by: String,
    piece_size: u32,
    total_pieces: u32,
    sequential_download: bool,
    first_last_piece_prio: bool,
    super_seeding: bool,
    tags: Vec<String>,
    files: Vec<TorrentFileItem>,
    trackers: Vec<TorrentTrackerItem>,
    download_limit_kib: u32,
    upload_limit_kib: u32,
}

#[derive(Serialize)]
struct KillSwitchResponse {
    engaged: bool,
}

#[derive(Deserialize)]
struct KillSwitchRequest {
    engaged: bool,
}

#[derive(Deserialize)]
struct LimitsRequest {
    dl_kib: u32,
    ul_kib: u32,
    ratio_limit: f32,
}

#[derive(Deserialize)]
struct AddTorrentRequest {
    source: String,
    download_dir: Option<String>,
}

// ── Handlers ──────────────────────────────────────────────────────────────

async fn get_status(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    if let Err(e) = check_auth(&req, &state) { return e; }

    HttpResponse::Ok().json(StatusResponse {
        name: "nebulae-server".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        engine: "transmission".to_string(),
        kill_switch_engaged: state.kill_switch.load(Ordering::Relaxed),
    })
}

async fn list_torrents(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    if let Err(e) = check_auth(&req, &state) { return e; }

    match state.transmission.list_torrents().await {
        Ok(torrents) => {
            let items: Vec<TorrentItem> = torrents.into_iter().map(|t| t.into()).collect();
            HttpResponse::Ok().json(items)
        }
        Err(e) => {
            warn!("list_torrents failed: {e}");
            HttpResponse::InternalServerError().json(serde_json::json!({ "error": e }))
        }
    }
}

async fn add_torrent(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    body: web::Json<AddTorrentRequest>,
) -> HttpResponse {
    if let Err(e) = check_auth(&req, &state) { return e; }

    match state.transmission.add_torrent(&body.source, body.download_dir.as_deref()).await {
        Ok(hash) => HttpResponse::Ok().json(serde_json::json!({ "info_hash": hash })),
        Err(e) => {
            warn!("add_torrent failed: {e}");
            HttpResponse::InternalServerError().json(serde_json::json!({ "error": e }))
        }
    }
}

async fn pause_torrent(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    if let Err(e) = check_auth(&req, &state) { return e; }
    let id = path.into_inner();

    match state.transmission.pause_torrent(&id).await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({ "ok": true })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({ "error": e })),
    }
}

async fn resume_torrent(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    if let Err(e) = check_auth(&req, &state) { return e; }
    let id = path.into_inner();

    match state.transmission.resume_torrent(&id).await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({ "ok": true })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({ "error": e })),
    }
}

async fn pause_all(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    if let Err(e) = check_auth(&req, &state) { return e; }

    match state.transmission.pause_all().await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({ "ok": true })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({ "error": e })),
    }
}

async fn resume_all(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    if let Err(e) = check_auth(&req, &state) { return e; }

    match state.transmission.resume_all().await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({ "ok": true })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({ "error": e })),
    }
}

async fn remove_torrent(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> HttpResponse {
    if let Err(e) = check_auth(&req, &state) { return e; }
    let id = path.into_inner();
    let delete_files = query.get("delete_files").map_or(false, |v| v == "true");

    match state.transmission.remove_torrent(&id, delete_files).await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({ "ok": true })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({ "error": e })),
    }
}

async fn kill_switch(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    body: web::Json<KillSwitchRequest>,
) -> HttpResponse {
    if let Err(e) = check_auth(&req, &state) { return e; }

    if body.engaged {
        state.kill_switch.store(true, Ordering::SeqCst);
        // Pause all torrents when kill switch engages.
        let _ = state.transmission.pause_all().await;
    } else {
        state.kill_switch.store(false, Ordering::SeqCst);
    }

    HttpResponse::Ok().json(KillSwitchResponse {
        engaged: state.kill_switch.load(Ordering::Relaxed),
    })
}

async fn reannounce_torrent(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    if let Err(e) = check_auth(&req, &state) { return e; }
    let id = path.into_inner();

    match state.transmission.reannounce(&id).await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({ "ok": true })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({ "error": e })),
    }
}

async fn recheck_torrent(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    if let Err(e) = check_auth(&req, &state) { return e; }
    let id = path.into_inner();

    match state.transmission.recheck(&id).await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({ "ok": true })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({ "error": e })),
    }
}

async fn set_torrent_limits(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
    body: web::Json<LimitsRequest>,
) -> HttpResponse {
    if let Err(e) = check_auth(&req, &state) { return e; }
    let id = path.into_inner();

    match state.transmission.set_limits(&id, body.dl_kib, body.ul_kib, body.ratio_limit).await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({ "ok": true })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({ "error": e })),
    }
}

// ── Main ──────────────────────────────────────────────────────────────────

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let transmission = transmission::TransmissionClient::new(
        &cli.transmission_url,
        if cli.transmission_user.is_empty() { None } else { Some(&cli.transmission_user) },
        if cli.transmission_pass.is_empty() { None } else { Some(&cli.transmission_pass) },
    );

    let state = Arc::new(AppState {
        token: cli.token.clone(),
        transmission,
        kill_switch: AtomicBool::new(false),
    });

    info!(
        "nebulae-server v{} listening on {}:{} (Transmission at {})",
        env!("CARGO_PKG_VERSION"),
        cli.bind,
        cli.port,
        cli.transmission_url,
    );

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/api/v1/status", web::get().to(get_status))
            .route("/api/v1/torrents", web::get().to(list_torrents))
            .route("/api/v1/torrents", web::post().to(add_torrent))
            .route("/api/v1/torrents/{id}/pause", web::post().to(pause_torrent))
            .route("/api/v1/torrents/{id}/resume", web::post().to(resume_torrent))
            .route("/api/v1/torrents/{id}", web::delete().to(remove_torrent))
            .route("/api/v1/torrents/{id}/reannounce", web::post().to(reannounce_torrent))
            .route("/api/v1/torrents/{id}/recheck", web::post().to(recheck_torrent))
            .route("/api/v1/torrents/{id}/limits", web::post().to(set_torrent_limits))
            .route("/api/v1/torrents/pause-all", web::post().to(pause_all))
            .route("/api/v1/torrents/resume-all", web::post().to(resume_all))
            .route("/api/v1/kill-switch", web::post().to(kill_switch))
    })
    .bind((cli.bind.as_str(), cli.port))?
    .run()
    .await
}
