use super::shell::ShellState;
use super::test_support::connected_state_with_scene_and_channel;
use super::view::SceneFadeConfig;
use crate::show_file::DEFAULT_DURATION_MS;

#[tokio::test]
async fn export_show_file_contains_current_configs() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;
    state
        .set_scene_fade_enabled("1::Intro".to_string(), true)
        .await
        .unwrap();

    let file = state.export_show_file("saved".to_string()).await;

    assert_eq!(file.schema_version, 1);
    assert!(!file.safety.lockout);
    assert_eq!(file.saved_at, "saved");
    assert_eq!(file.scene_fade_configs[0].scene_index, 1);
    assert_eq!(file.scene_fade_configs[0].duration_ms, 4000);
}

#[tokio::test]
async fn export_show_file_for_save_rejects_listen_mode() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;
    state.set_listen_mode(true).await.unwrap();

    assert_eq!(
        state
            .export_show_file_for_save("saved".to_string())
            .await
            .unwrap_err(),
        "Stop Listen Mode before saving a show file"
    );
}

#[tokio::test]
async fn new_show_file_clears_file_state_and_rebuilds_current_lv1_scenes() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;

    {
        let mut inner = state.inner.lock().await;
        inner.scene_fade_configs[0].fade_enabled = true;
        inner.show_file_path = Some(std::path::PathBuf::from("/tmp/existing.lv1show"));
        inner.show_file_last_saved_at = Some("123".to_string());
        inner.show_file_dirty = true;
        inner.lockout = true;
    }

    let snapshot = state.new_show_file().await.unwrap();

    assert_eq!(snapshot.show_file_path, None);
    assert_eq!(snapshot.show_file_last_saved_at, None);
    assert!(!snapshot.show_file_dirty);
    assert!(!snapshot.lockout);
    assert_eq!(snapshot.scene_fade_configs.len(), 1);
    assert!(!snapshot.scene_fade_configs[0].fade_enabled);
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

    {
        let mut inner = state.inner.lock().await;
        inner.selected_scene_id = Some("stale::scene".to_string());
        inner.scene_fade_configs = vec![SceneFadeConfig {
            scene_id: "stale::scene".to_string(),
            scene_index: 99,
            scene_name: "Stale".to_string(),
            fade_enabled: true,
            duration_ms: DEFAULT_DURATION_MS,
            fade_targets: Vec::new(),
        }];
    }

    let snapshot = state.new_show_file().await.unwrap();

    assert_eq!(snapshot.selected_scene_id, None);
    assert!(snapshot.scene_fade_configs.is_empty());
}

#[tokio::test]
async fn new_show_file_rejects_listen_mode() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;
    state.set_listen_mode(true).await.unwrap();

    assert_eq!(
        state.new_show_file().await.unwrap_err(),
        "Stop Listen Mode before creating a new show file"
    );
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
    let mut file = crate::show_file::ShowFile {
        schema_version: 1,
        app_version: "0.1.0".to_string(),
        saved_at: "123".to_string(),
        safety: crate::show_file::ShowFileSafety { lockout: true },
        scene_fade_configs: vec![
            crate::show_file::ShowFileSceneFadeConfig {
                scene_index: 1,
                scene_name: "Intro".to_string(),
                fade_enabled: true,
                duration_ms: 5000,
                fade_targets: vec![crate::show_file::ShowFileFadeTarget {
                    group: 0,
                    channel: 2,
                    channel_name: "Lead".to_string(),
                    target_db: -9.0,
                    enabled: true,
                    updated_at: "999".to_string(),
                }],
            },
            crate::show_file::ShowFileSceneFadeConfig {
                scene_index: 2,
                scene_name: "Missing".to_string(),
                fade_enabled: true,
                duration_ms: 5000,
                fade_targets: Vec::new(),
            },
        ],
    };

    let snapshot = state
        .load_show_file_from_dto(std::path::PathBuf::from("/tmp/test.lv1show"), &mut file)
        .await
        .unwrap();

    assert!(snapshot.lockout);
    assert_eq!(snapshot.scene_fade_configs.len(), 1);
    assert_eq!(snapshot.scene_fade_configs[0].duration_ms, 5000);
    assert_eq!(
        snapshot.scene_fade_configs[0].fade_targets[0].channel_name,
        "Lead"
    );
    assert!(snapshot.show_file_dirty);
    assert!(
        snapshot
            .logs
            .iter()
            .any(|entry| { entry.message == "Deleted saved scene config during load: 2: Missing" })
    );
}

#[tokio::test]
async fn load_show_file_clears_unknown_fader_warnings() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;
    {
        let mut inner = state.inner.lock().await;
        inner.unknown_fader_warnings.insert((0, 99));
    }

    let mut file = crate::show_file::ShowFile {
        schema_version: 1,
        app_version: "0.1.0".to_string(),
        saved_at: "123".to_string(),
        safety: crate::show_file::ShowFileSafety { lockout: false },
        scene_fade_configs: vec![crate::show_file::ShowFileSceneFadeConfig {
            scene_index: 1,
            scene_name: "Intro".to_string(),
            fade_enabled: false,
            duration_ms: 4000,
            fade_targets: Vec::new(),
        }],
    };

    state
        .load_show_file_from_dto(std::path::PathBuf::from("/tmp/test.lv1show"), &mut file)
        .await
        .unwrap();

    let inner = state.inner.lock().await;
    assert!(inner.unknown_fader_warnings.is_empty());
}
