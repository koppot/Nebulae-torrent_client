use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActiveOutputKind {
    #[default]
    LocalVlc,
    AirPlay,
    GoogleCast,
}

/// The 6 explicit receiver-native route states, plus local playback baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RouteState {
    #[default]
    Local,
    PreparingNativeMedia,
    ReadyForPicker,
    AwaitingExternal,
    ExternalActive,
    PlaybackPaused,
    Disconnected,
}

impl RouteState {
    pub fn as_str(&self) -> &'static str {
        match self {
            RouteState::Local => "local",
            RouteState::PreparingNativeMedia => "preparing_native_media",
            RouteState::ReadyForPicker => "ready_for_picker",
            RouteState::AwaitingExternal => "awaiting_external",
            RouteState::ExternalActive => "external_active",
            RouteState::PlaybackPaused => "playback_paused",
            RouteState::Disconnected => "disconnected",
        }
    }

    pub fn display_label(&self) -> &'static str {
        match self {
            RouteState::Local => "Local playback",
            RouteState::PreparingNativeMedia => "Preparing native media",
            RouteState::ReadyForPicker => "Ready to select an AirPlay receiver",
            RouteState::AwaitingExternal => "Awaiting external playback",
            RouteState::ExternalActive => "External playback active",
            RouteState::PlaybackPaused => "Playback paused or waiting",
            RouteState::Disconnected => "Disconnected or failed",
        }
    }
}

/// Recovery snapshot taken when transitioning from local playback to remote output
/// or when remote output disconnects, enabling deterministic return to local player.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaybackRecoverySnapshot {
    pub media_path: String,
    pub position_ms: i64,
    pub was_paused: bool,
    pub audio_track_id: Option<i32>,
    pub audio_track_name: Option<String>,
    pub audio_track_idx: Option<usize>,
    pub subtitle_track_id: Option<i32>,
    pub subtitle_track_name: Option<String>,
    pub subtitle_track_idx: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedOutputStatus {
    pub active_output: ActiveOutputKind,
    pub target_id: Option<String>,
    pub target_name: Option<String>,
    pub state: String,
    pub route_state: RouteState,
    pub route_state_label: String,
    pub is_external: bool,
    pub error: Option<String>,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub is_seekable: bool,
    pub supports_return_to_local: bool,
    pub has_recovery_snapshot: bool,
    pub latest_remote_position_ms: Option<i64>,
}

/// Coordinates output routing between local VLC, native macOS AirPlay, and Google Cast.
#[derive(Debug, Default)]
pub struct OutputCoordinator {
    pub active_output: ActiveOutputKind,
    pub target_id: Option<String>,
    pub target_name: Option<String>,
    pub route_state: RouteState,
    pub state: String,
    pub error: Option<String>,
    pub recovery_snapshot: Option<PlaybackRecoverySnapshot>,
    pub latest_remote_position_ms: Option<i64>,
}

impl OutputCoordinator {
    pub fn new() -> Self {
        Self {
            active_output: ActiveOutputKind::LocalVlc,
            target_id: None,
            target_name: None,
            route_state: RouteState::Local,
            state: "local".to_string(),
            error: None,
            recovery_snapshot: None,
            latest_remote_position_ms: None,
        }
    }

    pub fn reset_to_local(&mut self) {
        self.active_output = ActiveOutputKind::LocalVlc;
        self.target_id = None;
        self.target_name = None;
        self.route_state = RouteState::Local;
        self.state = "local".to_string();
        self.error = None;
        self.latest_remote_position_ms = None;
        // Notice: recovery_snapshot is intentionally retained until restored or replaced
    }

    pub fn set_airplay_preparing(
        &mut self,
        target_name: String,
        snapshot: PlaybackRecoverySnapshot,
    ) {
        self.active_output = ActiveOutputKind::AirPlay;
        self.target_id = Some("airplay:default".to_string());
        self.target_name = Some(target_name);
        self.route_state = RouteState::ReadyForPicker;
        self.state = "ready_for_picker".to_string();
        self.error = None;
        self.latest_remote_position_ms = Some(snapshot.position_ms);
        self.recovery_snapshot = Some(snapshot);
    }

    pub fn set_airplay_awaiting(&mut self) {
        self.active_output = ActiveOutputKind::AirPlay;
        self.route_state = RouteState::AwaitingExternal;
        self.state = "awaiting_external".to_string();
        self.error = None;
    }

    pub fn set_airplay_active(&mut self) {
        self.active_output = ActiveOutputKind::AirPlay;
        self.route_state = RouteState::ExternalActive;
        self.state = "external_active".to_string();
        self.error = None;
    }

    pub fn set_airplay_paused(&mut self) {
        self.active_output = ActiveOutputKind::AirPlay;
        self.route_state = RouteState::PlaybackPaused;
        self.state = "playback_paused".to_string();
        self.error = None;
    }

    pub fn set_cast_starting(
        &mut self,
        target_id: String,
        target_name: String,
        snapshot: PlaybackRecoverySnapshot,
    ) {
        self.active_output = ActiveOutputKind::GoogleCast;
        self.target_id = Some(target_id);
        self.target_name = Some(target_name);
        self.route_state = RouteState::AwaitingExternal;
        self.state = "request_issued".to_string();
        self.error = None;
        self.latest_remote_position_ms = Some(snapshot.position_ms);
        self.recovery_snapshot = Some(snapshot);
    }

    pub fn set_disconnected(&mut self) {
        self.route_state = RouteState::Disconnected;
        self.state = "disconnected".to_string();
    }

    pub fn set_error(&mut self, err: String) {
        self.route_state = RouteState::Disconnected;
        self.state = format!("error:{}", err);
        self.error = Some(err);
    }

    pub fn update_remote_position(&mut self, pos_ms: i64) {
        if pos_ms > 0 {
            self.latest_remote_position_ms = Some(pos_ms);
        }
    }
}
