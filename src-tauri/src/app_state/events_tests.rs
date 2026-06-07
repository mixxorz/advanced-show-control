use super::shell::{ShellInner, ShellState};
use super::test_support::{connected_snapshot, connected_state_with_scene_and_channel};
use super::view::{AppConnectionState, FadeTarget, SceneFadeConfig};
use crate::show_file::DEFAULT_DURATION_MS;
use lv1_scene_fade_utility::lv1::messages::Lv1Event;
use lv1_scene_fade_utility::lv1::model::{
    ConnectionStatus, Lv1StateSnapshot, SceneListEntry, SceneState,
};

#[test]
fn scene_list_reconciliation_creates_default_configs() {
    let mut inner = ShellInner::default();
    inner.reconcile_scene_fade_configs(&[
        SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        },
        SceneListEntry {
            index: 2,
            name: "Verse".to_string(),
        },
    ]);

    assert_eq!(inner.scene_fade_configs.len(), 2);
    assert_eq!(inner.scene_fade_configs[0].scene_id, "1::Intro");
    assert!(!inner.scene_fade_configs[0].fade_enabled);
    assert_eq!(inner.scene_fade_configs[0].duration_ms, DEFAULT_DURATION_MS);
    assert!(inner.scene_fade_configs[0].fade_targets.is_empty());
    assert_eq!(inner.selected_scene_id.as_deref(), Some("1::Intro"));
}

#[test]
fn scene_list_reconciliation_preserves_matching_config_data() {
    let mut inner = ShellInner::default();
    inner.scene_fade_configs = vec![SceneFadeConfig {
        scene_id: "2::Verse".to_string(),
        scene_index: 2,
        scene_name: "Verse".to_string(),
        fade_enabled: true,
        duration_ms: DEFAULT_DURATION_MS,
        fade_targets: vec![FadeTarget {
            group: 0,
            channel: 4,
            channel_name: "Lead".to_string(),
            target_db: -5.5,
            enabled: true,
            updated_at: "123".to_string(),
        }],
    }];
    inner.selected_scene_id = Some("2::Verse".to_string());

    inner.reconcile_scene_fade_configs(&[
        SceneListEntry {
            index: 2,
            name: "Verse".to_string(),
        },
        SceneListEntry {
            index: 3,
            name: "Chorus".to_string(),
        },
    ]);

    let verse = inner
        .scene_fade_configs
        .iter()
        .find(|scene| scene.scene_id == "2::Verse")
        .unwrap();
    assert!(verse.fade_enabled);
    assert_eq!(verse.fade_targets.len(), 1);
    assert_eq!(inner.scene_fade_configs.len(), 2);
    assert_eq!(inner.selected_scene_id.as_deref(), Some("2::Verse"));
}

#[test]
fn scene_list_reconciliation_turns_off_listen_mode_when_selected_scene_disappears() {
    let mut inner = ShellInner::default();
    inner.scene_fade_configs = vec![SceneFadeConfig {
        scene_id: "1::Intro".to_string(),
        scene_index: 1,
        scene_name: "Intro".to_string(),
        fade_enabled: false,
        duration_ms: DEFAULT_DURATION_MS,
        fade_targets: Vec::new(),
    }];
    inner.selected_scene_id = Some("1::Intro".to_string());
    inner.listen_mode_active = true;

    inner.reconcile_scene_fade_configs(&[SceneListEntry {
        index: 2,
        name: "Verse".to_string(),
    }]);

    assert!(!inner.listen_mode_active);
    assert_eq!(inner.selected_scene_id.as_deref(), Some("2::Verse"));
    assert!(inner.logs.iter().any(|entry| entry.message
        == "Listen Mode stopped because selected scene is no longer available"));
}

#[test]
fn scene_reconciliation_marks_loaded_show_dirty_when_scene_removed() {
    let mut inner = ShellInner::default();
    inner.show_file_path = Some(std::path::PathBuf::from("/tmp/test.lv1show"));
    inner.scene_fade_configs = vec![SceneFadeConfig {
        scene_id: "1::Intro".to_string(),
        scene_index: 1,
        scene_name: "Intro".to_string(),
        fade_enabled: true,
        duration_ms: DEFAULT_DURATION_MS,
        fade_targets: vec![FadeTarget {
            group: 0,
            channel: 2,
            channel_name: "Lead".to_string(),
            target_db: -5.0,
            enabled: true,
            updated_at: "123".to_string(),
        }],
    }];

    inner.reconcile_scene_fade_configs(&[SceneListEntry {
        index: 2,
        name: "Verse".to_string(),
    }]);

    assert!(inner.show_file_dirty);
    assert_eq!(inner.scene_fade_configs.len(), 1);
    assert_eq!(inner.scene_fade_configs[0].scene_id, "2::Verse");
}

#[test]
fn scene_list_reconciliation_keeps_listen_mode_when_no_scene_was_selected() {
    let mut inner = ShellInner::default();
    inner.listen_mode_active = true;

    inner.reconcile_scene_fade_configs(&[SceneListEntry {
        index: 2,
        name: "Verse".to_string(),
    }]);

    assert!(inner.listen_mode_active);
    assert_eq!(inner.selected_scene_id.as_deref(), Some("2::Verse"));
    assert!(!inner.logs.iter().any(|entry| entry.message
        == "Listen Mode stopped because selected scene is no longer available"));
}

#[tokio::test]
async fn begin_connecting_sets_connecting_snapshot_and_logs_it() {
    let state = ShellState::default();

    let (generation, snapshot) = state.begin_connecting().await;

    assert_eq!(generation, 1);
    assert_eq!(snapshot.connection, AppConnectionState::Connecting);
    assert_eq!(snapshot.logs.len(), 1);
    assert_eq!(snapshot.logs[0].message, "Connecting to LV1");
}

#[tokio::test]
async fn lv1_scene_event_updates_rust_owned_snapshot() {
    let state = ShellState::default();
    let (generation, _snapshot) = state.begin_connecting().await;
    let snapshot = state
        .apply_lv1_event_for_generation(
            generation,
            &Lv1Event::SceneChanged(SceneState {
                index: 7,
                name: "Chorus".to_string(),
            }),
        )
        .await;

    let snapshot = snapshot.expect("event should apply to current generation");

    assert_eq!(snapshot.connection, AppConnectionState::Connecting);
    assert_eq!(snapshot.current_scene.unwrap().name, "Chorus");
    assert_eq!(snapshot.logs.len(), 2);
}

#[tokio::test]
async fn begin_connection_preserves_incoming_connection_state() {
    let state = ShellState::default();
    let (_, _connecting) = state.begin_connecting().await;

    let snapshot = state
        .begin_connection(Lv1StateSnapshot {
            connection: ConnectionStatus::Connecting,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
        })
        .await;

    assert_eq!(snapshot.connection, AppConnectionState::Connecting);
    assert_eq!(snapshot.logs.last().unwrap().message, "Connecting to LV1");

    let snapshot = state
        .begin_connection(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
        })
        .await;

    assert_eq!(snapshot.connection, AppConnectionState::Connected);
    assert_eq!(snapshot.logs.last().unwrap().message, "LV1 connected");
}

#[tokio::test]
async fn stale_lv1_events_are_ignored_after_generation_change() {
    let state = ShellState::default();

    let (first_generation, first_snapshot) = state.begin_connecting().await;
    assert_eq!(first_snapshot.connection, AppConnectionState::Connecting);

    let (second_generation, second_connecting) = state.begin_connecting().await;
    assert_eq!(second_generation, first_generation + 1);
    assert_eq!(second_connecting.connection, AppConnectionState::Connecting);

    let second_snapshot = state
        .begin_connection(Lv1StateSnapshot {
            scene: None,
            scene_list: vec![],
            channels: vec![],
            connection: ConnectionStatus::Connected,
        })
        .await;
    assert_eq!(second_snapshot.connection, AppConnectionState::Connected);

    let stale = state
        .apply_lv1_event_for_generation(
            first_generation,
            &Lv1Event::SceneChanged(SceneState {
                index: 5,
                name: "Intro".to_string(),
            }),
        )
        .await;
    assert!(stale.is_none());

    let current = state
        .apply_lv1_event_for_generation(
            second_generation,
            &Lv1Event::SceneChanged(SceneState {
                index: 6,
                name: "Bridge".to_string(),
            }),
        )
        .await;
    assert!(current.is_some());

    let latest = current.expect("event should apply to current generation");
    assert_eq!(latest.current_scene.unwrap().name, "Bridge");
}

#[tokio::test]
async fn disconnect_increments_generation_and_ignores_old_events() {
    let state = ShellState::default();
    let (generation, snapshot) = state.begin_connecting().await;
    assert_eq!(snapshot.connection, AppConnectionState::Connecting);

    let snapshot = state.begin_connection(connected_snapshot()).await;
    assert_eq!(snapshot.connection, AppConnectionState::Connected);

    let disconnected = state.disconnect().await;
    assert_eq!(disconnected.connection, AppConnectionState::Disconnected);

    let stale = state
        .apply_lv1_event_for_generation(
            generation,
            &Lv1Event::SceneChanged(SceneState {
                index: 9,
                name: "Outro".to_string(),
            }),
        )
        .await;
    assert!(stale.is_none());
}

#[tokio::test]
async fn disconnect_turns_off_listen_mode() {
    let state = ShellState::default();
    let _ = state
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

    {
        let mut inner = state.inner.lock().await;
        inner.listen_mode_active = true;
    }

    let snapshot = state.disconnect().await;

    assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
    assert!(!snapshot.listen_mode_active);
}

#[tokio::test]
async fn unknown_channel_fader_warning_logs_only_once_per_channel() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;
    state.set_listen_mode(true).await.unwrap();

    state
        .apply_lv1_event_for_generation(
            generation,
            &Lv1Event::FaderChanged {
                group: 0,
                channel: 99,
                gain_db: -1.0,
            },
        )
        .await;
    state
        .apply_lv1_event_for_generation(
            generation,
            &Lv1Event::FaderChanged {
                group: 0,
                channel: 99,
                gain_db: -2.0,
            },
        )
        .await;

    let warnings: Vec<_> = state
        .snapshot()
        .await
        .logs
        .into_iter()
        .filter(|entry| entry.message == "Ignored fader target for unknown channel 0/99")
        .collect();
    assert_eq!(warnings.len(), 1);
}

#[tokio::test]
async fn disconnect_turns_off_listen_mode_and_preserves_configs() {
    let state = ShellState::default();
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;
    state.set_listen_mode(true).await.unwrap();

    let view = state.disconnect().await;

    assert!(!view.listen_mode_active);
    assert_eq!(view.scene_fade_configs.len(), 1);
}

#[tokio::test]
async fn lv1_disconnected_event_turns_off_listen_mode() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    let _ = state
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

    {
        let mut inner = state.inner.lock().await;
        inner.listen_mode_active = true;
    }

    let snapshot = state
        .apply_lv1_event_for_generation(generation, &Lv1Event::Disconnected)
        .await
        .expect("event should apply to current generation");

    assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
    assert!(!snapshot.listen_mode_active);
}
