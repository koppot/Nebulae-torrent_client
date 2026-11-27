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
//!
//! The restream runs on its OWN media player created from the shared
//! libvlc instance; the local playback session is neither stopped nor
//! modified, and nothing about local playback changes on any platform.
use super::engine;
use super::hls_cast;
use crate::player::PlayerState;

use std::ffi::CString;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

/// Locks a possibly-poisoned mutex, preferring the data over the guard
/// (same policy as the player commands; poll/teardown data is advisory).
fn poison<T>(g: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    g.lock().unwrap_or_else(|p| p.into_inner())
}

/// Default TCP port for the local HTTP restream.
pub const DEFAULT_PORT: u16 = 18123;

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
    pub scratch: Option<std::path::PathBuf>,
    pub renderer_set: bool,
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
fn primary_lan_ipv4() -> Option<String> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    let local = sock.local_addr().ok()?;
    if local.is_ipv4() {
        Some(local.ip().to_string())
    } else {
        None
    }
}

/// Start streaming the current disc's main feature. Requires an ACTIVE
/// disc session (the shared instance and the title snapshot come from it);
/// the local playback keeps running untouched.
pub fn start(
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
    let host = primary_lan_ipv4();
    let mut lan = None;
    if let (RestreamMode::Hls, Some(dir)) = (&mode, &scratch) {
        let bind = host
            .as_deref()
            .and_then(|h| h.parse().ok())
            .unwrap_or_else(hls_cast::loopback);
        let lib_end = lib.clone();
        let mp_addr = mp as usize;
        let ended = Box::new(move || {
            // 5 Stopped, 6 Ended, 7 Error. The server is stopped before the
            // player is released (see stop()), so mp_addr is live here.
            matches!(
                unsafe {
                    (lib_end.libvlc_media_player_get_state)(mp_addr as *mut std::os::raw::c_void)
                },
                5..=7
            )
        });
        match hls_cast::LanServer::start(dir.clone(), bind, port, ended) {
            Ok(s) => lan = Some(s),
            Err(e) => {
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

    // A poisoned slot must not leak the already-playing player: release the
    // handles before surfacing the error.
    let stored = match state.disc_restream.lock() {
        Ok(mut slot) => {
            *slot = Some(DiscRestream {
                media_player: mp as usize,
                media: media as usize,
                port: served_port,
                title_index,
                lan,
                scratch,
                renderer_set,
            });
            true
        }
        Err(_) => {
            unsafe {
                (lib.libvlc_media_player_stop)(mp);
                (lib.libvlc_media_player_release)(mp);
                (lib.libvlc_media_release)(media);
            }
            false
        }
    };
    if !stored {
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
    })
}

/// Stop and release the restream player, if any. Safe to call any time.
pub fn stop(state: &PlayerState) {
    let existing = state
        .disc_restream
        .lock()
        .map_err(|_| "player state poisoned")
        .ok()
        .and_then(|mut g| g.take());
    let Some(mut rs) = existing else { return };
    // The LAN server's "ended" callback reads the player: stop it (joins) FIRST.
    if let Some(mut lan) = rs.lan.take() {
        lan.stop();
    }
    // Poisoned lock still yields the data (same policy as `poison` above);
    // the handles MUST be released or the HTTP stream keeps serving.
    let lib = {
        let guard = state.lib.lock().unwrap_or_else(|p| p.into_inner());
        guard.as_ref().cloned()
    };
    let Some(lib) = lib else {
        tracing::error!(
            "[disc-restream] lib unavailable; restream handles not released (port={})",
            rs.port
        );
        return;
    };
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
    if let Some(d) = rs.scratch.take() {
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
    fn restream_mrl_title_zero_is_omitted() {
        assert_eq!(
            build_restream_mrl("/Discs/D1.ISO", false, Some(0)),
            format!("dvdread://{}", engine::encode_mrl_path("/Discs/D1.ISO"))
        );
    }
}
