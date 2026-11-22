use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActiveOutputKind {
    #[default]
    LocalVlc,
    AirPlay,
    GoogleCast,
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
    pub is_external: bool,
    pub error: Option<String>,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub is_seekable: bool,
    pub supports_return_to_local: bool,
    pub has_recovery_snapshot: bool,
}

/// Coordinates output routing between local VLC, native macOS AirPlay, and Google Cast.
#[derive(Debug, Default)]
pub struct OutputCoordinator {
    pub active_output: ActiveOutputKind,
    pub target_id: Option<String>,
    pub target_name: Option<String>,
    pub state: String,
    pub error: Option<String>,
    pub recovery_snapshot: Option<PlaybackRecoverySnapshot>,
}

impl OutputCoordinator {
    pub fn new() -> Self {
        Self {
            active_output: ActiveOutputKind::LocalVlc,
            target_id: None,
            target_name: None,
            state: "local".to_string(),
            error: None,
            recovery_snapshot: None,
        }
    }

    pub fn reset_to_local(&mut self) {
        self.active_output = ActiveOutputKind::LocalVlc;
        self.target_id = None;
        self.target_name = None;
        self.state = "local".to_string();
        self.error = None;
        // Notice: recovery_snapshot is intentionally retained until restored or replaced
    }

    pub fn set_airplay_preparing(
        &mut self,
        target_name: String,
        snapshot: PlaybackRecoverySnapshot,
    ) {
        self.active_output = ActiveOutputKind::AirPlay;
        self.target_id = Some("airplay:appletv".to_string());
        self.target_name = Some(target_name);
        self.state = "ready_for_routing".to_string();
        self.error = None;
        self.recovery_snapshot = Some(snapshot);
    }

    pub fn set_airplay_active(&mut self) {
        self.active_output = ActiveOutputKind::AirPlay;
        self.state = "external_active".to_string();
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
        self.state = "request_issued".to_string();
        self.error = None;
        self.recovery_snapshot = Some(snapshot);
    }

    pub fn set_error(&mut self, err: String) {
        self.state = format!("error:{}", err);
        self.error = Some(err);
    }
}
