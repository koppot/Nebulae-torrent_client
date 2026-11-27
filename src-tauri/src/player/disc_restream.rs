//! Disc restreaming — the smallest working "Play on Device" path for
//! authored discs (task order 2026-11-27, Part 6).
//!
//! Receivers cannot decode authored disc structures, and the AirPlay path
//! (native AVPlayer) cannot open them at all. The working chain, MEASURED on
//! the Glass Fleet D1 ISO (the disc that misreports length: broken VOB PTS),
//! is VLC's own stream output: dvdread access of the main feature title,
//! transcode to H.264 (x264) + AAC, MPEG-TS over HTTP:
//!
//! ```text
//! dvdread:///<disc>#<feature title>
//!   :sout=#transcode{vcodec=h264,vb=2500,venc=x264{...},acodec=mp4a,ab=192,
//!          channels=2,samplerate=44100}
//!          :std{access=http,mux=ts,dst=0.0.0.0:<port>/nebulae-cast}
//! ```
//!
//! Measured 2026-11-27 (scripts/tests/disc-cast-dryrun.sh): a receiver
//! fetched 4,568,964 bytes of valid MPEG-TS carrying h264 720x480 + 3 aac
//! audio streams from this exact ISO.
//!
//! KNOWN LIMITS (accurate, reported to the UI):
//! - Raw MPEG-TS ("ts" mode) is not playable by AirPlay/AVPlayer or Chromecast
//!   receivers (both need HLS). The default "hls" mode writes keyframe-aligned
//!   segments and serves a playlist built from the PTS in the segments
//!   (player/hls_cast.rs; measured on the Glass Fleet D1 ISO, Banner of the
//!   Stars III and a plain file with a clean exit).
//! - "renderer" mode hands the media to libvlc's own Chromecast/DLNA output.
//! - Menus are not available on the receiver: the main feature title is
//!   streamed directly.
//! - File casting is untouched (AirPlay receiver-native path unchanged).
//! - UHD (4K Blu-ray) casting is NOT measured and NOT claimed: every
//!   measurement above is a DVD (720x480) or a plain file. Nothing here says
//!   UHD or HDR discs work through it.
//! - AirPlay to an Apple TV is macOS-only (native AVPlayer). Windows has no
//!   AirPlay sender (see the Task 5 research); only the libvlc renderer path
//!   (Chromecast/DLNA) and the HLS URL exist there.
//!
//! The restream runs on its OWN media player created from the shared
//! libvlc instance; the local playback session is neither stopped nor
//! modified, and nothing about local playback changes on any platform.
use super::engine;
use super::hls_cast;
use crate::player::PlayerState;

use std::ffi::CString;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// Locks a possibly-poisoned mutex, preferring the data over the guard
/// (same policy as the player commands; poll/teardown data is advisory).
fn poison<T>(g: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    g.lock().unwrap_or_else(|p| p.into_inner())
}

/// Default TCP port for the local HTTP restream.
pub const DEFAULT_PORT: u16 = 18123;

/// Per-session cap on the HLS scratch directory. The chain writes about
/// 2.5 Mbps video + 192 kbps audio = ~1.2 GB per hour (calculated 1.1-1.3;
/// measured on the Glass Fleet D1 ISO: 556 s, ~70 MB, largest segment
/// 26 MB), so 6 GiB holds ~4.5 h of feature, longer than any disc title.
/// Past it the encoder is stopped and the playlist says so.
pub const SCRATCH_LIMIT_BYTES: u64 = 6 << 30;

/// Scratch directories untouched this long are treated as orphans of a
/// crashed session and swept on the next start.
const ORPHAN_AGE: Duration = Duration::from_secs(12 * 3600);

/// The restream player pointer, shared with the LAN server's callbacks.
/// `with` holds the lock while it calls, and `close` takes the same lock,
/// so once `close` returns no callback can be inside libvlc or call it
/// later: a connection thread that outlives the server's join cannot touch a
/// released player.
pub struct PlayerGate(Mutex<Option<usize>>);

impl PlayerGate {
    fn new(mp: usize) -> Arc<Self> {
        Arc::new(Self(Mutex::new(Some(mp))))
    }

    /// `None` once closed.
    pub fn with<T>(&self, f: impl FnOnce(usize) -> T) -> Option<T> {
        let g = poison(&self.0);
        (*g).map(f)
    }

    pub fn close(&self) {
        *poison(&self.0) = None;
    }
}

/// What the frontend needs to hand the stream to a receiver or the user.
#[derive(Clone, serde::Serialize)]
pub struct DiscRestreamInfo {
    /// Full URL a receiver/VLC player can open.
    pub url: String,
    pub port: u16,
    /// Snapshot index of the main feature title (informational).
    pub title_index: i32,
    /// Main feature duration from the title snapshot (ms; 0 if unknown).
    pub duration_ms: i64,
    /// "hls" (default; what AirPlay/Chromecast receivers open), "ts" (raw
    /// MPEG-TS over HTTP) or "renderer" (libvlc casts straight to the
    /// receiver; `url` is empty).
    pub kind: String,
    /// Identity of this restream (its player pointer); lets a waiter notice
    /// that the session it started was stopped or replaced. Not sent to the UI.
    #[serde(skip)]
    pub session_id: usize,
}

/// Where the restream goes.
pub enum RestreamMode {
    Hls,
    RawTs,
    /// libvlc renderer item (Chromecast/DLNA) held by the discovery manager.
    Renderer {
        item: usize,
        start_time_s: f64,
    },
}

/// Live restream handles: a SECOND media player + media on the shared
/// libvlc instance, created for the stream and released on stop.
pub struct DiscRestream {
    pub media_player: usize,
    pub media: usize,
    pub port: u16,
    pub title_index: i32,
    /// HLS mode only: the LAN server and the segment scratch directory.
    pub lan: Option<hls_cast::LanServer>,
    /// HLS mode only: guards the player pointer the LAN callbacks read.
    pub gate: Option<Arc<PlayerGate>>,
    pub scratch: Option<std::path::PathBuf>,
    pub renderer_set: bool,
    /// Renderer mode: the held `libvlc_renderer_item_t` reference this
    /// restream owns; released after the player (see `teardown`).
    pub renderer_item: usize,
    /// What the local player was doing before the cast paused it; restored
    /// exactly when the cast stops.
    pub local_prior: PriorLocal,
}

/// Local playback state captured before a cast takes over.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PriorLocal {
    Playing,
    Paused,
    /// Stopped, ended, buffering, no player...: leave it alone.
    #[default]
    Other,
}

impl PriorLocal {
    /// From `libvlc_media_player_get_state` (3 Playing, 4 Paused).
    pub fn from_state(state: i32) -> Self {
        match state {
            3 => Self::Playing,
            4 => Self::Paused,
            _ => Self::Other,
        }
    }

    /// Whether the cast must pause local playback when it starts.
    pub fn pause_on_cast(self) -> bool {
        self == Self::Playing
    }

    /// Whether stopping the cast must resume local playback. A user who had
    /// paused before casting stays paused.
    pub fn resume_on_stop(self) -> bool {
        self == Self::Playing
    }
}

/// Take a reference on a renderer item so it outlives the discovery list
/// entry (the scan can drop it at any time). Release with `release_item`.
pub fn hold_item(lib: &engine::VlcLib, item: usize) -> Option<usize> {
    let hold = lib.libvlc_renderer_item_hold?;
    let held = unsafe { hold(item as *mut std::os::raw::c_void) };
    (!held.is_null()).then_some(held as usize)
}

pub fn release_item(lib: &engine::VlcLib, item: usize) {
    if item == 0 {
        return;
    }
    if let Some(release) = lib.libvlc_renderer_item_release {
        unsafe { release(item as *mut std::os::raw::c_void) };
    }
}

/// Remember the local state the cast found and return it.
pub fn set_local_prior(state: &PlayerState, prior: PriorLocal) {
    if let Some(r) = poison(&state.disc_restream).as_mut() {
        r.local_prior = prior;
    }
}

/// What to restore on stop, read BEFORE the restream is torn down. `Other`
/// when no restream is active (nothing of ours to undo).
pub fn local_prior(state: &PlayerState) -> PriorLocal {
    poison(&state.disc_restream)
        .as_ref()
        .map(|r| r.local_prior)
        .unwrap_or_default()
}

/// The proven transcode + HTTP chain for `port`. Pure; unit-tested.
pub fn build_sout_chain(port: u16) -> String {
    format!(
        ":sout=#transcode{{vcodec=h264,vb=2500,venc=x264{{profile=high,level=4.0,preset=veryfast}},\
acodec=mp4a,ab=192,channels=2,samplerate=44100}}:std{{access=http,mux=ts,dst=0.0.0.0:{port}/nebulae-cast}}"
    )
}

/// Build the disc MRL for the restream. DVD (folder VIDEO_TS or an ISO)
/// uses dvdread with the 1-based feature title (measured chain); Blu-ray
/// uses bluray:// (the demuxer autoplays the longest playlist). Pure;
/// unit-tested.
pub fn build_restream_mrl(
    resolved_path: &str,
    is_bluray: bool,
    dvd_title_number: Option<usize>,
) -> String {
    if is_bluray {
        return engine::format_disc_mrl("bluray", resolved_path);
    }
    let mrl = engine::format_disc_mrl("dvdread", resolved_path);
    match dvd_title_number {
        Some(n) if n > 0 => format!("{mrl}#{n}"),
        _ => mrl,
    }
}

/// Best-effort primary LAN IPv4 for the receiver URL: connect a UDP socket
/// to a public address (no packet is sent; connect() only picks a route)
/// and read the local endpoint.
fn primary_lan_ipv4() -> Option<std::net::IpAddr> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    let local = sock.local_addr().ok()?;
    (local.is_ipv4() && !local.ip().is_loopback() && !local.ip().is_unspecified())
        .then(|| local.ip())
}

/// Start streaming the current disc's main feature. Requires an ACTIVE
/// disc session (the shared instance and the title snapshot come from it);
/// the local playback keeps running untouched.
///
/// In `Renderer` mode the caller passes a HELD renderer item (see
/// `hold_item`) and this function takes ownership of that reference: it is
/// released here on every failure and by `stop` after success.
pub fn start(
    state: &PlayerState,
    resolved_path: &str,
    port: u16,
    mode: RestreamMode,
) -> Result<DiscRestreamInfo, String> {
    let held = match &mode {
        RestreamMode::Renderer { item, .. } => *item,
        _ => 0,
    };
    let r = start_inner(state, resolved_path, port, mode);
    if r.is_err() && held != 0 {
        let lib = poison(&state.lib).as_ref().cloned();
        if let Some(lib) = lib {
            release_item(&lib, held);
        }
    }
    r
}

fn start_inner(
    state: &PlayerState,
    resolved_path: &str,
    port: u16,
    mode: RestreamMode,
) -> Result<DiscRestreamInfo, String> {
    // One restream at a time: replace any previous one.
    stop(state);

    let lib = {
        let mut guard = state.lib.lock().map_err(|_| "player state poisoned")?;
        if guard.is_none() {
            *guard = Some(std::sync::Arc::new(engine::VlcLib::load()?));
        }
        guard.as_ref().unwrap().clone()
    };
    let instance = state.shared_instance.load(Ordering::SeqCst);
    if instance == 0 {
        return Err("No active playback session: start the disc locally first".into());
    }
    // A receiver can only reach a LAN address; a loopback-only bind would
    // hand it a URL that never works, so refuse instead.
    let lan_ip = match mode {
        RestreamMode::Renderer { .. } => None,
        _ => Some(primary_lan_ipv4().ok_or(
            "No LAN network address found: connect to a network so the receiver can reach this computer",
        )?),
    };

    let is_bluray = engine::current_session_is_bluray();
    let (title_index, duration_ms, dvd_title_number) = {
        let mp = *poison(&state.media_player);
        if mp == 0 {
            (0, 0, None)
        } else {
            match unsafe { engine::snapshot_titles(&lib, mp) } {
                Some((titles, _current)) => {
                    let idx = engine::main_feature_title(&titles);
                    let dur = idx
                        .and_then(|i| titles.get(i))
                        .map(|t| t.duration_ms)
                        .unwrap_or(0);
                    // dvdread titles are 1-based; the measured mapping puts
                    // the feature's snapshot index at the same number
                    // (Glass Fleet D1: index 1 -> dvdread #1 -> 7,213,066 ms
                    // feature streamed successfully).
                    (idx.map(|i| i as i32).unwrap_or(0), dur, idx)
                }
                None => (0, 0, None),
            }
        }
    };

    let is_disc = is_bluray || std::path::Path::new(resolved_path).is_dir() || {
        let l = resolved_path.to_ascii_lowercase();
        l.ends_with(".iso")
    };
    let mrl = if is_disc {
        build_restream_mrl(resolved_path, is_bluray, dvd_title_number)
    } else {
        String::new()
    };
    let media = unsafe {
        if is_disc {
            let c_mrl = CString::new(mrl.as_str()).map_err(|_| "bad path".to_string())?;
            (lib.libvlc_media_new_location)(instance as *mut std::os::raw::c_void, c_mrl.as_ptr())
        } else {
            let c_path = CString::new(resolved_path).map_err(|_| "bad path".to_string())?;
            (lib.libvlc_media_new_path)(instance as *mut std::os::raw::c_void, c_path.as_ptr())
        }
    };
    if media.is_null() {
        return Err(format!("libvlc could not open {resolved_path}"));
    }
    let scratch = match mode {
        RestreamMode::Hls => {
            let swept =
                hls_cast::sweep_orphans(&std::env::temp_dir(), ORPHAN_AGE, SystemTime::now(), None);
            if swept > 0 {
                tracing::info!("[disc-restream] swept {swept} orphaned scratch dir(s)");
            }
            let d = std::env::temp_dir().join(format!("nebulae-cast-{}", uuid::Uuid::new_v4()));
            if let Err(e) = std::fs::create_dir_all(&d) {
                unsafe { (lib.libvlc_media_release)(media) };
                return Err(format!("cannot create the segment directory: {e}"));
            }
            Some(d)
        }
        _ => None,
    };
    let chain = match (&mode, &scratch) {
        (RestreamMode::Hls, Some(d)) => match hls_cast::build_sout_chain(d) {
            Ok(c) => Some(c),
            Err(e) => {
                unsafe { (lib.libvlc_media_release)(media) };
                let _ = std::fs::remove_dir_all(d);
                return Err(e);
            }
        },
        (RestreamMode::RawTs, _) => Some(build_sout_chain(port)),
        _ => None,
    };
    if let Some(chain) = chain {
        if let Ok(opt) = CString::new(chain.as_str()) {
            unsafe { (lib.libvlc_media_add_option)(media, opt.as_ptr()) };
        }
    }
    if let RestreamMode::Renderer { start_time_s, .. } = &mode {
        if !is_disc && *start_time_s > 0.0 {
            if let Ok(o) = CString::new(format!(":start-time={start_time_s:.3}")) {
                unsafe { (lib.libvlc_media_add_option)(media, o.as_ptr()) };
            }
        }
    }
    // No menus on the receiver: the demuxer goes straight to the feature.
    if is_disc {
        for opt in [":no-bluray-menu", ":no-dvdnav-menu"] {
            if let Ok(o) = CString::new(opt) {
                unsafe { (lib.libvlc_media_add_option)(media, o.as_ptr()) };
            }
        }
    }
    let cleanup_scratch = |d: &Option<std::path::PathBuf>| {
        if let Some(d) = d {
            let _ = std::fs::remove_dir_all(d);
        }
    };
    let mp = unsafe { (lib.libvlc_media_player_new_from_media)(media) };
    if mp.is_null() {
        unsafe { (lib.libvlc_media_release)(media) };
        cleanup_scratch(&scratch);
        return Err("restream media player creation failed".into());
    }
    let mut renderer_set = false;
    if let RestreamMode::Renderer { item, .. } = &mode {
        match lib.libvlc_media_player_set_renderer {
            Some(set) if *item != 0 => {
                let rc = unsafe { set(mp, *item as *mut std::os::raw::c_void) };
                if rc != 0 {
                    unsafe {
                        (lib.libvlc_media_player_release)(mp);
                        (lib.libvlc_media_release)(media);
                    }
                    return Err(format!("libvlc refused the renderer (rc={rc})"));
                }
                renderer_set = true;
            }
            _ => {
                unsafe {
                    (lib.libvlc_media_player_release)(mp);
                    (lib.libvlc_media_release)(media);
                }
                return Err("this libvlc has no renderer support".into());
            }
        }
    }
    let rc = unsafe { (lib.libvlc_media_player_play)(mp) };
    if rc != 0 {
        unsafe {
            if renderer_set {
                if let Some(set) = lib.libvlc_media_player_set_renderer {
                    set(mp, std::ptr::null_mut());
                }
            }
            (lib.libvlc_media_player_release)(mp);
            (lib.libvlc_media_release)(media);
        }
        cleanup_scratch(&scratch);
        return Err(format!("restream play failed (rc={rc})"));
    }

    // HLS: serve the segments on ONE LAN interface; the playlist is built
    // from the PTS in the segment files and ends when the encoder ends.
    let host = lan_ip.map(|ip| ip.to_string());
    let mut lan = None;
    let mut gate = None;
    if let (RestreamMode::Hls, Some(dir), Some(bind)) = (&mode, &scratch, lan_ip) {
        let g = PlayerGate::new(mp as usize);
        let (lib_end, g_end) = (lib.clone(), g.clone());
        let ended = Box::new(move || {
            // 5 Stopped, 6 Ended, 7 Error. A closed gate means the player is
            // gone: report ended.
            g_end
                .with(|mp| {
                    matches!(
                        unsafe {
                            (lib_end.libvlc_media_player_get_state)(mp as *mut std::os::raw::c_void)
                        },
                        5..=7
                    )
                })
                .unwrap_or(true)
        });
        let (lib_lim, g_lim) = (lib.clone(), g.clone());
        let on_limit = Box::new(move || {
            g_lim.with(|mp| unsafe {
                (lib_lim.libvlc_media_player_stop)(mp as *mut std::os::raw::c_void)
            });
        });
        gate = Some(g.clone());
        match hls_cast::LanServer::start_with_limit(
            dir.clone(),
            bind,
            port,
            ended,
            SCRATCH_LIMIT_BYTES,
            on_limit,
        ) {
            Ok(s) => lan = Some(s),
            Err(e) => {
                g.close();
                unsafe {
                    (lib.libvlc_media_player_stop)(mp);
                    (lib.libvlc_media_player_release)(mp);
                    (lib.libvlc_media_release)(media);
                }
                cleanup_scratch(&scratch);
                return Err(format!("cannot start the LAN server: {e}"));
            }
        }
    }
    let served_port = lan.as_ref().map(|s| s.addr.port()).unwrap_or(port);
    let host = host.unwrap_or_else(|| "127.0.0.1".to_string());
    let url = match (&mode, &lan) {
        (RestreamMode::Hls, Some(s)) => s.playlist_url(&host),
        (RestreamMode::RawTs, _) => format!("http://{host}:{port}/nebulae-cast"),
        _ => String::new(),
    };
    let kind = match mode {
        RestreamMode::Hls => "hls",
        RestreamMode::RawTs => "ts",
        RestreamMode::Renderer { .. } => "renderer",
    };

    // A poisoned slot must not leak the already-playing player: tear it
    // down before surfacing the error.
    let rs = DiscRestream {
        media_player: mp as usize,
        media: media as usize,
        port: served_port,
        title_index,
        lan,
        gate,
        scratch,
        renderer_set,
        renderer_item: if let RestreamMode::Renderer { item, .. } = &mode {
            *item
        } else {
            0
        },
        local_prior: PriorLocal::Other,
    };
    let rejected = match state.disc_restream.lock() {
        Ok(mut slot) => {
            *slot = Some(rs);
            None
        }
        Err(_) => Some(rs),
    };
    if let Some(mut rs) = rejected {
        // The caller (`start`) releases the held item on this Err.
        rs.renderer_item = 0;
        teardown(rs, Some(lib.clone()));
        return Err("player state poisoned: restream player released, not started".into());
    }
    tracing::info!(
        "[disc-restream] started: kind={kind} mrl={mrl} port={served_port} title_index={title_index} duration_ms={duration_ms}"
    );
    Ok(DiscRestreamInfo {
        url,
        port: served_port,
        title_index,
        duration_ms,
        kind: kind.to_string(),
        session_id: mp as usize,
    })
}

/// Result of `wait_for_segments`.
#[derive(Debug, PartialEq)]
pub enum Wait {
    Ready,
    /// The restream was stopped (player closed, app exit, user pressed Stop)
    /// or replaced by another one.
    Closed,
    /// The encoder ended before enough segments existed.
    EncoderEnded,
    TimedOut,
}

/// True while restream `session_id` is the live one.
pub fn is_active(state: &PlayerState, session_id: usize) -> bool {
    poison(&state.disc_restream)
        .as_ref()
        .is_some_and(|r| r.media_player == session_id)
}

/// Wait until `need` HLS segments are servable. Returns at once, not at the
/// timeout, when the restream is stopped or replaced. The restream mutex is
/// held only to clone a probe, never while the segment directory is scanned.
pub async fn wait_for_segments(
    state: &PlayerState,
    session_id: usize,
    need: usize,
    timeout: Duration,
    poll: Duration,
) -> Wait {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let probe = {
            let slot = poison(&state.disc_restream);
            match slot.as_ref() {
                Some(r) if r.media_player == session_id => r.lan.as_ref().map(|l| l.probe()),
                _ => return Wait::Closed,
            }
        };
        let Some(probe) = probe else {
            return Wait::Closed;
        };
        if probe.ready_segments() >= need {
            return Wait::Ready;
        }
        if probe.ended() {
            return Wait::EncoderEnded;
        }
        if std::time::Instant::now() >= deadline {
            return Wait::TimedOut;
        }
        tokio::time::sleep(poll).await;
    }
}

/// Stop and release the restream player, if any. Safe to call any time.
pub fn stop(state: &PlayerState) {
    let existing = state
        .disc_restream
        .lock()
        .map_err(|_| "player state poisoned")
        .ok()
        .and_then(|mut g| g.take());
    let Some(rs) = existing else { return };
    // Poisoned lock still yields the data (same policy as `poison` above);
    // the handles MUST be released or the HTTP stream keeps serving.
    let lib = {
        let guard = state.lib.lock().unwrap_or_else(|p| p.into_inner());
        guard.as_ref().cloned()
    };
    teardown(rs, lib);
}

/// Release everything a restream owns, in the only safe order: stop serving,
/// close the gate (no LAN callback can reach the player afterwards, even from
/// a connection that outlived the server's join), release the player, then
/// delete the scratch directory. The scratch directory is deleted on EVERY
/// path, including a missing libvlc handle.
fn teardown(mut rs: DiscRestream, lib: Option<Arc<engine::VlcLib>>) {
    if let Some(mut lan) = rs.lan.take() {
        lan.stop();
    }
    if let Some(g) = rs.gate.take() {
        g.close();
    }
    let scratch = rs.scratch.take();
    let lib_for_item = lib.clone();
    match lib {
        Some(lib) => {
            let mp_ptr = rs.media_player as *mut std::os::raw::c_void;
            let media_ptr = rs.media as *mut std::os::raw::c_void;
            unsafe {
                if rs.renderer_set {
                    if let Some(set) = lib.libvlc_media_player_set_renderer {
                        set(mp_ptr, std::ptr::null_mut());
                    }
                }
                (lib.libvlc_media_player_stop)(mp_ptr);
                (lib.libvlc_media_player_release)(mp_ptr);
                (lib.libvlc_media_release)(media_ptr);
            }
        }
        None => tracing::error!(
            "[disc-restream] lib unavailable; restream handles not released (port={})",
            rs.port
        ),
    }
    // The player is gone; only now may the renderer item reference go.
    if rs.renderer_item != 0 {
        match &lib_for_item {
            Some(lib) => release_item(lib, rs.renderer_item),
            None => tracing::error!("[disc-restream] lib unavailable; renderer item not released"),
        }
    }
    if let Some(d) = scratch {
        let _ = std::fs::remove_dir_all(d);
    }
    tracing::info!(
        "[disc-restream] stopped (port={} title_index={})",
        rs.port,
        rs.title_index
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sout_chain_contains_transcode_and_port() {
        let c = build_sout_chain(18123);
        assert!(c.starts_with(":sout=#transcode{"), "{c}");
        assert!(c.contains("vcodec=h264"));
        assert!(c.contains("acodec=mp4a"));
        assert!(c.contains("std{access=http,mux=ts,dst=0.0.0.0:18123/nebulae-cast}"));
    }

    #[test]
    fn restream_mrl_dvdread_carries_the_feature_title() {
        let want = format!(
            "dvdread://{}#1",
            engine::encode_mrl_path("/Volumes/D/GLASS_FLEET_D1.ISO")
        );
        assert_eq!(
            build_restream_mrl("/Volumes/D/GLASS_FLEET_D1.ISO", false, Some(1)),
            want
        );
    }

    #[test]
    fn restream_mrl_without_title_and_the_bluray_form() {
        assert_eq!(
            build_restream_mrl("/Discs/Banner DVD", false, None),
            format!("dvdread://{}", engine::encode_mrl_path("/Discs/Banner DVD"))
        );
        assert_eq!(
            build_restream_mrl("/Discs/Fuuka", true, Some(2)),
            format!("bluray://{}", engine::encode_mrl_path("/Discs/Fuuka"))
        );
    }

    #[test]
    fn closed_gate_never_calls_the_player_and_close_waits_for_a_call_in_flight() {
        let g = PlayerGate::new(7);
        assert_eq!(g.with(|mp| mp + 1), Some(8));
        let in_call = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (g2, c2) = (g.clone(), in_call.clone());
        let t = std::thread::spawn(move || {
            g2.with(|_| {
                c2.store(true, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(150));
                c2.store(false, Ordering::SeqCst);
            })
        });
        while !in_call.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
        }
        g.close();
        assert!(
            !in_call.load(Ordering::SeqCst),
            "close() returned while a callback was still inside the player"
        );
        t.join().unwrap();
        let mut called = false;
        assert_eq!(
            g.with(|_| {
                called = true;
            }),
            None
        );
        assert!(!called);
    }

    #[test]
    fn stop_deletes_the_scratch_dir_even_without_a_libvlc_handle() {
        let state = PlayerState::default();
        assert!(state.lib.lock().unwrap().is_none());
        let dir = std::env::temp_dir().join(format!("nebulae-cast-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("seg-00000000.ts"), b"x").unwrap();
        *state.disc_restream.lock().unwrap() = Some(DiscRestream {
            media_player: 0,
            media: 0,
            port: 0,
            title_index: 0,
            lan: None,
            gate: Some(PlayerGate::new(0)),
            scratch: Some(dir.clone()),
            renderer_set: false,
            renderer_item: 0,
            local_prior: PriorLocal::Other,
        });
        stop(&state);
        assert!(!dir.exists(), "scratch leaked");
        assert!(state.disc_restream.lock().unwrap().is_none());
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
    }

    /// A restream slot with a real LAN server over `dir`; ended() = `ended`.
    fn fake_session(
        state: &PlayerState,
        id: usize,
        dir: &std::path::Path,
        ended: Arc<std::sync::atomic::AtomicBool>,
    ) {
        let lan = hls_cast::LanServer::start(
            dir.to_path_buf(),
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            Box::new(move || ended.load(Ordering::SeqCst)),
        )
        .unwrap();
        *state.disc_restream.lock().unwrap() = Some(DiscRestream {
            media_player: id,
            media: 0,
            port: 0,
            title_index: 0,
            lan: Some(lan),
            gate: None,
            scratch: None,
            renderer_set: false,
            renderer_item: 0,
            local_prior: PriorLocal::Other,
        });
    }

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("nebulae-wait-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn wait_returns_closed_at_once_when_the_slot_is_empty() {
        let state = PlayerState::default();
        let t = std::time::Instant::now();
        let w = rt().block_on(wait_for_segments(
            &state,
            1,
            2,
            Duration::from_secs(60),
            Duration::from_millis(250),
        ));
        assert_eq!(w, Wait::Closed);
        assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
    }

    #[test]
    fn wait_notices_a_stop_mid_wait_and_a_replaced_session() {
        let dir = tmp("stop");
        let state = Arc::new(PlayerState::default());
        fake_session(&state, 5, &dir, Arc::new(Default::default()));
        let s2 = state.clone();
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            stop(&s2);
        });
        let t = std::time::Instant::now();
        let w = rt().block_on(wait_for_segments(
            &state,
            5,
            2,
            Duration::from_secs(60),
            Duration::from_millis(50),
        ));
        stopper.join().unwrap();
        assert_eq!(w, Wait::Closed);
        assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
        // Another restream took the slot: the old waiter must not wait on it.
        fake_session(&state, 9, &dir, Arc::new(Default::default()));
        let w = rt().block_on(wait_for_segments(
            &state,
            5,
            2,
            Duration::from_secs(60),
            Duration::from_millis(50),
        ));
        assert_eq!(w, Wait::Closed);
        assert!(!is_active(&state, 5) && is_active(&state, 9));
        stop(&state);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wait_reports_ready_ended_and_timeout() {
        let dir = tmp("ready");
        let state = PlayerState::default();
        let ended = Arc::new(std::sync::atomic::AtomicBool::new(false));
        fake_session(&state, 3, &dir, ended.clone());
        let wait = |ms| {
            rt().block_on(wait_for_segments(
                &state,
                3,
                2,
                Duration::from_millis(ms),
                Duration::from_millis(20),
            ))
        };
        assert_eq!(wait(150), Wait::TimedOut);
        // Segments with a real video PES: 3 files -> 2 listed (newest held).
        for i in 0..3u64 {
            let mut p = vec![0xFFu8; 188];
            p[..4].copy_from_slice(&[0x47, 0x41, 0x00, 0x10]);
            p[4..13].copy_from_slice(&[0, 0, 1, 0xE0, 0, 0, 0x80, 0x80, 5]);
            let pts = i * 360_000;
            p[13..18].copy_from_slice(&[
                0x21 | (((pts >> 30) & 7) << 1) as u8,
                (pts >> 22) as u8,
                0x01 | (((pts >> 15) & 0x7f) << 1) as u8,
                (pts >> 7) as u8,
                0x01 | ((pts & 0x7f) << 1) as u8,
            ]);
            std::fs::write(dir.join(format!("seg-{i:08}.ts")), p).unwrap();
        }
        assert_eq!(wait(2000), Wait::Ready);
        std::fs::remove_file(dir.join("seg-00000001.ts")).unwrap();
        std::fs::remove_file(dir.join("seg-00000002.ts")).unwrap();
        ended.store(true, Ordering::SeqCst);
        assert_eq!(wait(2000), Wait::EncoderEnded);
        stop(&state);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prior_local_state_is_restored_exactly() {
        // libvlc states: 0 NothingSpecial 1 Opening 2 Buffering 3 Playing
        // 4 Paused 5 Stopped 6 Ended 7 Error
        let cases = [
            (3, PriorLocal::Playing, true, true),
            (4, PriorLocal::Paused, false, false),
            (0, PriorLocal::Other, false, false),
            (2, PriorLocal::Other, false, false),
            (5, PriorLocal::Other, false, false),
            (6, PriorLocal::Other, false, false),
            (7, PriorLocal::Other, false, false),
        ];
        for (st, want, pause, resume) in cases {
            let p = PriorLocal::from_state(st);
            assert_eq!(p, want, "state {st}");
            assert_eq!(p.pause_on_cast(), pause, "state {st}");
            assert_eq!(p.resume_on_stop(), resume, "state {st}");
        }
    }

    #[test]
    fn prior_is_stored_with_the_restream_and_absent_without_one() {
        let state = PlayerState::default();
        assert_eq!(local_prior(&state), PriorLocal::Other, "no restream");
        set_local_prior(&state, PriorLocal::Paused); // no slot: no-op
        let dir = tmp("prior");
        fake_session(&state, 4, &dir, Arc::new(Default::default()));
        assert_eq!(local_prior(&state), PriorLocal::Other);
        set_local_prior(&state, PriorLocal::Paused);
        assert_eq!(local_prior(&state), PriorLocal::Paused);
        stop(&state);
        assert_eq!(local_prior(&state), PriorLocal::Other, "gone after stop");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scratch_cap_covers_the_estimated_hourly_rate() {
        // ~2.5 Mbps video + 192 kbps audio, per hour, in bytes.
        let per_hour = (2_500_000u64 + 192_000) / 8 * 3600;
        assert!(
            (1_000_000_000..1_350_000_000).contains(&per_hour),
            "{per_hour}"
        );
        assert!(SCRATCH_LIMIT_BYTES / per_hour >= 4, "cap holds >= 4 h");
    }

    #[test]
    fn restream_mrl_title_zero_is_omitted() {
        assert_eq!(
            build_restream_mrl("/Discs/D1.ISO", false, Some(0)),
            format!("dvdread://{}", engine::encode_mrl_path("/Discs/D1.ISO"))
        );
    }
}
