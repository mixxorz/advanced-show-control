use super::shell::ShellState;
use super::test_support::connected_state_with_scene_and_channel;
use advanced_show_control::lv1::types::{
    ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry,
};

#[tokio::test]
async fn store_scene_config_snapshots_all_current_channels_and_scopes_first_store() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;

    let snapshot = state
        .store_scene_config("1::Intro".to_string())
        .await
        .unwrap();

    let config = &snapshot.scene_configs[0];
    assert_eq!(config.channel_configs.len(), 1);
    assert_eq!(config.channel_configs[0].group, 0);
    assert_eq!(config.channel_configs[0].channel, 2);
    assert_eq!(config.channel_configs[0].fader_db, Some(-8.0));
    assert_eq!(config.scoped_channels.len(), 1);
    assert_eq!(config.scoped_channels[0].group, 0);
    assert_eq!(config.scoped_channels[0].channel, 2);
    assert!(snapshot.show_file_dirty);
}

#[tokio::test]
async fn store_scene_config_rejects_missing_scene_id() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;

    let err = state
        .store_scene_config("2::Verse".to_string())
        .await
        .unwrap_err();

    assert_eq!(err, "Scene config not found");
}

#[tokio::test]
async fn store_scene_config_rejects_empty_lv1_channel_list() {
    let state = ShellState::default();
    state
        .begin_connection(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }],
            channels: Vec::new(),
        })
        .await;

    let err = state
        .store_scene_config("1::Intro".to_string())
        .await
        .unwrap_err();

    assert_eq!(err, "LV1 channel list is empty");
}

#[tokio::test]
async fn set_scene_duration_ms_updates_duration_and_marks_dirty() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;

    let zero = state
        .set_scene_duration_ms("1::Intro".to_string(), 0)
        .await
        .unwrap();
    assert_eq!(zero.scene_configs[0].duration_ms, 0);
    assert!(zero.show_file_dirty);

    let snapshot = state
        .set_scene_duration_ms("1::Intro".to_string(), 8_000)
        .await
        .unwrap();

    assert_eq!(snapshot.scene_configs[0].duration_ms, 8_000);
    assert!(snapshot.show_file_dirty);
}

#[tokio::test]
async fn set_scene_scope_faders_enabled_updates_toggle_and_marks_dirty() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;
    state
        .store_scene_config("1::Intro".to_string())
        .await
        .unwrap();

    let snapshot = state
        .set_scene_scope_faders_enabled("1::Intro".to_string(), false)
        .await
        .unwrap();

    assert!(!snapshot.scene_configs[0].scope_toggles.faders);
    assert!(snapshot.show_file_dirty);
}

#[tokio::test]
async fn set_scene_scope_pan_enabled_updates_toggle_and_marks_dirty() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;
    state
        .store_scene_config("1::Intro".to_string())
        .await
        .unwrap();

    let snapshot = state
        .set_scene_scope_pan_enabled("1::Intro".to_string(), true)
        .await
        .unwrap();

    assert!(snapshot.scene_configs[0].scope_toggles.pan);
    assert!(snapshot.show_file_dirty);
}

#[tokio::test]
async fn store_scene_config_preserves_existing_scope_on_later_store() {
    let state = ShellState::default();

    state
        .begin_connection(connected_state_with_three_channels())
        .await;
    state
        .store_scene_config("1::Intro".to_string())
        .await
        .unwrap();

    state
        .begin_connection(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }],
            channels: vec![
                ChannelInfo {
                    group: 0,
                    channel: 4,
                    name: "Bass".to_string(),
                    gain_db: -10.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                },
                ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Lead".to_string(),
                    gain_db: -8.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                },
            ],
        })
        .await;

    let snapshot = state
        .store_scene_config("1::Intro".to_string())
        .await
        .unwrap();

    let config = &snapshot.scene_configs[0];
    assert_eq!(config.channel_configs.len(), 2);
    assert_eq!(config.channel_configs[0].channel, 4);
    assert_eq!(config.channel_configs[1].channel, 2);
    assert_eq!(config.scoped_channels.len(), 2);
    assert_eq!(config.scoped_channels[0].channel, 2);
    assert_eq!(config.scoped_channels[1].channel, 4);
}

#[tokio::test]
async fn set_channel_scoped_toggles_single_channel_scope_and_marks_dirty() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;
    state
        .store_scene_config("1::Intro".to_string())
        .await
        .unwrap();

    let cleared = state
        .set_channel_scoped("1::Intro".to_string(), 0, 2, false)
        .await
        .unwrap();
    assert!(cleared.scene_configs[0].scoped_channels.is_empty());
    assert!(cleared.show_file_dirty);

    let restored = state
        .set_channel_scoped("1::Intro".to_string(), 0, 2, true)
        .await
        .unwrap();
    assert_eq!(restored.scene_configs[0].scoped_channels.len(), 1);
    assert_eq!(restored.scene_configs[0].scoped_channels[0].group, 0);
    assert_eq!(restored.scene_configs[0].scoped_channels[0].channel, 2);
}

#[tokio::test]
async fn set_channel_scoped_noop_keeps_clean_show_file_clean() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;
    state
        .store_scene_config("1::Intro".to_string())
        .await
        .unwrap();
    set_show_file_clean(&state).await;

    let snapshot = state
        .set_channel_scoped("1::Intro".to_string(), 0, 2, true)
        .await
        .unwrap();

    assert!(!snapshot.show_file_dirty);
    assert_eq!(snapshot.scene_configs[0].scoped_channels.len(), 1);
}

#[tokio::test]
async fn set_all_channels_scoped_sets_and_clears_scope() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_two_channels())
        .await;
    state
        .store_scene_config("1::Intro".to_string())
        .await
        .unwrap();

    let cleared = state
        .set_all_channels_scoped("1::Intro".to_string(), false)
        .await
        .unwrap();
    assert!(cleared.scene_configs[0].scoped_channels.is_empty());
    assert!(cleared.show_file_dirty);

    let restored = state
        .set_all_channels_scoped("1::Intro".to_string(), true)
        .await
        .unwrap();
    assert_eq!(restored.scene_configs[0].scoped_channels.len(), 2);
    assert_eq!(restored.scene_configs[0].scoped_channels[0].group, 0);
    assert_eq!(restored.scene_configs[0].scoped_channels[0].channel, 2);
    assert_eq!(restored.scene_configs[0].scoped_channels[1].channel, 3);
}

#[tokio::test]
async fn set_all_channels_scoped_noop_keeps_clean_show_file_clean() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_two_channels())
        .await;
    state
        .store_scene_config("1::Intro".to_string())
        .await
        .unwrap();
    set_show_file_clean(&state).await;

    let snapshot = state
        .set_all_channels_scoped("1::Intro".to_string(), true)
        .await
        .unwrap();

    assert!(!snapshot.show_file_dirty);
    assert_eq!(snapshot.scene_configs[0].scoped_channels.len(), 2);
    assert_eq!(snapshot.scene_configs[0].scoped_channels[0].channel, 2);
    assert_eq!(snapshot.scene_configs[0].scoped_channels[1].channel, 3);
}

#[tokio::test]
async fn set_all_channels_scoped_reordered_scopes_is_noop_and_preserves_order() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_two_channels())
        .await;
    state
        .store_scene_config("1::Intro".to_string())
        .await
        .unwrap();

    {
        let mut inner = state.inner.lock().await;
        inner.scene_configs[0].scoped_channels = vec![
            super::view::ChannelRef {
                group: 0,
                channel: 3,
            },
            super::view::ChannelRef {
                group: 0,
                channel: 2,
            },
        ];
        inner.show_file_dirty = false;
    }

    let snapshot = state
        .set_all_channels_scoped("1::Intro".to_string(), true)
        .await
        .unwrap();

    assert!(!snapshot.show_file_dirty);
    assert_eq!(snapshot.scene_configs[0].scoped_channels[0].channel, 3);
    assert_eq!(snapshot.scene_configs[0].scoped_channels[1].channel, 2);
}

fn connected_state_with_two_channels() -> Lv1StateSnapshot {
    Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: None,
        scene_list: vec![SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        }],
        channels: vec![
            ChannelInfo {
                group: 0,
                channel: 2,
                name: "Lead".to_string(),
                gain_db: -8.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            },
            ChannelInfo {
                group: 0,
                channel: 3,
                name: "Harmony".to_string(),
                gain_db: -12.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            },
        ],
    }
}

fn connected_state_with_three_channels() -> Lv1StateSnapshot {
    Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: None,
        scene_list: vec![SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        }],
        channels: vec![
            ChannelInfo {
                group: 0,
                channel: 2,
                name: "Lead".to_string(),
                gain_db: -8.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            },
            ChannelInfo {
                group: 0,
                channel: 3,
                name: "Pad".to_string(),
                gain_db: -12.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            },
            ChannelInfo {
                group: 0,
                channel: 4,
                name: "Bass".to_string(),
                gain_db: -10.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            },
        ],
    }
}

async fn set_show_file_clean(state: &ShellState) {
    let mut inner = state.inner.lock().await;
    inner.show_file_dirty = false;
}
