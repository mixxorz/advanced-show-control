use advanced_show_control::lv1::types::{
    ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry,
};
use advanced_show_control::show::types::{SceneScopeToggles, scene_id};

use super::shell::ShellState;
use super::view::AppViewState;
use super::view::{ChannelConfig, ChannelRef, SceneConfig};

pub(super) async fn begin_test_connection(
    state: &ShellState,
    snapshot: Lv1StateSnapshot,
) -> AppViewState {
    let (generation, _) = state.begin_connecting().await;
    state
        .begin_connection(generation, snapshot)
        .await
        .expect("test connection should apply to current generation")
}

pub(super) async fn set_pending_lv1_identity(
    state: &ShellState,
    identity: Option<crate::connection_state::Lv1SystemIdentity>,
) -> AppViewState {
    let (generation, _) = state.begin_connecting().await;
    state
        .set_pending_lv1_identity(generation, identity)
        .await
        .expect("test pending identity should apply to current generation")
}

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
            pan: None,
            balance: None,
            width: None,
            pan_mode: None,
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
        scene_id: scene_id(scene_index, scene_name),
        scene_index,
        scene_name: scene_name.to_string(),
        duration_ms: 0,
        channel_configs,
        scoped_channels,
        scope_toggles: SceneScopeToggles::default(),
    }
}
