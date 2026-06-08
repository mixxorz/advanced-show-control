use super::shell::ShellState;
use super::test_support::connected_state_with_scene_and_channel;
use super::view::ShowSnapshot;
use crate::show_file::{ShowFile, ShowFileChannelConfig, ShowFileChannelRef, ShowFileSceneConfig};

#[tokio::test]
async fn export_show_file_contains_current_configs() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;
    state
        .store_scene_config("1::Intro".to_string())
        .await
        .unwrap();

    let file = state.export_show_file("saved".to_string()).await;

    assert_eq!(file.schema_version, 1);
    assert!(!file.safety.lockout);
    assert_eq!(file.saved_at, "saved");
    assert_eq!(file.scene_configs[0].scene_index, 1);
    assert_eq!(file.scene_configs[0].duration_ms, 0);
    assert_eq!(
        file.scene_configs[0].channel_configs,
        vec![ShowFileChannelConfig {
            group: 0,
            channel: 2,
            fader_db: Some(-8.0),
        }]
    );
    assert_eq!(
        file.scene_configs[0].scoped_channels,
        vec![ShowFileChannelRef {
            group: 0,
            channel: 2
        }]
    );
}

fn lv1_scene_only_snapshot() -> advanced_show_control::lv1::types::Lv1StateSnapshot {
    advanced_show_control::lv1::types::Lv1StateSnapshot {
        connection: advanced_show_control::lv1::types::ConnectionStatus::Connected,
        scene: None,
        scene_list: vec![advanced_show_control::lv1::types::SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        }],
        channels: Vec::new(),
    }
}

#[tokio::test]
async fn new_show_file_clears_file_state_and_rebuilds_current_lv1_scenes() {
    let state = ShellState::default();
    state.begin_connection(lv1_scene_only_snapshot()).await;
    state
        .show
        .replace_snapshot(ShowSnapshot {
            lockout: true,
            scene_configs: vec![super::view::SceneConfig {
                scene_id: "1::Intro".to_string(),
                scene_index: 1,
                scene_name: "Intro".to_string(),
                duration_ms: 0,
                channel_configs: vec![super::view::ChannelConfig {
                    group: 0,
                    channel: 2,
                    fader_db: Some(-8.0),
                }],
                scoped_channels: vec![super::view::ChannelRef {
                    group: 0,
                    channel: 2,
                }],
            }],
        })
        .await
        .unwrap();

    let snapshot = state.new_show_file().await.unwrap();

    assert_eq!(snapshot.show_file_path, None);
    assert_eq!(snapshot.show_file_last_saved_at, None);
    assert!(!snapshot.show_file_dirty);
    assert!(!snapshot.lockout);
    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].duration_ms, 0);
    assert!(snapshot.scene_configs[0].channel_configs.is_empty());
    assert!(snapshot.scene_configs[0].scoped_channels.is_empty());
    assert_eq!(
        snapshot.logs.last().unwrap().message,
        "New show file created"
    );
}

#[tokio::test]
async fn current_show_file_path_returns_pathbuf() {
    let state = ShellState::default();
    let path = std::path::PathBuf::from("/tmp/test.lv1show");

    state
        .mark_show_file_saved(path.clone(), "999".to_string())
        .await;

    assert_eq!(state.current_show_file_path().await, Some(path));
}

#[tokio::test]
async fn new_show_file_clears_stale_selection_when_disconnected() {
    let state = ShellState::default();
    state
        .show
        .replace_snapshot(ShowSnapshot {
            lockout: false,
            scene_configs: vec![super::view::SceneConfig {
                scene_id: "stale::scene".to_string(),
                scene_index: 99,
                scene_name: "Stale".to_string(),
                duration_ms: 0,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
            }],
        })
        .await
        .unwrap();
    {
        let mut inner = state.inner.lock().await;
        inner.selected_scene_id = Some("stale::scene".to_string());
    }

    let snapshot = state.new_show_file().await.unwrap();

    assert_eq!(snapshot.selected_scene_id, None);
    assert!(snapshot.scene_configs.is_empty());
}

#[tokio::test]
async fn mark_show_file_saved_updates_path_and_clears_dirty() {
    let state = ShellState::default();

    let snapshot = state
        .mark_show_file_saved(
            std::path::PathBuf::from("/tmp/test.lv1show"),
            "999".to_string(),
        )
        .await;

    assert_eq!(
        snapshot.show_file_path.as_deref(),
        Some("/tmp/test.lv1show")
    );
    assert_eq!(snapshot.show_file_last_saved_at.as_deref(), Some("999"));
    assert!(!snapshot.show_file_dirty);
    assert_eq!(snapshot.logs.last().unwrap().message, "Show file saved");
}

#[tokio::test]
async fn load_show_file_applies_kept_configs_and_logs_pruned_entries() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;
    let mut file = ShowFile {
        schema_version: 1,
        app_version: "0.1.0".to_string(),
        saved_at: "123".to_string(),
        safety: crate::show_file::ShowFileSafety { lockout: true },
        scene_configs: vec![
            ShowFileSceneConfig {
                scene_index: 1,
                scene_name: "Intro".to_string(),
                duration_ms: 5000,
                channel_configs: vec![ShowFileChannelConfig {
                    group: 0,
                    channel: 2,
                    fader_db: Some(-9.0),
                }],
                scoped_channels: vec![ShowFileChannelRef {
                    group: 0,
                    channel: 2,
                }],
            },
            ShowFileSceneConfig {
                scene_index: 2,
                scene_name: "Missing".to_string(),
                duration_ms: 5000,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
            },
        ],
    };

    let snapshot = state
        .load_show_file_from_dto(std::path::PathBuf::from("/tmp/test.lv1show"), &mut file)
        .await
        .unwrap();

    assert!(snapshot.lockout);
    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].duration_ms, 5000);
    assert_eq!(snapshot.scene_configs[0].channel_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].scoped_channels.len(), 1);
    assert!(snapshot.show_file_dirty);
    assert!(
        snapshot
            .logs
            .iter()
            .any(|entry| { entry.message == "Deleted saved scene config during load: 2: Missing" })
    );
}

#[tokio::test]
async fn load_show_file_allows_empty_lv1_channels_when_scenes_exist() {
    let state = ShellState::default();
    state.begin_connection(lv1_scene_only_snapshot()).await;

    let mut file = ShowFile {
        schema_version: 1,
        app_version: "0.1.0".to_string(),
        saved_at: "123".to_string(),
        safety: crate::show_file::ShowFileSafety { lockout: false },
        scene_configs: vec![ShowFileSceneConfig {
            scene_index: 1,
            scene_name: "Intro".to_string(),
            duration_ms: 5000,
            channel_configs: vec![ShowFileChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-9.0),
            }],
            scoped_channels: vec![ShowFileChannelRef {
                group: 0,
                channel: 2,
            }],
        }],
    };

    let snapshot = state
        .load_show_file_from_dto(std::path::PathBuf::from("/tmp/test.lv1show"), &mut file)
        .await
        .unwrap();

    assert!(!snapshot.show_file_dirty);
    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].channel_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].scoped_channels.len(), 1);
}
