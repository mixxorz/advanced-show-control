use std::path::PathBuf;

use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};

use crate::scenes::SceneConfig;

#[derive(Debug, Clone, PartialEq)]
pub enum ShowEvent {
    StateChanged {
        reason: ShowProjectionReason,
        state: ShowProjectionState,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowProjectionReason {
    ShowState,
    ConnectionMetadata,
    FileMetadata,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShowProjectionState {
    pub lockout: bool,
    pub scene_configs: Vec<SceneConfig>,
    pub cued_scene_internal_id: Option<String>,
    pub selected_scene_internal_id: Option<String>,
    pub show_file_path: Option<PathBuf>,
    pub show_file_name: String,
    pub show_file_dirty: bool,
    pub show_file_last_saved_at: Option<String>,
    pub discovered_lv1_systems: Vec<DiscoveredLv1System>,
    pub connected_lv1_identity: Option<Lv1SystemIdentity>,
    pub pending_lv1_identity: Option<Lv1SystemIdentity>,
    pub reconnect: ReconnectState,
    pub last_event_at: Option<String>,
}
