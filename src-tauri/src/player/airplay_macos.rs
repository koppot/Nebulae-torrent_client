//! Native macOS AirPlay / Apple TV bridge using AVFoundation and AVKit.
//!
//! Provides an AVPlayer output path anchored to an authentic AVRoutePickerView
//! embedded in Nebulae's window, observing external playback state and routing
//! player controls to AVPlayer during AirPlay sessions.

#[cfg(target_os = "macos")]
pub mod implementation {
    use objc2::encode::Encode;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send, sel};
    use objc2_app_kit::{NSView, NSWindow, NSWindowOrderingMode};
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    use serde::{Deserialize, Serialize};
    use std::ffi::{c_char, CStr};
    use std::sync::Mutex;
    use tauri::{AppHandle, Manager};

    extern "C" {
        static AVMediaCharacteristicAudible: *mut AnyObject;
        static AVMediaCharacteristicLegible: *mut AnyObject;
    }

    #[repr(C)]
    #[derive(Copy, Clone, Debug)]
    pub struct CMTime {
        pub value: i64,
        pub timescale: i32,
        pub flags: u32,
        pub epoch: i64,
    }

    unsafe impl objc2::encode::Encode for CMTime {
        const ENCODING: objc2::encode::Encoding = objc2::encode::Encoding::Struct(
            "CMTime",
            &[i64::ENCODING, i32::ENCODING, u32::ENCODING, i64::ENCODING],
        );
    }

    unsafe impl objc2::encode::RefEncode for CMTime {
        const ENCODING_REF: objc2::encode::Encoding =
            objc2::encode::Encoding::Pointer(&Self::ENCODING);
    }

    impl CMTime {
        pub fn from_seconds(s: f64) -> Self {
            let clamped = if s.is_finite() && s >= 0.0 { s } else { 0.0 };
            Self {
                value: (clamped * 1000.0) as i64,
                timescale: 1000,
                flags: 1, // kCMTimeFlags_Valid
                epoch: 0,
            }
        }

        pub fn zero() -> Self {
            Self {
                value: 0,
                timescale: 1000,
                flags: 1,
                epoch: 0,
            }
        }

        pub fn to_seconds(&self) -> f64 {
            if self.timescale > 0 && (self.flags & 1) != 0 {
                self.value as f64 / self.timescale as f64
            } else {
                0.0
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AirPlayTrackOption {
        pub index: usize,
        pub display_name: String,
        pub language_code: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AirPlayPreflightResult {
        pub is_playable: bool,
        pub audio_compatible: bool,
        pub subtitle_compatible: bool,
        pub audio_options: Vec<AirPlayTrackOption>,
        pub subtitle_options: Vec<AirPlayTrackOption>,
        pub matched_audio_idx: Option<usize>,
        pub matched_subtitle_idx: Option<usize>,
        pub rejection_reason: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AirPlayNativeStatus {
        pub is_active: bool,
        pub is_external_active: bool,
        pub is_playing: bool,
        pub position_ms: i64,
        pub duration_ms: i64,
        pub error: Option<String>,
    }

    #[allow(dead_code)]
    struct AirPlaySessionHolder {
        player: *mut AnyObject,
        player_item: *mut AnyObject,
        asset: *mut AnyObject,
        route_picker: *mut AnyObject,
        audio_group: *mut AnyObject,
        subtitle_group: *mut AnyObject,
        media_path: String,
        last_known_pos_ms: i64,
        is_playing: bool,
    }

    unsafe impl Send for AirPlaySessionHolder {}
    unsafe impl Sync for AirPlaySessionHolder {}

    static AIRPLAY_SESSION: Mutex<Option<AirPlaySessionHolder>> = Mutex::new(None);
    static ROUTE_PICKER_VIEW: Mutex<usize> = Mutex::new(0);

    /// Preflight check verifying AVFoundation playability and track mapping for a local media file.
    pub fn preflight(
        file_path: &str,
        vlc_audio_name: Option<&str>,
        vlc_audio_lang: Option<&str>,
        vlc_sub_name: Option<&str>,
        vlc_sub_lang: Option<&str>,
        is_external_sub: bool,
    ) -> AirPlayPreflightResult {
        if is_external_sub {
            return AirPlayPreflightResult {
                is_playable: false,
                audio_compatible: false,
                subtitle_compatible: false,
                audio_options: Vec::new(),
                subtitle_options: Vec::new(),
                matched_audio_idx: None,
                matched_subtitle_idx: None,
                rejection_reason: Some(
                    "External subtitle files (.srt/.ass) are not supported by native AirPlay without conversion. Turn subtitles off or use embedded subtitles to stream via AirPlay."
                        .to_string(),
                ),
            };
        }

        unsafe {
            let ns_path = NSString::from_str(file_path);
            let url_cls = class!(NSURL);
            let file_url: *mut AnyObject = msg_send![url_cls, fileURLWithPath: &*ns_path];
            if file_url.is_null() {
                return AirPlayPreflightResult {
                    is_playable: false,
                    audio_compatible: false,
                    subtitle_compatible: false,
                    audio_options: Vec::new(),
                    subtitle_options: Vec::new(),
                    matched_audio_idx: None,
                    matched_subtitle_idx: None,
                    rejection_reason: Some("Invalid file path URL for AVFoundation.".to_string()),
                };
            }

            let asset_cls = class!(AVURLAsset);
            let asset: *mut AnyObject = msg_send![asset_cls, URLAssetWithURL: file_url, options: std::ptr::null::<AnyObject>()];
            if asset.is_null() {
                return AirPlayPreflightResult {
                    is_playable: false,
                    audio_compatible: false,
                    subtitle_compatible: false,
                    audio_options: Vec::new(),
                    subtitle_options: Vec::new(),
                    matched_audio_idx: None,
                    matched_subtitle_idx: None,
                    rejection_reason: Some("AVFoundation could not allocate asset.".to_string()),
                };
            }

            let is_playable: bool = msg_send![asset, isPlayable];
            if !is_playable {
                return AirPlayPreflightResult {
                    is_playable: false,
                    audio_compatible: false,
                    subtitle_compatible: false,
                    audio_options: Vec::new(),
                    subtitle_options: Vec::new(),
                    matched_audio_idx: None,
                    matched_subtitle_idx: None,
                    rejection_reason: Some(
                        "This file format is not natively playable by AVFoundation/AirPlay without conversion. Native AirPlay requires an Apple-supported container (e.g. MP4, MOV, M4V)."
                            .to_string(),
                    ),
                };
            }

            // Inspect Audible group
            let audio_group: *mut AnyObject = msg_send![asset, mediaSelectionGroupForMediaCharacteristic: AVMediaCharacteristicAudible];
            let mut audio_options = Vec::new();
            if !audio_group.is_null() {
                let options_arr: *mut AnyObject = msg_send![audio_group, options];
                if !options_arr.is_null() {
                    let count: usize = msg_send![options_arr, count];
                    for i in 0..count {
                        let opt: *mut AnyObject = msg_send![options_arr, objectAtIndex: i];
                        let name_str = get_nsstring_prop(opt, sel!(displayName));
                        let lang_str = get_option_language(opt);
                        audio_options.push(AirPlayTrackOption {
                            index: i,
                            display_name: name_str
                                .unwrap_or_else(|| format!("Audio Track {}", i + 1)),
                            language_code: lang_str,
                        });
                    }
                }
            }

            // Inspect Legible group
            let sub_group: *mut AnyObject = msg_send![asset, mediaSelectionGroupForMediaCharacteristic: AVMediaCharacteristicLegible];
            let mut sub_options = Vec::new();
            if !sub_group.is_null() {
                let options_arr: *mut AnyObject = msg_send![sub_group, options];
                if !options_arr.is_null() {
                    let count: usize = msg_send![options_arr, count];
                    for i in 0..count {
                        let opt: *mut AnyObject = msg_send![options_arr, objectAtIndex: i];
                        let name_str = get_nsstring_prop(opt, sel!(displayName));
                        let lang_str = get_option_language(opt);
                        sub_options.push(AirPlayTrackOption {
                            index: i,
                            display_name: name_str
                                .unwrap_or_else(|| format!("Subtitle Track {}", i + 1)),
                            language_code: lang_str,
                        });
                    }
                }
            }

            // Match selected audio track
            let mut matched_audio_idx = None;
            let mut audio_compatible = true;
            let mut audio_rejection_reason = None;

            if audio_options.is_empty() {
                matched_audio_idx = None;
            } else if vlc_audio_name.is_none() && vlc_audio_lang.is_none() {
                // Default to first track if no preference is asserted
                matched_audio_idx = Some(0);
            } else {
                // 1. Exact display_name match
                if let Some(target_name) = vlc_audio_name {
                    let target_clean = target_name.trim().to_lowercase();
                    let exact_matches: Vec<_> = audio_options
                        .iter()
                        .filter(|o| o.display_name.trim().to_lowercase() == target_clean)
                        .collect();
                    if exact_matches.len() == 1 {
                        matched_audio_idx = Some(exact_matches[0].index);
                    } else if exact_matches.len() > 1 {
                        audio_compatible = false;
                        audio_rejection_reason = Some(format!(
                            "Ambiguous audio track match: multiple tracks share display name '{}'.",
                            target_name
                        ));
                    }
                }

                // 2. Language match
                if matched_audio_idx.is_none() && audio_compatible {
                    if let Some(target_lang) = vlc_audio_lang {
                        let lang_clean = target_lang.trim().to_lowercase();
                        let lang_matches: Vec<_> = audio_options
                            .iter()
                            .filter(|o| {
                                o.language_code
                                    .as_deref()
                                    .unwrap_or("")
                                    .trim()
                                    .eq_ignore_ascii_case(&lang_clean)
                            })
                            .collect();
                        if lang_matches.len() == 1 {
                            matched_audio_idx = Some(lang_matches[0].index);
                        } else if lang_matches.len() > 1 {
                            if let Some(target_name) = vlc_audio_name {
                                let target_clean = target_name.trim().to_lowercase();
                                let disambiguated: Vec<_> = lang_matches
                                    .iter()
                                    .filter(|o| {
                                        let d = o.display_name.trim().to_lowercase();
                                        d.contains(&target_clean) || target_clean.contains(&d)
                                    })
                                    .collect();
                                if disambiguated.len() == 1 {
                                    matched_audio_idx = Some(disambiguated[0].index);
                                } else {
                                    audio_compatible = false;
                                    audio_rejection_reason = Some(format!(
                                        "Ambiguous audio track match: multiple tracks match language '{}'.",
                                        target_lang
                                    ));
                                }
                            } else {
                                audio_compatible = false;
                                audio_rejection_reason = Some(format!(
                                    "Ambiguous audio track match: multiple tracks match language '{}'.",
                                    target_lang
                                ));
                            }
                        }
                    }
                }

                // 3. Substring name match
                if matched_audio_idx.is_none() && audio_compatible {
                    if let Some(target_name) = vlc_audio_name {
                        let target_clean = target_name.trim().to_lowercase();
                        let sub_matches: Vec<_> = audio_options
                            .iter()
                            .filter(|o| {
                                let d = o.display_name.trim().to_lowercase();
                                d.contains(&target_clean) || target_clean.contains(&d)
                            })
                            .collect();
                        if sub_matches.len() == 1 {
                            matched_audio_idx = Some(sub_matches[0].index);
                        } else if sub_matches.len() > 1 {
                            audio_compatible = false;
                            audio_rejection_reason = Some(format!(
                                "Ambiguous audio track match: multiple tracks match '{}'.",
                                target_name
                            ));
                        }
                    }
                }

                if matched_audio_idx.is_none() && audio_rejection_reason.is_none() {
                    if audio_options.len() == 1 {
                        matched_audio_idx = Some(0);
                    } else {
                        audio_compatible = false;
                        audio_rejection_reason = Some(
                            "Selected audio track could not be unambiguously mapped to an embedded audio track."
                                .to_string(),
                        );
                    }
                }
            }

            // Match selected subtitle track
            let mut matched_sub_idx = None;
            let mut subtitle_compatible = true;
            let mut sub_rejection_reason = None;

            if vlc_sub_name.is_none() && vlc_sub_lang.is_none() {
                // User has subtitles off: fully compatible (nil selection preserved)
                matched_sub_idx = None;
                subtitle_compatible = true;
            } else if sub_options.is_empty() {
                subtitle_compatible = false;
                sub_rejection_reason = Some(
                    "Selected subtitle track cannot be played: media file contains no embedded subtitle tracks."
                        .to_string(),
                );
            } else {
                // 1. Exact name match
                if let Some(target_name) = vlc_sub_name {
                    let target_clean = target_name.trim().to_lowercase();
                    let exact_matches: Vec<_> = sub_options
                        .iter()
                        .filter(|o| o.display_name.trim().to_lowercase() == target_clean)
                        .collect();
                    if exact_matches.len() == 1 {
                        matched_sub_idx = Some(exact_matches[0].index);
                    } else if exact_matches.len() > 1 {
                        subtitle_compatible = false;
                        sub_rejection_reason = Some(format!(
                            "Ambiguous subtitle track match: multiple tracks share display name '{}'.",
                            target_name
                        ));
                    }
                }

                // 2. Language match
                if matched_sub_idx.is_none() && subtitle_compatible {
                    if let Some(target_lang) = vlc_sub_lang {
                        let lang_clean = target_lang.trim().to_lowercase();
                        let lang_matches: Vec<_> = sub_options
                            .iter()
                            .filter(|o| {
                                o.language_code
                                    .as_deref()
                                    .unwrap_or("")
                                    .trim()
                                    .eq_ignore_ascii_case(&lang_clean)
                            })
                            .collect();
                        if lang_matches.len() == 1 {
                            matched_sub_idx = Some(lang_matches[0].index);
                        } else if lang_matches.len() > 1 {
                            if let Some(target_name) = vlc_sub_name {
                                let target_clean = target_name.trim().to_lowercase();
                                let disambiguated: Vec<_> = lang_matches
                                    .iter()
                                    .filter(|o| {
                                        let d = o.display_name.trim().to_lowercase();
                                        d.contains(&target_clean) || target_clean.contains(&d)
                                    })
                                    .collect();
                                if disambiguated.len() == 1 {
                                    matched_sub_idx = Some(disambiguated[0].index);
                                } else {
                                    subtitle_compatible = false;
                                    sub_rejection_reason = Some(format!(
                                        "Ambiguous subtitle track match: multiple tracks match language '{}'.",
                                        target_lang
                                    ));
                                }
                            } else {
                                subtitle_compatible = false;
                                sub_rejection_reason = Some(format!(
                                    "Ambiguous subtitle track match: multiple tracks match language '{}'.",
                                    target_lang
                                ));
                            }
                        }
                    }
                }

                // 3. Substring name match
                if matched_sub_idx.is_none() && subtitle_compatible {
                    if let Some(target_name) = vlc_sub_name {
                        let target_clean = target_name.trim().to_lowercase();
                        let sub_matches: Vec<_> = sub_options
                            .iter()
                            .filter(|o| {
                                let d = o.display_name.trim().to_lowercase();
                                d.contains(&target_clean) || target_clean.contains(&d)
                            })
                            .collect();
                        if sub_matches.len() == 1 {
                            matched_sub_idx = Some(sub_matches[0].index);
                        } else if sub_matches.len() > 1 {
                            subtitle_compatible = false;
                            sub_rejection_reason = Some(format!(
                                "Ambiguous subtitle track match: multiple tracks match '{}'.",
                                target_name
                            ));
                        }
                    }
                }

                if matched_sub_idx.is_none() && sub_rejection_reason.is_none() {
                    subtitle_compatible = false;
                    sub_rejection_reason = Some(
                        "Selected subtitle track could not be unambiguously mapped to an embedded subtitle track."
                            .to_string(),
                    );
                }
            }

            let rejection_reason = if !audio_compatible {
                audio_rejection_reason
            } else if !subtitle_compatible {
                sub_rejection_reason
            } else {
                None
            };

            AirPlayPreflightResult {
                is_playable: true,
                audio_compatible,
                subtitle_compatible,
                audio_options,
                subtitle_options: sub_options,
                matched_audio_idx,
                matched_subtitle_idx: matched_sub_idx,
                rejection_reason,
            }
        }
    }

    /// Prepares an AVPlayer session with the selected audio and subtitle options,
    /// binds it to the mounted AVRoutePickerView, and seeks to the start time.
    pub fn prepare_session(
        _app: &AppHandle,
        file_path: &str,
        start_position_ms: i64,
        matched_audio_idx: Option<usize>,
        matched_sub_idx: Option<usize>,
    ) -> Result<(), String> {
        prepare_session_inner(
            file_path,
            start_position_ms,
            matched_audio_idx,
            matched_sub_idx,
        )
    }

    /// Inner helper callable in tests without requiring an active GUI Tauri AppHandle.
    pub fn prepare_session_inner(
        file_path: &str,
        start_position_ms: i64,
        matched_audio_idx: Option<usize>,
        matched_sub_idx: Option<usize>,
    ) -> Result<(), String> {
        prepare_from(
            file_path,
            false,
            start_position_ms,
            matched_audio_idx,
            matched_sub_idx,
        )
    }

    /// AirPlay an HLS playlist served over LAN HTTP (disc restream). The
    /// receiver pulls the segments from this Mac; no local file is opened.
    pub fn prepare_session_url(url: &str, start_position_ms: i64) -> Result<(), String> {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err("AirPlay URL sessions accept http(s) URLs only.".into());
        }
        prepare_from(url, true, start_position_ms, None, None)
    }

    fn prepare_from(
        source: &str,
        is_url: bool,
        start_position_ms: i64,
        matched_audio_idx: Option<usize>,
        matched_sub_idx: Option<usize>,
    ) -> Result<(), String> {
        let file_path = source;
        unsafe {
            // Teardown any lingering prior session before reallocating
            let _ = teardown();

            let ns_path = NSString::from_str(file_path);
            let url_cls = class!(NSURL);
            let file_url: *mut AnyObject = if is_url {
                msg_send![url_cls, URLWithString: &*ns_path]
            } else {
                msg_send![url_cls, fileURLWithPath: &*ns_path]
            };
            if file_url.is_null() {
                return Err("Failed to create the URL for AVFoundation.".into());
            }

            let asset_cls = class!(AVURLAsset);
            let asset: *mut AnyObject = msg_send![asset_cls, URLAssetWithURL: file_url, options: std::ptr::null::<AnyObject>()];
            if asset.is_null() {
                return Err("Failed to create AVURLAsset.".into());
            }

            // A remote HLS asset is not loaded yet; isPlayable would block on
            // the network, so AVPlayerItem status reports failures instead.
            let is_playable: bool = is_url || msg_send![asset, isPlayable];
            if !is_playable {
                return Err(
                    "File is not playable by AVFoundation without conversion. Native AirPlay requires an Apple-supported format."
                        .into(),
                );
            }

            let item_cls = class!(AVPlayerItem);
            let player_item: *mut AnyObject = msg_send![item_cls, playerItemWithAsset: asset];
            if player_item.is_null() {
                return Err("Failed to create AVPlayerItem.".into());
            }

            // Apply and verify audio track selection
            let audio_group: *mut AnyObject = msg_send![asset, mediaSelectionGroupForMediaCharacteristic: AVMediaCharacteristicAudible];
            if !audio_group.is_null() {
                if let Some(idx) = matched_audio_idx {
                    let options_arr: *mut AnyObject = msg_send![audio_group, options];
                    let count: usize = msg_send![options_arr, count];
                    if idx < count {
                        let opt: *mut AnyObject = msg_send![options_arr, objectAtIndex: idx];
                        let _: () = msg_send![player_item, selectMediaOption: opt, inMediaSelectionGroup: audio_group];
                        let selected: *mut AnyObject = msg_send![player_item, selectedMediaOptionInMediaSelectionGroup: audio_group];
                        if selected.is_null() {
                            return Err(
                                "AVPlayerItem failed to verify selected audio track option.".into(),
                            );
                        }
                    } else {
                        return Err(format!(
                            "Audio track index {idx} out of bounds (count={count})"
                        ));
                    }
                }
            }

            // Apply and verify subtitle track selection
            let sub_group: *mut AnyObject = msg_send![asset, mediaSelectionGroupForMediaCharacteristic: AVMediaCharacteristicLegible];
            if !sub_group.is_null() {
                if let Some(idx) = matched_sub_idx {
                    let options_arr: *mut AnyObject = msg_send![sub_group, options];
                    let count: usize = msg_send![options_arr, count];
                    if idx < count {
                        let opt: *mut AnyObject = msg_send![options_arr, objectAtIndex: idx];
                        let _: () = msg_send![player_item, selectMediaOption: opt, inMediaSelectionGroup: sub_group];
                        let selected: *mut AnyObject = msg_send![player_item, selectedMediaOptionInMediaSelectionGroup: sub_group];
                        if selected.is_null() {
                            return Err(
                                "AVPlayerItem failed to verify selected subtitle track option."
                                    .into(),
                            );
                        }
                    } else {
                        return Err(format!(
                            "Subtitle track index {idx} out of bounds (count={count})"
                        ));
                    }
                } else {
                    // Turn off subtitles in legible group
                    let nil_opt: *mut AnyObject = std::ptr::null_mut();
                    let _: () = msg_send![player_item, selectMediaOption: nil_opt, inMediaSelectionGroup: sub_group];
                }
            }

            let player_cls = class!(AVPlayer);
            let player: *mut AnyObject = msg_send![player_cls, playerWithPlayerItem: player_item];
            if player.is_null() {
                return Err("Failed to create AVPlayer.".into());
            }

            // Mute local AVPlayer audio until external playback is confirmed active
            let _: () = msg_send![player, setMuted: true];

            // Enable external playback (AirPlay)
            let _: () = msg_send![player, setAllowsExternalPlayback: true];

            // Seek to start position
            let start_s = (start_position_ms as f64) / 1000.0;
            let cm_time = CMTime::from_seconds(start_s);
            let zero_time = CMTime::zero();
            let _: () = msg_send![player, seekToTime: cm_time, toleranceBefore: zero_time, toleranceAfter: zero_time];

            // Bind to route picker if mounted
            let picker_ptr = *ROUTE_PICKER_VIEW.lock().unwrap() as *mut AnyObject;
            if !picker_ptr.is_null() {
                let _: () = msg_send![picker_ptr, setPlayer: player];
            }

            let mut sess = AIRPLAY_SESSION.lock().unwrap();
            *sess = Some(AirPlaySessionHolder {
                player,
                player_item,
                asset,
                route_picker: picker_ptr,
                audio_group,
                subtitle_group: sub_group,
                media_path: file_path.to_string(),
                last_known_pos_ms: start_position_ms,
                is_playing: false,
            });

            Ok(())
        }
    }

    /// Mounts the native AVRoutePickerView control in the main window at the requested coordinates.
    pub fn mount_route_picker(
        app: &AppHandle,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Result<(), String> {
        let win = app
            .get_webview_window("main")
            .ok_or_else(|| "main window not found".to_string())?;

        let raw = win
            .ns_window()
            .map_err(|e| format!("cannot get ns_window: {e}"))?;
        let raw_usize = raw as usize;

        app.run_on_main_thread(move || {
            unsafe {
                let ns_win = raw_usize as *mut NSWindow;
                if ns_win.is_null() {
                    return;
                }
                let content_view = match (*ns_win).contentView() {
                    Some(cv) => cv,
                    None => return,
                };

                let window_height = content_view.frame().size.height;
                // AppKit has bottom-left origin: flip y coordinate
                let y_appkit = (window_height - y - height).max(0.0);
                let picker_frame = NSRect::new(
                    NSPoint::new(x, y_appkit),
                    NSSize::new(width.max(24.0), height.max(24.0)),
                );

                let mut cur = ROUTE_PICKER_VIEW.lock().unwrap();
                let mut picker_ptr = *cur as *mut AnyObject;
                if picker_ptr.is_null() {
                    let picker_cls = class!(AVRoutePickerView);
                    let alloc_picker: *mut AnyObject = msg_send![picker_cls, alloc];
                    picker_ptr = msg_send![alloc_picker, initWithFrame: picker_frame];
                    if picker_ptr.is_null() {
                        return;
                    }
                    let _: () = msg_send![picker_ptr, setRoutePickerButtonBordered: false];
                    let _: () = msg_send![
                        &*content_view,
                        addSubview: &*(picker_ptr as *mut NSView),
                        positioned: NSWindowOrderingMode::Above,
                        relativeTo: std::ptr::null::<NSView>()
                    ];
                    *cur = picker_ptr as usize;

                    // Bind active player if session is prepared
                    let sess = AIRPLAY_SESSION.lock().unwrap();
                    if let Some(ref s) = *sess {
                        if !s.player.is_null() {
                            let _: () = msg_send![picker_ptr, setPlayer: s.player];
                        }
                    }
                } else {
                    let _: () = msg_send![picker_ptr, setFrame: picker_frame];
                    let _: () = msg_send![picker_ptr, setHidden: false];
                }
            }
        })
        .map_err(|e| format!("run_on_main_thread failed: {e}"))
    }

    /// Unmounts and hides the native AVRoutePickerView.
    pub fn unmount_route_picker(app: &AppHandle) -> Result<(), String> {
        app.run_on_main_thread(move || unsafe {
            let mut cur = ROUTE_PICKER_VIEW.lock().unwrap();
            let picker_ptr = *cur as *mut AnyObject;
            if !picker_ptr.is_null() {
                let _: () = msg_send![picker_ptr, setPlayer: std::ptr::null::<AnyObject>()];
                let _: () = msg_send![picker_ptr, removeFromSuperview];
                *cur = 0;
            }
        })
        .map_err(|e| format!("run_on_main_thread failed: {e}"))
    }

    pub fn play() -> Result<(), String> {
        unsafe {
            let mut sess = AIRPLAY_SESSION.lock().unwrap();
            if let Some(ref mut s) = *sess {
                if !s.player.is_null() {
                    let is_ext: bool = msg_send![s.player, isExternalPlaybackActive];
                    let _: () = msg_send![s.player, setMuted: !is_ext];
                    let _: () = msg_send![s.player, play];
                    s.is_playing = true;
                    return Ok(());
                }
            }
            Err("No active AirPlay session".into())
        }
    }

    pub fn pause() -> Result<(), String> {
        unsafe {
            let mut sess = AIRPLAY_SESSION.lock().unwrap();
            if let Some(ref mut s) = *sess {
                if !s.player.is_null() {
                    let _: () = msg_send![s.player, pause];
                    s.is_playing = false;
                    return Ok(());
                }
            }
            Err("No active AirPlay session".into())
        }
    }

    pub fn seek(position_ms: i64) -> Result<(), String> {
        unsafe {
            let mut sess = AIRPLAY_SESSION.lock().unwrap();
            if let Some(ref mut s) = *sess {
                if !s.player.is_null() {
                    let cm = CMTime::from_seconds((position_ms as f64) / 1000.0);
                    let zero = CMTime::zero();
                    let _: () = msg_send![s.player, seekToTime: cm, toleranceBefore: zero, toleranceAfter: zero];
                    s.last_known_pos_ms = position_ms;
                    return Ok(());
                }
            }
            Err("No active AirPlay session".into())
        }
    }

    pub fn get_status() -> AirPlayNativeStatus {
        unsafe {
            let sess = AIRPLAY_SESSION.lock().unwrap();
            if let Some(ref s) = *sess {
                if !s.player.is_null() {
                    let is_ext: bool = msg_send![s.player, isExternalPlaybackActive];
                    let _: () = msg_send![s.player, setMuted: !is_ext];
                    let time_status: isize = msg_send![s.player, timeControlStatus];
                    let is_playing = time_status == 2;
                    let current_cm: CMTime = msg_send![s.player, currentTime];
                    let pos_ms = (current_cm.to_seconds() * 1000.0) as i64;

                    let mut dur_ms = 0i64;
                    if !s.player_item.is_null() {
                        let dur_cm: CMTime = msg_send![s.player_item, duration];
                        dur_ms = (dur_cm.to_seconds() * 1000.0) as i64;
                    }

                    return AirPlayNativeStatus {
                        is_active: true,
                        is_external_active: is_ext,
                        is_playing,
                        position_ms: pos_ms.max(s.last_known_pos_ms),
                        duration_ms: dur_ms,
                        error: None,
                    };
                }
            }
            AirPlayNativeStatus {
                is_active: false,
                is_external_active: false,
                is_playing: false,
                position_ms: 0,
                duration_ms: 0,
                error: None,
            }
        }
    }

    /// Stops AirPlay playback, tears down AVPlayer and AVPlayerItem, and returns the latest recoverable position in milliseconds.
    pub fn teardown() -> Option<i64> {
        unsafe {
            let mut sess = AIRPLAY_SESSION.lock().unwrap();
            if let Some(s) = sess.take() {
                let mut latest_ms = s.last_known_pos_ms;
                if !s.player.is_null() {
                    let current_cm: CMTime = msg_send![s.player, currentTime];
                    let pos_s = current_cm.to_seconds();
                    if pos_s > 0.0 {
                        latest_ms = (pos_s * 1000.0) as i64;
                    }
                    let _: () = msg_send![s.player, pause];
                    let _: () = msg_send![s.player, setMuted: true];
                    let nil_item: *mut AnyObject = std::ptr::null_mut();
                    let _: () = msg_send![s.player, replaceCurrentItemWithPlayerItem: nil_item];
                }

                let picker_ptr = *ROUTE_PICKER_VIEW.lock().unwrap() as *mut AnyObject;
                if !picker_ptr.is_null() {
                    let _: () = msg_send![picker_ptr, setPlayer: std::ptr::null::<AnyObject>()];
                }

                return Some(latest_ms);
            }
            None
        }
    }

    unsafe fn get_nsstring_prop(obj: *mut AnyObject, sel: objc2::runtime::Sel) -> Option<String> {
        if obj.is_null() {
            return None;
        }
        let str_obj: *mut AnyObject = msg_send![obj, performSelector: sel];
        if str_obj.is_null() {
            return None;
        }
        let utf8: *const c_char = msg_send![str_obj, UTF8String];
        if utf8.is_null() {
            return None;
        }
        CStr::from_ptr(utf8).to_str().ok().map(|s| s.to_string())
    }

    unsafe fn get_option_language(opt: *mut AnyObject) -> Option<String> {
        if opt.is_null() {
            return None;
        }
        let locale: *mut AnyObject = msg_send![opt, locale];
        if locale.is_null() {
            return None;
        }
        let lang: *mut AnyObject = msg_send![locale, languageCode];
        if lang.is_null() {
            return None;
        }
        let utf8: *const c_char = msg_send![lang, UTF8String];
        if utf8.is_null() {
            return None;
        }
        CStr::from_ptr(utf8).to_str().ok().map(|s| s.to_string())
    }
}

#[cfg(not(target_os = "macos"))]
pub mod implementation {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AirPlayTrackOption {
        pub index: usize,
        pub display_name: String,
        pub language_code: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AirPlayPreflightResult {
        pub is_playable: bool,
        pub audio_compatible: bool,
        pub subtitle_compatible: bool,
        pub audio_options: Vec<AirPlayTrackOption>,
        pub subtitle_options: Vec<AirPlayTrackOption>,
        pub matched_audio_idx: Option<usize>,
        pub matched_subtitle_idx: Option<usize>,
        pub rejection_reason: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AirPlayNativeStatus {
        pub is_active: bool,
        pub is_external_active: bool,
        pub is_playing: bool,
        pub position_ms: i64,
        pub duration_ms: i64,
        pub error: Option<String>,
    }

    pub fn preflight(
        _file_path: &str,
        _vlc_audio_name: Option<&str>,
        _vlc_audio_lang: Option<&str>,
        _vlc_sub_name: Option<&str>,
        _vlc_sub_lang: Option<&str>,
        _is_external_sub: bool,
    ) -> AirPlayPreflightResult {
        AirPlayPreflightResult {
            is_playable: false,
            audio_compatible: false,
            subtitle_compatible: false,
            audio_options: Vec::new(),
            subtitle_options: Vec::new(),
            matched_audio_idx: None,
            matched_subtitle_idx: None,
            rejection_reason: Some("Native AirPlay is only supported on macOS.".to_string()),
        }
    }

    pub fn prepare_session(
        _app: &tauri::AppHandle,
        _file_path: &str,
        _start_position_ms: i64,
        _matched_audio_idx: Option<usize>,
        _matched_sub_idx: Option<usize>,
    ) -> Result<(), String> {
        Err("Native AirPlay is only supported on macOS.".into())
    }

    pub fn prepare_session_url(_url: &str, _start_position_ms: i64) -> Result<(), String> {
        Err("Native AirPlay is only supported on macOS.".into())
    }

    pub fn prepare_session_inner(
        _file_path: &str,
        _start_position_ms: i64,
        _matched_audio_idx: Option<usize>,
        _matched_sub_idx: Option<usize>,
    ) -> Result<(), String> {
        Err("Native AirPlay is only supported on macOS.".into())
    }

    pub fn mount_route_picker(
        _app: &tauri::AppHandle,
        _x: f64,
        _y: f64,
        _width: f64,
        _height: f64,
    ) -> Result<(), String> {
        Err("Native AirPlay is only supported on macOS.".into())
    }

    pub fn unmount_route_picker(_app: &tauri::AppHandle) -> Result<(), String> {
        Ok(())
    }

    pub fn play() -> Result<(), String> {
        Err("Native AirPlay is only supported on macOS.".into())
    }

    pub fn pause() -> Result<(), String> {
        Err("Native AirPlay is only supported on macOS.".into())
    }

    pub fn seek(_position_ms: i64) -> Result<(), String> {
        Err("Native AirPlay is only supported on macOS.".into())
    }

    pub fn get_status() -> AirPlayNativeStatus {
        AirPlayNativeStatus {
            is_active: false,
            is_external_active: false,
            is_playing: false,
            position_ms: 0,
            duration_ms: 0,
            error: None,
        }
    }

    pub fn teardown() -> Option<i64> {
        None
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::implementation::*;

    #[test]
    fn test_airplay_disposable_media_preflight_and_lifecycle() {
        let disposable = "/tmp/test_airplay_disposable.mp4";
        assert!(
            std::path::Path::new(disposable).exists(),
            "Disposable test media must exist at /tmp/test_airplay_disposable.mp4"
        );

        // 1. Preflight check: standalone MP4 container, H.264/AAC
        let preflight_res = preflight(disposable, None, None, None, None, false);
        assert!(
            preflight_res.is_playable,
            "Disposable MP4 must be playable by AVFoundation"
        );
        assert!(preflight_res.audio_compatible, "Audio must be compatible");
        assert!(
            preflight_res.subtitle_compatible,
            "Subtitles-off must be compatible"
        );
        assert!(preflight_res.rejection_reason.is_none());

        // 2. Prepare session at 500ms
        let prep = prepare_session_inner(disposable, 500, preflight_res.matched_audio_idx, None);
        assert!(
            prep.is_ok(),
            "prepare_session_inner should succeed: {:?}",
            prep.err()
        );

        // 3. Status immediately after prepare: session active, AVPlayer muted, not external yet
        let status = get_status();
        assert!(status.is_active, "AirPlay session must be active");
        assert!(
            !status.is_external_active,
            "External playback must not be active locally without route selection"
        );
        assert!(
            !status.is_playing,
            "Player must not be playing immediately after prepare"
        );

        // 4. Play control routing
        let play_res = play();
        assert!(play_res.is_ok(), "play() should succeed on active session");

        // 5. Pause control routing
        let pause_res = pause();
        assert!(
            pause_res.is_ok(),
            "pause() should succeed on active session"
        );

        // 6. Seek control routing
        let seek_res = seek(1200);
        assert!(seek_res.is_ok(), "seek() should succeed on active session");

        // 7. Teardown session and verify recovered position
        let end_pos = teardown();
        assert!(end_pos.is_some(), "Teardown must return valid position");
        assert!(
            end_pos.unwrap() >= 0,
            "Recovered position must be non-negative"
        );

        // 8. Verified teardown cleanup
        let after_status = get_status();
        assert!(
            !after_status.is_active,
            "Session must be inactive after teardown"
        );
    }

    #[test]
    fn test_airplay_multitrack_track_fidelity() {
        let multitrack = "/tmp/test_airplay_multitrack.mp4";
        assert!(
            std::path::Path::new(multitrack).exists(),
            "Multitrack test media must exist at /tmp/test_airplay_multitrack.mp4"
        );

        // 1. Preflight matching English track
        let pre_eng = preflight(
            multitrack,
            Some("English Main"),
            Some("eng"),
            None,
            None,
            false,
        );
        assert!(pre_eng.is_playable);
        assert!(pre_eng.audio_compatible);
        assert_eq!(pre_eng.matched_audio_idx, Some(0));

        // 2. Preflight matching Japanese Commentary track
        let pre_jpn = preflight(
            multitrack,
            Some("Japanese Commentary"),
            Some("jpn"),
            None,
            None,
            false,
        );
        assert!(pre_jpn.is_playable);
        assert!(pre_jpn.audio_compatible);
        assert_eq!(pre_jpn.matched_audio_idx, Some(1));

        // 3. Preflight with non-existent track must reject unambiguously
        let pre_none = preflight(
            multitrack,
            Some("French Stereo"),
            Some("fra"),
            None,
            None,
            false,
        );
        assert!(!pre_none.audio_compatible);
        assert!(pre_none.rejection_reason.is_some());

        // 4. Prepare session with Japanese Commentary (index 1) and verify applied on AVPlayerItem
        let prep = prepare_session_inner(multitrack, 0, Some(1), None);
        assert!(
            prep.is_ok(),
            "Track application and AVPlayerItem verification succeeded"
        );

        let end_pos = teardown();
        assert!(end_pos.is_some());
    }

    #[test]
    fn test_airplay_incompatible_container_rejection() {
        let incompatible = "/tmp/test_airplay_incompatible.mkv";
        assert!(
            std::path::Path::new(incompatible).exists(),
            "Incompatible test media must exist at /tmp/test_airplay_incompatible.mkv"
        );

        // AVFoundation does not support MKV containers natively
        let pre = preflight(incompatible, None, None, None, None, false);
        assert!(!pre.is_playable);
        assert!(pre.rejection_reason.is_some());
        let reason = pre.rejection_reason.unwrap();
        assert!(
            reason.contains("Apple-supported") || reason.contains("not natively playable"),
            "Reason must explain container incompatibility: {reason}"
        );

        // Attempting to prepare session on MKV must return error
        let prep = prepare_session_inner(incompatible, 0, None, None);
        assert!(prep.is_err(), "prepare_session on MKV must fail");
    }

    #[test]
    fn test_airplay_external_subtitles_rejection() {
        let disposable = "/tmp/test_airplay_disposable.mp4";
        let pre = preflight(
            disposable,
            None,
            None,
            Some("external.srt"),
            Some("eng"),
            true,
        );
        assert!(!pre.subtitle_compatible);
        assert!(!pre.is_playable);
        assert!(pre
            .rejection_reason
            .unwrap()
            .contains("External subtitle files"));
    }

    #[test]
    fn test_airplay_teardown_idempotent() {
        // Teardown when no session is active should return None and not crash
        assert_eq!(teardown(), None);
        assert_eq!(teardown(), None);
    }
}
