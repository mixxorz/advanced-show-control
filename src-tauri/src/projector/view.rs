use serde::Serialize;

use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity};
use crate::cue_lists::CueList;
use crate::scenes::SceneConfig;
use crate::settings::AppSettings;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SceneSummary {
    pub index: i32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSummary {
    pub group: i32,
    pub channel: i32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppLogEntry {
    pub id: u64,
    pub timestamp: String,
    pub severity: LogSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LogSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AppConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AppFadeState {
    #[default]
    Idle,
    Running,
    Blocked,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppViewState {
    pub connection: AppConnectionState,
    pub discovered_lv1_systems: Vec<DiscoveredLv1System>,
    pub connected_lv1_identity: Option<Lv1SystemIdentity>,
    pub current_scene: Option<SceneSummary>,
    pub scenes: Vec<SceneSummary>,
    pub scene_count: usize,
    pub channel_count: usize,
    pub channels: Vec<ChannelSummary>,
    pub fade_state: AppFadeState,
    pub lockout: bool,
    pub scene_configs: Vec<SceneConfig>,
    pub scene_settings_clipboard_available: bool,
    pub cue_lists: Vec<CueList>,
    pub active_cue_list_id: Option<String>,
    pub cued_cue_entry_id: Option<String>,
    pub last_cue_recall_status: Option<String>,
    pub settings: AppSettings,
    pub selected_scene_internal_id: Option<String>,
    pub show_file_name: String,
    pub show_file_path: Option<String>,
    pub show_file_dirty: bool,
    pub show_file_last_saved_at: Option<String>,
    pub logs: Vec<AppLogEntry>,
    pub last_event_at: Option<String>,
    pub state_version: u64,
}

/// @cc [owner:mixxorz,label:architecture] pre-snapshot-placeholder
/// This value MUST be used only before the first accepted projector snapshot or in isolated tests;
/// it MUST NOT overwrite an accepted snapshot or be treated as evidence of backend disconnection.
impl Default for AppViewState {
    fn default() -> Self {
        Self {
            connection: AppConnectionState::Disconnected,
            discovered_lv1_systems: Vec::new(),
            connected_lv1_identity: None,
            current_scene: None,
            scenes: Vec::new(),
            scene_count: 0,
            channel_count: 0,
            channels: Vec::new(),
            fade_state: AppFadeState::Idle,
            lockout: false,
            scene_configs: Vec::new(),
            scene_settings_clipboard_available: false,
            cue_lists: Vec::new(),
            active_cue_list_id: None,
            cued_cue_entry_id: None,
            last_cue_recall_status: None,
            settings: AppSettings::default(),
            selected_scene_internal_id: None,
            show_file_name: "Untitled Session".to_string(),
            show_file_path: None,
            show_file_dirty: false,
            show_file_last_saved_at: None,
            logs: Vec::new(),
            last_event_at: None,
            state_version: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_view_is_only_a_disconnected_pre_snapshot_placeholder() {
        let view = AppViewState::default();

        assert_eq!(view.connection, AppConnectionState::Disconnected);
        assert_eq!(view.fade_state, AppFadeState::Idle);
        assert_eq!(view.show_file_name, "Untitled Session");
        assert_eq!(view.state_version, 0);
        assert!(view.scenes.is_empty());
        assert!(view.scene_configs.is_empty());
        assert_eq!(view.settings, AppSettings::default());
    }
}
