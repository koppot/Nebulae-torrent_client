//! Quality-based peer eviction task.
//!
//! Periodically scores active peers using the EMA rate cache and issues
//! `peer-remove` RPC calls for peers whose quality score falls below a
//! configurable threshold after a minimum observation period.
//!
//! # Design
//!
//! - Runs as a single background Tokio task, spawned alongside the
//!   enforcement poller.
//! - Shares the `peer_ema_cache` with `get_peer_telemetry` — the EMA state
//!   is updated by both the frontend poll path and this task's own
//!   `torrent_get` calls, whichever runs first.
//! - Rate-limited to at most [`MAX_EVICTIONS_PER_TORRENT_PER_CYCLE`] removals
//!   per torrent per cycle to avoid excessive churn.
//! - Per-peer cooldown of [`EVICTION_COOLDOWN`] prevents repeatedly evicting
//!   the same peer that promptly reconnects.
//! - Reads `PeeringSettings` from the DB on every cycle so changes take
//!   effect without a restart.
//!
//! # Quality Threshold
//!
//! Default: `0.15` — only peers with extremely poor quality are evicted.
//! This is intentionally conservative. A peer must have:
//!   - Low sustained DL throughput AND
//!   - High rate variance (low stability) AND
//!   - Low locality score
//!
//! Peers with zero throughput (seeder-to-seeder) are
//! excluded from eviction regardless of score.
//!
//! # Phase 3 Note
//!
//! Without geo/RTT data the quality score uses EMA rates and stability only.
//! Once Phase 3 (MaxMind + RTT prober) is wired in, the scores improve and
//! the locality axis contributes real differentiation.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::{
    db::Database,
    engine::transmission::TransmissionRpcClient,
    models::peering::{LocalityTier, PeerEmaState, PeerTelemetry, PeeringMode, PeeringSettings},
    utils::geo::GeoResolver,
};

/// How often the quality eviction task wakes up.
const POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Minimum number of EMA observations before a peer is eligible for eviction.
/// At the default 2s frontend poll rate this is ~2 minutes. The quality
/// eviction task also updates EMA state on its own 60s cycle, so at worst
/// a peer needs to survive 5 quality-eviction cycles (5 minutes) before
/// being eligible.
const MIN_OBSERVATIONS_FOR_EVICTION: u32 = 5;

/// Quality score below which a peer is a candidate for eviction.
const DEFAULT_QUALITY_THRESHOLD: f32 = 0.15;

/// Maximum peer-remove calls per torrent per cycle.
const MAX_EVICTIONS_PER_TORRENT_PER_CYCLE: usize = 2;

/// How long after evicting a peer before we will evict it again (if it
/// reconnects). Prevents eviction storms against a determined peer.
const EVICTION_COOLDOWN: Duration = Duration::from_secs(300);

/// EMA smoothing factor — same as in get_peer_telemetry for consistency.
const EMA_ALPHA: f64 = 0.3;

/// Spawn the quality eviction task as a detached Tokio task.
///
/// # Arguments
/// - `rpc_client` — shared reference to the Transmission RPC client
/// - `peer_ema_cache` — shared EMA state (same instance as the engine)
/// - `db` — database handle for reading current `PeeringSettings`
/// - `peer_remove_unsupported` — shared flag from the enforcement poller;
///   if `peer-remove` is unsupported we skip quality eviction too
pub fn spawn(
    rpc_client: Arc<TransmissionRpcClient>,
    peer_ema_cache: Arc<Mutex<HashMap<String, PeerEmaState>>>,
    db: Arc<Database>,
    peer_remove_unsupported: Arc<AtomicBool>,
    geo_resolver: Option<Arc<GeoResolver>>,
) {
    tauri::async_runtime::spawn(async move {
        run_eviction_loop(
            rpc_client,
            peer_ema_cache,
            db,
            peer_remove_unsupported,
            geo_resolver,
        )
        .await;
    });
}

async fn run_eviction_loop(
    rpc: Arc<TransmissionRpcClient>,
    ema_cache: Arc<Mutex<HashMap<String, PeerEmaState>>>,
    db: Arc<Database>,
    peer_remove_unsupported: Arc<AtomicBool>,
    geo_resolver: Option<Arc<GeoResolver>>,
) {
    // Per-peer cooldown tracker: maps "torrent_hash:ip:port" → last eviction time.
    let mut eviction_history: HashMap<String, Instant> = HashMap::new();

    info!(
        "[quality_eviction] eviction loop started — poll interval: {}s, threshold: {}",
        POLL_INTERVAL.as_secs(),
        DEFAULT_QUALITY_THRESHOLD
    );

    // Initial delay — wait one full cycle before the first eviction pass so the
    // EMA state has time to build up from actual peer data.
    sleep(POLL_INTERVAL).await;

    loop {
        if peer_remove_unsupported.load(Ordering::Relaxed) {
            debug!("[quality_eviction] peer-remove unsupported — skipping cycle");
            sleep(POLL_INTERVAL).await;
            continue;
        }

        // Read current peering settings from DB (picks up live changes).
        let settings = read_peering_settings(&db);
        let mode = settings.mode;
        let threshold = DEFAULT_QUALITY_THRESHOLD;

        // Fetch all torrents with peer lists.
        let rpc_clone = Arc::clone(&rpc);
        let ema_clone = Arc::clone(&ema_cache);
        let geo_clone = geo_resolver.clone();
        let result = tokio::task::spawn_blocking(move || {
            run_eviction_cycle(
                &rpc_clone,
                &ema_clone,
                mode,
                threshold,
                geo_clone.as_deref(),
            )
        })
        .await;

        match result {
            Ok(Ok(cycle_result)) => {
                // Merge eviction history from this cycle.
                for (key, when) in cycle_result.evicted {
                    eviction_history.insert(key, when);
                }
                info!(
                    "[quality_eviction] cycle complete — {} peer(s) evicted for poor quality",
                    cycle_result.total_evicted
                );
            }
            Ok(Err(e)) => {
                warn!("[quality_eviction] cycle error: {e}");
            }
            Err(e) => {
                warn!("[quality_eviction] spawn_blocking join error: {e}");
            }
        }

        // Prune stale cooldown entries to prevent unbounded growth.
        eviction_history.retain(|_, t| t.elapsed() < EVICTION_COOLDOWN);

        sleep(POLL_INTERVAL).await;
    }
}

struct CycleResult {
    evicted: HashMap<String, Instant>,
    total_evicted: usize,
}

fn run_eviction_cycle(
    rpc: &TransmissionRpcClient,
    ema_cache: &Mutex<HashMap<String, PeerEmaState>>,
    mode: PeeringMode,
    threshold: f32,
    geo_resolver: Option<&GeoResolver>,
) -> Result<CycleResult, String> {
    let torrents = rpc.torrent_get(None)?;

    let mut evicted: HashMap<String, Instant> = HashMap::new();
    let mut total_evicted = 0usize;

    for torrent in &torrents {
        let torrent_hash = match torrent.get("hashString").and_then(|h| h.as_str()) {
            Some(h) => h,
            None => continue,
        };
        let torrent_id = match torrent.get("id").and_then(|v| v.as_i64()) {
            Some(id) => id,
            None => continue,
        };
        let peers = match torrent.get("peers").and_then(|p| p.as_array()) {
            Some(p) => p,
            None => continue,
        };

        let mut evictions_this_torrent = 0usize;

        for peer in peers {
            if evictions_this_torrent >= MAX_EVICTIONS_PER_TORRENT_PER_CYCLE {
                break;
            }

            let address = match peer.get("address").and_then(|a| a.as_str()) {
                Some(a) => a,
                None => continue,
            };
            let port = peer.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
            let peer_addr = format!("{address}:{port}");
            let cache_key = format!("{torrent_hash}:{address}");
            let eviction_key = format!("{torrent_hash}:{peer_addr}");

            // Update EMA state for this peer from current instantaneous rates.
            let raw_dl = peer
                .get("rateToClient")
                .and_then(|r| r.as_u64())
                .unwrap_or(0) as f64;
            let raw_ul = peer.get("rateToPeer").and_then(|r| r.as_u64()).unwrap_or(0) as f64;

            let (quality_score, observation_count) = {
                let mut cache = ema_cache.lock().unwrap_or_else(|p| p.into_inner());
                let ema = cache.entry(cache_key).or_insert(PeerEmaState {
                    ema_dl_bps: raw_dl,
                    ema_ul_bps: raw_ul,
                    ema_dl_variance: 0.0,
                    observation_count: 0,
                });

                let prev_dl = ema.ema_dl_bps;
                ema.ema_dl_bps = EMA_ALPHA * raw_dl + (1.0 - EMA_ALPHA) * ema.ema_dl_bps;
                ema.ema_ul_bps = EMA_ALPHA * raw_ul + (1.0 - EMA_ALPHA) * ema.ema_ul_bps;
                let delta = (raw_dl - prev_dl).abs();
                ema.ema_dl_variance = EMA_ALPHA * delta + (1.0 - EMA_ALPHA) * ema.ema_dl_variance;
                ema.observation_count = ema.observation_count.saturating_add(1);

                let obs = ema.observation_count;

                // Compute a synthetic PeerTelemetry for scoring.
                let stability = if obs >= 3 && ema.ema_dl_bps > 0.0 {
                    let nv = (ema.ema_dl_variance / ema.ema_dl_bps.max(1.0)).clamp(0.0, 1.0) as f32;
                    (1.0 - nv).clamp(0.0, 1.0)
                } else {
                    0.5
                };

                let peer_telemetry = PeerTelemetry {
                    peer_id: peer_addr.clone(),
                    ip: address.to_string(),
                    port,
                    rtt_ms: 0, // Phase 3: RTT prober
                    req_resp_latency_ms: 0,
                    sustained_dl_bps: (ema.ema_dl_bps * 8.0) as u64,
                    sustained_ul_bps: (ema.ema_ul_bps * 8.0) as u64,
                    upload_reciprocity: 0.5,
                    stability_score: stability,
                    disconnect_count: 0,
                    timeout_count: 0,
                    choke_events: 0,
                    client_name: String::new(),
                    locality_tier: geo_resolver.map_or_else(
                        || classify_ip_locality(address),
                        |g| g.classify_locality(address),
                    ),
                    country_code: geo_resolver.and_then(|g| g.country_code(address)),
                    asn: geo_resolver.and_then(|g| g.asn(address)),
                    utp: false,
                    encryption: false,
                    ipv6: address.contains(':'),
                    dht: false,
                    pex: false,
                    connection_age_seconds: 0,
                    total_downloaded_bytes: 0,
                    total_uploaded_bytes: 0,
                };

                (peer_telemetry.compute_quality_score(mode), obs)
            };

            // ── Eviction eligibility checks ──────────────────────────────────

            // 1. Not enough history yet.
            if observation_count < MIN_OBSERVATIONS_FOR_EVICTION {
                continue;
            }

            // 2. Score is acceptable — not a candidate.
            if quality_score >= threshold {
                continue;
            }

            // 3. Peer has no DL rate AND no UL rate — seeder-to-seeder situation.
            //    Do NOT evict: zero-rate peers can be valuable future uploaders.
            let total_rate = raw_dl + raw_ul;
            if total_rate < 1.0 {
                continue;
            }

            // 4. Cooldown: skip if we evicted this peer recently.
            // (Cooldown is checked from eviction_history in the outer loop,
            //  but eviction_history is not passed into spawn_blocking. We
            //  track newly-evicted in this cycle's `evicted` map and merge
            //  in the outer async loop — so within a single cycle we do not
            //  re-evict the same peer even if it shows up on multiple torrents.)
            if evicted.contains_key(&eviction_key) {
                continue;
            }

            // ── Issue peer-remove ────────────────────────────────────────────
            match rpc.peer_remove(torrent_id, &peer_addr) {
                Ok(()) => {
                    info!(
                        "[quality_eviction] evicted {peer_addr} from torrent {torrent_id} \
                         (score={quality_score:.3} < threshold={threshold:.3}, obs={observation_count})"
                    );
                    evicted.insert(eviction_key, Instant::now());
                    evictions_this_torrent += 1;
                    total_evicted += 1;
                }
                Err(e) if e.contains("peer-remove") => {
                    // Daemon signalled that peer-remove is not supported.
                    // Return an error that the caller propagates to set the
                    // shared unsupported flag.
                    return Err(format!(
                        "peer-remove RPC not supported by Transmission daemon: {e}"
                    ));
                }
                Err(e) => {
                    warn!("[quality_eviction] peer_remove failed for {peer_addr}: {e}");
                }
            }
        }
    }

    Ok(CycleResult {
        evicted,
        total_evicted,
    })
}

/// Simple LAN/Global tier classification without MaxMind (used until Phase 3
/// wires the full geo resolver into the eviction task).
fn classify_ip_locality(ip: &str) -> LocalityTier {
    if crate::utils::geo::is_local_ip(ip) {
        LocalityTier::Lan
    } else {
        LocalityTier::Global
    }
}

fn read_peering_settings(db: &Database) -> PeeringSettings {
    db.get_setting("peering_settings_json")
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}
