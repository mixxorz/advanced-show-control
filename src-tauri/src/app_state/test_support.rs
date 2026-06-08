use advanced_show_control::lv1::types::{
    ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry,
};
use advanced_show_control::show::types::SceneScopeToggles;

use super::view::{ChannelConfig, ChannelRef, SceneConfig};

pub(super) fn connected_snapshot() -> Lv1StateSnapshot {
    Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: None,
        scene_list: Vec::new(),
        channels: Vec::new(),
    }
}

pub(super) fn connected_state_with_scene_and_channel() -> Lv1StateSnapshot {
    Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: None,
        scene_list: vec![SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        }],
        channels: vec![ChannelInfo {
            group: 0,
            channel: 2,
            name: "Lead".to_string(),
            gain_db: -8.0,
            muted: false,
        }],
    }
}

pub(super) fn scene_config(
    scene_index: i32,
    scene_name: &str,
    channel_configs: Vec<ChannelConfig>,
    scoped_channels: Vec<ChannelRef>,
) -> SceneConfig {
    SceneConfig {
        scene_id: format!("{scene_index}::{scene_name}"),
        scene_index,
        scene_name: scene_name.to_string(),
        duration_ms: 0,
        channel_configs,
        scoped_channels,
        scope_toggles: SceneScopeToggles::default(),
    }
}
