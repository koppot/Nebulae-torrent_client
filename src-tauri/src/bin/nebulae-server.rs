//! Nebulae Server — headless bridge that exposes a remote rtorrent daemon
//! (Unraid, TrueNAS, any Docker host) over a small token-authenticated HTTP
//! API. The original Nebulae desktop app is the interface; there is no web UI
//! here by design.
//!
//! Example:
//!   nebulae-server --engine rtorrent --scgi /run/rtorrent/scgi.sock \
//!                  --listen 0.0.0.0:9174 --token $NEBULAE_SERVER_TOKEN

use axum::{
    extract::{Path, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use nebulae_torrent_client_lib::engine::rtorrent::scgi::ScgiTarget;
use nebulae_torrent_client_lib::engine::{MockTorrentEngine, RtorrentRemoteEngine, TorrentEngine};
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

const DEFAULT_LISTEN: &str = "0.0.0.0:9174";

struct AppState {
    engine: Arc<dyn TorrentEngine>,
    engine_kind: &'static str,
    token: String,
}

// ---------- auth ----------

async fn auth(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let authorized = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .map(|value| value == format!("Bearer {}", state.token))
        .unwrap_or(false);
    if !authorized {
        return (StatusCode::UNAUTHORIZED, "invalid or missing bearer token").into_response();
    }
    next.run(request).await
}

// ---------- response helpers ----------

fn engine_error(message: String) -> Response {
    (StatusCode::BAD_GATEWAY, message).into_response()
}

async fn run_engine<T, F>(state: &Arc<AppState>, call: F) -> Response
where
    T: serde::Serialize + Send + 'static,
    F: FnOnce(&dyn TorrentEngine) -> Result<T, String> + Send + 'static,
{
    let engine = Arc::clone(&state.engine);
    match tokio::task::spawn_blocking(move || call(engine.as_ref())).await {
        Ok(Ok(payload)) => Json(payload).into_response(),
        Ok(Err(message)) => engine_error(message),
        Err(join) => engine_error(format!("internal error: {join}")),
    }
}

// ---------- handlers ----------

async fn status(State(state): State<Arc<AppState>>) -> Response {
    let engine_kind = state.engine_kind;
    run_engine(&state, move |engine| {
        let status = engine.engine_status();
        Ok(json!({
            "name": "nebulae-server",
            "version": env!("CARGO_PKG_VERSION"),
            "engine": engine_kind,
            "active_torrents": status.active_torrents,
            "paused_torrents": status.paused_torrents,
            "kill_switch_engaged": status.kill_switch_engaged,
        }))
    })
    .await
}

async fn list_torrents(State(state): State<Arc<AppState>>) -> Response {
    run_engine(&state, |engine| engine.list_torrents()).await
}

#[derive(Deserialize)]
struct AddBody {
    /// Magnet URI, http(s) URL, or a .torrent path on the server.
    source: Option<String>,
    /// Raw .torrent bencode, base64-encoded (uploaded from the desktop app).
    source_b64: Option<String>,
    download_dir: Option<String>,
}

async fn add_torrent(State(state): State<Arc<AppState>>, body: Option<Json<AddBody>>) -> Response {
    let Some(Json(body)) = body else {
        return (StatusCode::BAD_REQUEST, "JSON body required").into_response();
    };
    run_engine(&state, move |engine| {
        if let Some(source_b64) = body.source_b64 {
            use base64::engine::general_purpose::STANDARD;
            use base64::Engine;
            let bytes = STANDARD
                .decode(source_b64.trim())
                .map_err(|e| format!("invalid base64 torrent payload: {e}"))?;
            return engine.add_torrent_from_bytes(&bytes, body.download_dir.as_deref());
        }
        let source = body
            .source
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| "body must contain 'source' or 'source_b64'".to_string())?;
        engine.add_torrent(source.trim(), body.download_dir.as_deref())
    })
    .await
}

async fn pause_torrent(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    run_engine(&state, move |engine| engine.pause_torrent(&id)).await
}

async fn resume_torrent(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    run_engine(&state, move |engine| engine.resume_torrent(&id)).await
}

async fn force_recheck(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    run_engine(&state, move |engine| engine.force_recheck(&id)).await
}

async fn pause_all(State(state): State<Arc<AppState>>) -> Response {
    run_engine(&state, |engine| engine.pause_all()).await
}

async fn resume_all(State(state): State<Arc<AppState>>) -> Response {
    run_engine(&state, |engine| engine.resume_all()).await
}

#[derive(Deserialize)]
struct LimitsBody {
    dl_kib: u32,
    ul_kib: u32,
    #[serde(default)]
    ratio_limit: f32,
}

async fn set_limits(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: Option<Json<LimitsBody>>,
) -> Response {
    let Some(Json(body)) = body else {
        return (StatusCode::BAD_REQUEST, "JSON body required").into_response();
    };
    run_engine(&state, move |engine| {
        engine.set_torrent_limits(&id, body.dl_kib, body.ul_kib, body.ratio_limit)
    })
    .await
}

#[derive(Deserialize)]
struct LocationBody {
    path: String,
}

async fn set_location(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: Option<Json<LocationBody>>,
) -> Response {
    let Some(Json(body)) = body else {
        return (StatusCode::BAD_REQUEST, "JSON body required").into_response();
    };
    run_engine(&state, move |engine| engine.set_location(&id, &body.path)).await
}

#[derive(Deserialize)]
struct RemoveQuery {
    #[serde(default)]
    delete_files: String,
}

async fn remove_torrent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<RemoveQuery>,
) -> Response {
    let delete_files = query.delete_files == "true" || query.delete_files == "1";
    run_engine(&state, move |engine| {
        engine.remove_torrent(&id, delete_files)
    })
    .await
}

#[derive(Deserialize)]
struct KillSwitchBody {
    enable: bool,
}

async fn kill_switch(
    State(state): State<Arc<AppState>>,
    body: Option<Json<KillSwitchBody>>,
) -> Response {
    let Some(Json(body)) = body else {
        return (StatusCode::BAD_REQUEST, "JSON body required").into_response();
    };
    run_engine(&state, move |engine| {
        if body.enable {
            engine.trigger_kill_switch()
        } else {
            engine.reset_kill_switch()
        }
        .map(|_| json!({ "active": engine.is_kill_switch_active() }))
    })
    .await
}

async fn kill_switch_status(State(state): State<Arc<AppState>>) -> Response {
    run_engine(&state, |engine| {
        Ok(json!({ "active": engine.is_kill_switch_active() }))
    })
    .await
}

// ---------- CLI ----------

enum ScgiDest {
    Unix(PathBuf),
    Tcp(String, u16),
}

struct Args {
    engine: String,
    scgi: Option<ScgiDest>,
    listen: String,
    token: Option<String>,
}

fn usage() -> String {
    format!(
        "nebulae-server {version}\n\
         Headless Nebulae engine bridge for rtorrent on server hosts.\n\n\
         OPTIONS:\n\
         \x20 --engine <rtorrent|mock>   Engine backend (default: rtorrent; 'mock' is for development only)\n\
         \x20 --scgi <PATH>              rtorrent SCGI unix domain socket\n\
         \x20 --scgi-tcp <HOST:PORT>     rtorrent SCGI TCP endpoint (alternative to --scgi)\n\
         \x20 --listen <ADDR:PORT>       API listen address (default: {DEFAULT_LISTEN})\n\
         \x20 --token <TOKEN>            API bearer token (default: $NEBULAE_SERVER_TOKEN, else generated and printed)\n\
         \x20 -h, --help                 Show this help\n",
        version = env!("CARGO_PKG_VERSION"),
    )
}

fn takes_value(name: &str, argv: &mut impl Iterator<Item = String>) -> Result<String, String> {
    argv.next()
        .ok_or_else(|| format!("{name} requires a value"))
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        engine: "rtorrent".to_string(),
        scgi: None,
        listen: DEFAULT_LISTEN.to_string(),
        token: None,
    };
    let mut argv = std::env::args().skip(1);
    while let Some(flag) = argv.next() {
        match flag.as_str() {
            "--engine" => args.engine = takes_value("--engine", &mut argv)?,
            "--scgi" => {
                let path = takes_value("--scgi", &mut argv)?;
                args.scgi = Some(ScgiDest::Unix(PathBuf::from(path)));
            }
            "--scgi-tcp" => {
                let endpoint = takes_value("--scgi-tcp", &mut argv)?;
                let (host, port) = endpoint
                    .rsplit_once(':')
                    .ok_or_else(|| format!("--scgi-tcp expects HOST:PORT, got '{endpoint}'"))?;
                let port: u16 = port
                    .parse()
                    .map_err(|_| format!("--scgi-tcp: bad port '{port}'"))?;
                args.scgi = Some(ScgiDest::Tcp(host.to_string(), port));
            }
            "--listen" => args.listen = takes_value("--listen", &mut argv)?,
            "--token" => args.token = Some(takes_value("--token", &mut argv)?),
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument '{other}'\n\n{}", usage())),
        }
    }
    Ok(args)
}

#[tokio::main]
async fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("error: {message}");
            std::process::exit(2);
        }
    };

    let engine: Arc<dyn TorrentEngine> = match args.engine.as_str() {
        "mock" => {
            eprintln!(
                "[nebulae-server] engine: mock (fabricated data — development only, never for private trackers)"
            );
            Arc::new(MockTorrentEngine::new())
        }
        "rtorrent" => match &args.scgi {
            Some(ScgiDest::Unix(path)) => {
                eprintln!("[nebulae-server] engine: rtorrent via unix socket {path:?}");
                Arc::new(RtorrentRemoteEngine::new(ScgiTarget::UnixSocket(
                    path.clone(),
                )))
            }
            Some(ScgiDest::Tcp(host, port)) => {
                eprintln!("[nebulae-server] engine: rtorrent via tcp {host}:{port}");
                Arc::new(RtorrentRemoteEngine::new(ScgiTarget::Tcp(
                    host.clone(),
                    *port,
                )))
            }
            None => {
                eprintln!(
                    "error: --scgi <PATH> or --scgi-tcp <HOST:PORT> is required with --engine rtorrent\n\n{}",
                    usage()
                );
                std::process::exit(2);
            }
        },
        other => {
            eprintln!("error: unknown engine '{other}' (expected rtorrent or mock)");
            std::process::exit(2);
        }
    };

    let token = match args
        .token
        .clone()
        .or_else(|| std::env::var("NEBULAE_SERVER_TOKEN").ok())
    {
        Some(token) if !token.trim().is_empty() => token.trim().to_string(),
        _ => {
            let generated = uuid::Uuid::new_v4().to_string();
            eprintln!(
                "[nebulae-server] no --token or $NEBULAE_SERVER_TOKEN given; generated one-time token (not persisted): {generated}"
            );
            generated
        }
    };

    let engine_kind = if args.engine == "mock" {
        "mock"
    } else {
        "rtorrent"
    };
    let state = Arc::new(AppState {
        engine,
        engine_kind,
        token,
    });

    let app = Router::new()
        .route("/api/v1/status", get(status))
        .route("/api/v1/torrents", get(list_torrents).post(add_torrent))
        .route("/api/v1/torrents/pause-all", post(pause_all))
        .route("/api/v1/torrents/resume-all", post(resume_all))
        .route("/api/v1/torrents/{id}/pause", post(pause_torrent))
        .route("/api/v1/torrents/{id}/resume", post(resume_torrent))
        .route("/api/v1/torrents/{id}/recheck", post(force_recheck))
        .route("/api/v1/torrents/{id}/limits", post(set_limits))
        .route("/api/v1/torrents/{id}/location", post(set_location))
        .route("/api/v1/torrents/{id}", delete(remove_torrent))
        .route(
            "/api/v1/kill-switch",
            post(kill_switch).get(kill_switch_status),
        )
        .layer(middleware::from_fn_with_state(Arc::clone(&state), auth))
        .with_state(state);

    let addr: SocketAddr = args
        .listen
        .parse()
        .map_err(|e| format!("invalid --listen address: {e}"))
        .unwrap_or_else(|message: String| {
            eprintln!("error: {message}");
            std::process::exit(2);
        });

    match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => {
            eprintln!(
                "[nebulae-server] v{} listening on {} (engine: {})",
                env!("CARGO_PKG_VERSION"),
                addr,
                engine_kind
            );
            eprintln!("[nebulae-server] note: this beta serves plain HTTP on the LAN; use a reverse proxy or keep it inside your trusted network");
            if let Err(error) = axum::serve(listener, app).await {
                eprintln!("[nebulae-server] fatal: {error}");
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("[nebulae-server] cannot bind {addr}: {error}");
            std::process::exit(1);
        }
    }
}
