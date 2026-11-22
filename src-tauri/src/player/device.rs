//! Device discovery and remote renderer playback management.
//!
//! Grounded in libVLC 3.0 renderer discovery and player assignment APIs.
//! Thread-safe handoff from native libVLC event callbacks to managed state.

use crate::player::engine::{
    VlcEvent, VlcEventCallback, VlcLib, LIBVLC_RENDERER_CAN_AUDIO, LIBVLC_RENDERER_CAN_VIDEO,
    LIBVLC_RENDERER_DISCOVERER_ITEM_ADDED, LIBVLC_RENDERER_DISCOVERER_ITEM_DELETED,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::{c_void, CStr, CString};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Frontend-safe representation of a discovered remote renderer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredDevice {
    pub id: String,
    pub name: String,
    pub renderer_type: String,
    pub icon_uri: Option<String>,
    pub can_audio: bool,
    pub can_video: bool,
    #[serde(skip)]
    pub native_item: usize, // held libvlc_renderer_item_t*
}

/// Truthful connection and playback state of remote casting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CastStatus {
    Idle,
    Discovering,
    Connecting,
    RequestIssued,
    Error(String),
    Disconnected,
}

/// Outcome of preflight compatibility check before casting handoff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastPreflightResult {
    pub device_id: String,
    pub device_name: String,
    pub is_supported_media: bool,
    pub video_supported: bool,
    pub audio_supported: bool,
    pub subtitle_supported: bool,
    pub selected_subtitle: Option<String>,
    pub selected_audio: Option<String>,
    #[serde(default)]
    pub matched_audio_idx: Option<usize>,
    #[serde(default)]
    pub matched_subtitle_idx: Option<usize>,
    pub rejection_reason: Option<String>,
    pub details: String,
}

/// Thread-safe registry holding held native renderer items and active devices.
pub struct DeviceRegistry {
    pub lib: Arc<VlcLib>,
    pub devices: HashMap<String, DiscoveredDevice>,
}

impl DeviceRegistry {
    pub fn new(lib: Arc<VlcLib>) -> Self {
        Self {
            lib,
            devices: HashMap::new(),
        }
    }

    pub fn handle_event(&mut self, event: &VlcEvent) {
        unsafe {
            if event.r#type == LIBVLC_RENDERER_DISCOVERER_ITEM_ADDED {
                let item = event.u.item_added.item;
                if item.is_null() {
                    return;
                }
                let already_present = self
                    .devices
                    .values()
                    .any(|d| d.native_item == item as usize);
                if !already_present {
                    if let Some(hold) = self.lib.libvlc_renderer_item_hold {
                        hold(item);
                    }
                    let name = self
                        .lib
                        .libvlc_renderer_item_name
                        .map(|f| {
                            let p = f(item);
                            if !p.is_null() {
                                CStr::from_ptr(p).to_string_lossy().into_owned()
                            } else {
                                "Unknown Device".to_string()
                            }
                        })
                        .unwrap_or_else(|| "Unknown Device".to_string());
                    let renderer_type = self
                        .lib
                        .libvlc_renderer_item_type
                        .map(|f| {
                            let p = f(item);
                            if !p.is_null() {
                                CStr::from_ptr(p).to_string_lossy().into_owned()
                            } else {
                                "chromecast".to_string()
                            }
                        })
                        .unwrap_or_else(|| "chromecast".to_string());
                    let icon_uri = self.lib.libvlc_renderer_item_icon_uri.and_then(|f| {
                        let p = f(item);
                        if !p.is_null() {
                            Some(CStr::from_ptr(p).to_string_lossy().into_owned())
                        } else {
                            None
                        }
                    });
                    let flags = self
                        .lib
                        .libvlc_renderer_item_flags
                        .map(|f| f(item))
                        .unwrap_or(0);
                    let can_audio = (flags & LIBVLC_RENDERER_CAN_AUDIO) != 0;
                    let can_video = (flags & LIBVLC_RENDERER_CAN_VIDEO) != 0;

                    let id = format!("device-{}", uuid::Uuid::new_v4());
                    self.devices.insert(
                        id.clone(),
                        DiscoveredDevice {
                            id,
                            name,
                            renderer_type,
                            icon_uri,
                            can_audio,
                            can_video,
                            native_item: item as usize,
                        },
                    );
                }
            } else if event.r#type == LIBVLC_RENDERER_DISCOVERER_ITEM_DELETED {
                let item = event.u.item_deleted.item;
                if item.is_null() {
                    return;
                }
                let to_remove: Vec<String> = self
                    .devices
                    .iter()
                    .filter(|(_, d)| d.native_item == item as usize)
                    .map(|(id, _)| id.clone())
                    .collect();
                for id in to_remove {
                    if let Some(removed) = self.devices.remove(&id) {
                        if let Some(release) = self.lib.libvlc_renderer_item_release {
                            release(removed.native_item as *mut c_void);
                        }
                    }
                }
            }
        }
    }
}

impl Drop for DeviceRegistry {
    fn drop(&mut self) {
        if let Some(release) = self.lib.libvlc_renderer_item_release {
            for (_, d) in self.devices.drain() {
                if d.native_item != 0 {
                    unsafe {
                        release(d.native_item as *mut c_void);
                    }
                }
            }
        }
    }
}

/// C callback trampoline invoking the locked registry.
unsafe extern "C" fn discovery_event_cb(event: *const VlcEvent, data: *mut c_void) {
    if event.is_null() || data.is_null() {
        return;
    }
    let registry_arc = &*(data as *const Arc<Mutex<DeviceRegistry>>);
    let event = &*event;
    if let Ok(mut reg) = registry_arc.lock() {
        reg.handle_event(event);
    }
}

/// Bounded discovery manager running native libVLC renderer discoverers.
pub struct DiscoveryManager {
    lib: Arc<VlcLib>,
    pub registry: Arc<Mutex<DeviceRegistry>>,
    active_discoverers: Vec<(usize, usize)>, // (em, rd)
    callback_user_data: *mut Arc<Mutex<DeviceRegistry>>,
    started_at: Option<Instant>,
}

unsafe impl Send for DiscoveryManager {}
unsafe impl Sync for DiscoveryManager {}

impl DiscoveryManager {
    pub fn new(lib: Arc<VlcLib>) -> Self {
        let registry = Arc::new(Mutex::new(DeviceRegistry::new(Arc::clone(&lib))));
        Self {
            lib,
            registry,
            active_discoverers: Vec::new(),
            callback_user_data: std::ptr::null_mut(),
            started_at: None,
        }
    }

    pub fn is_discovering(&self) -> bool {
        if let Some(started) = self.started_at {
            // Auto-expire after 25 seconds
            started.elapsed().as_secs() < 25 && !self.active_discoverers.is_empty()
        } else {
            false
        }
    }

    pub fn check_expiration(&mut self) {
        if let Some(started) = self.started_at {
            if started.elapsed().as_secs() >= 25 {
                self.stop();
            }
        }
    }

    pub fn start(&mut self, instance: usize) -> Result<usize, String> {
        self.stop();

        if instance == 0 {
            return Err("libvlc instance not initialized".into());
        }

        let mut service_names = Vec::new();
        if let Some(list_get) = self.lib.libvlc_renderer_discoverer_list_get {
            let mut services: *mut *mut crate::player::engine::VlcRdDescription =
                std::ptr::null_mut();
            let count = unsafe { list_get(instance as *mut c_void, &mut services) };
            if count > 0 && !services.is_null() {
                for i in 0..count {
                    unsafe {
                        let entry_ptr = *services.add(i);
                        if !entry_ptr.is_null() && !(*entry_ptr).psz_name.is_null() {
                            let s = CStr::from_ptr((*entry_ptr).psz_name)
                                .to_string_lossy()
                                .into_owned();
                            service_names.push(s);
                        }
                    }
                }
                if let Some(list_rel) = self.lib.libvlc_renderer_discoverer_list_release {
                    unsafe {
                        list_rel(services, count);
                    }
                }
            }
        }

        if service_names.is_empty() {
            #[cfg(target_os = "macos")]
            {
                service_names.push("Bonjour_renderer".to_string());
                service_names.push("microdns_renderer".to_string());
            }
            #[cfg(not(target_os = "macos"))]
            {
                service_names.push("microdns_renderer".to_string());
            }
        }

        // Allocate callback context pointer
        let user_data = Box::into_raw(Box::new(Arc::clone(&self.registry)));
        self.callback_user_data = user_data;

        let mut started_count = 0;
        for name in service_names {
            if let Some(new_rd) = self.lib.libvlc_renderer_discoverer_new {
                let c_name = match CString::new(name.as_str()) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let rd = unsafe { new_rd(instance as *mut c_void, c_name.as_ptr()) };
                if rd.is_null() {
                    continue;
                }
                let em = if let Some(get_em) = self.lib.libvlc_renderer_discoverer_event_manager {
                    unsafe { get_em(rd) }
                } else {
                    std::ptr::null_mut()
                };
                if !em.is_null() {
                    if let Some(attach) = self.lib.libvlc_event_attach {
                        unsafe {
                            let cb: VlcEventCallback = discovery_event_cb;
                            attach(
                                em,
                                LIBVLC_RENDERER_DISCOVERER_ITEM_ADDED,
                                cb,
                                user_data as *mut c_void,
                            );
                            attach(
                                em,
                                LIBVLC_RENDERER_DISCOVERER_ITEM_DELETED,
                                cb,
                                user_data as *mut c_void,
                            );
                        }
                    }
                }
                if let Some(start) = self.lib.libvlc_renderer_discoverer_start {
                    let rc = unsafe { start(rd) };
                    if rc == 0 {
                        self.active_discoverers.push((em as usize, rd as usize));
                        started_count += 1;
                    } else {
                        if !em.is_null() {
                            if let Some(detach) = self.lib.libvlc_event_detach {
                                unsafe {
                                    let cb: VlcEventCallback = discovery_event_cb;
                                    detach(
                                        em,
                                        LIBVLC_RENDERER_DISCOVERER_ITEM_ADDED,
                                        cb,
                                        user_data as *mut c_void,
                                    );
                                    detach(
                                        em,
                                        LIBVLC_RENDERER_DISCOVERER_ITEM_DELETED,
                                        cb,
                                        user_data as *mut c_void,
                                    );
                                }
                            }
                        }
                        if let Some(rel) = self.lib.libvlc_renderer_discoverer_release {
                            unsafe {
                                rel(rd);
                            }
                        }
                    }
                }
            }
        }

        self.started_at = Some(Instant::now());
        Ok(started_count)
    }

    pub fn stop(&mut self) {
        if !self.callback_user_data.is_null() {
            for (em, rd) in self.active_discoverers.drain(..) {
                if em != 0 {
                    if let Some(detach) = self.lib.libvlc_event_detach {
                        unsafe {
                            let cb: VlcEventCallback = discovery_event_cb;
                            detach(
                                em as *mut c_void,
                                LIBVLC_RENDERER_DISCOVERER_ITEM_ADDED,
                                cb,
                                self.callback_user_data as *mut c_void,
                            );
                            detach(
                                em as *mut c_void,
                                LIBVLC_RENDERER_DISCOVERER_ITEM_DELETED,
                                cb,
                                self.callback_user_data as *mut c_void,
                            );
                        }
                    }
                }
                if rd != 0 {
                    if let Some(stop) = self.lib.libvlc_renderer_discoverer_stop {
                        unsafe {
                            stop(rd as *mut c_void);
                        }
                    }
                    if let Some(rel) = self.lib.libvlc_renderer_discoverer_release {
                        unsafe {
                            rel(rd as *mut c_void);
                        }
                    }
                }
            }
            unsafe {
                let _ = Box::from_raw(self.callback_user_data);
            }
            self.callback_user_data = std::ptr::null_mut();
        }
        self.started_at = None;
    }

    pub fn list_devices(&self) -> Vec<DiscoveredDevice> {
        if let Ok(reg) = self.registry.lock() {
            reg.devices.values().cloned().collect()
        } else {
            Vec::new()
        }
    }

    pub fn get_device(&self, device_id: &str) -> Option<DiscoveredDevice> {
        if let Ok(reg) = self.registry.lock() {
            reg.devices.get(device_id).cloned()
        } else {
            None
        }
    }
}

impl Drop for DiscoveryManager {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Active remote casting session state attached to the player.
pub struct CastingSessionState {
    pub status: CastStatus,
    pub active_device: Option<DiscoveredDevice>,
    pub preflight: Option<CastPreflightResult>,
    pub last_local_position_ms: i64,
    pub last_media_path: Option<String>,
    pub last_audio_track_id: i32,
    pub last_audio_track_name: Option<String>,
    pub last_audio_track_idx: usize,
    pub last_subtitle_track_id: i32,
    pub last_subtitle_track_name: Option<String>,
    pub last_subtitle_track_idx: usize,
    pub was_paused: bool,
    pub conversion_approved: bool,
}

impl Default for CastingSessionState {
    fn default() -> Self {
        Self {
            status: CastStatus::Idle,
            active_device: None,
            preflight: None,
            last_local_position_ms: 0,
            last_media_path: None,
            last_audio_track_id: -1,
            last_audio_track_name: None,
            last_audio_track_idx: 0,
            last_subtitle_track_id: -1,
            last_subtitle_track_name: None,
            last_subtitle_track_idx: 0,
            was_paused: false,
            conversion_approved: false,
        }
    }
}

impl CastingSessionState {
    pub fn reset(&mut self) {
        self.status = CastStatus::Idle;
        self.active_device = None;
        self.preflight = None;
        self.last_local_position_ms = 0;
        self.last_media_path = None;
        self.last_audio_track_id = -1;
        self.last_audio_track_name = None;
        self.last_audio_track_idx = 0;
        self.last_subtitle_track_id = -1;
        self.last_subtitle_track_name = None;
        self.last_subtitle_track_idx = 0;
        self.was_paused = false;
        self.conversion_approved = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_casting_state_transitions() {
        let mut state = CastingSessionState::default();
        assert_eq!(state.status, CastStatus::Idle);
        assert!(state.active_device.is_none());

        state.status = CastStatus::Connecting;
        assert_eq!(state.status, CastStatus::Connecting);

        state.status = CastStatus::RequestIssued;
        assert_eq!(state.status, CastStatus::RequestIssued);

        state.status = CastStatus::Error("Connection dropped".to_string());
        assert_eq!(
            state.status,
            CastStatus::Error("Connection dropped".to_string())
        );

        state.last_local_position_ms = 45000;
        state.conversion_approved = true;

        state.reset();
        assert_eq!(state.status, CastStatus::Idle);
        assert_eq!(state.last_local_position_ms, 0);
        assert!(!state.conversion_approved);
    }

    #[test]
    fn test_discovered_device_serialization() {
        let dev = DiscoveredDevice {
            id: "device-123".to_string(),
            name: "Chromecast Ultra".to_string(),
            renderer_type: "chromecast".to_string(),
            icon_uri: Some("http://icon".to_string()),
            can_audio: true,
            can_video: true,
            native_item: 0x12345678,
        };
        let val = serde_json::to_value(&dev).unwrap();
        assert_eq!(val["id"], "device-123");
        assert_eq!(val["name"], "Chromecast Ultra");
        assert_eq!(val["renderer_type"], "chromecast");
        assert_eq!(val["can_audio"], true);
        assert_eq!(val["can_video"], true);
        assert!(val.get("native_item").is_none());
    }

    #[test]
    fn test_preflight_result_serialization() {
        let preflight = CastPreflightResult {
            device_id: "dev-1".to_string(),
            device_name: "TV".to_string(),
            is_supported_media: false,
            video_supported: true,
            audio_supported: false,
            subtitle_supported: false,
            selected_subtitle: Some("English".to_string()),
            selected_audio: Some("Japanese".to_string()),
            matched_audio_idx: None,
            matched_subtitle_idx: None,
            rejection_reason: Some(
                "Google Cast cannot stream selected subtitles without video conversion."
                    .to_string(),
            ),
            details: "Direct playback unavailable under receiver-native policy.".to_string(),
        };
        let val = serde_json::to_value(&preflight).unwrap();
        assert_eq!(val["is_supported_media"], false);
        assert_eq!(val["subtitle_supported"], false);
        assert_eq!(val["selected_subtitle"], "English");
        assert!(val["rejection_reason"]
            .as_str()
            .unwrap()
            .contains("subtitles"));
    }
}
