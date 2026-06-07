use super::shell::{ShellInner, ShellState};
use super::test_support::{connected_snapshot, connected_state_with_scene_and_channel, scene_config};
use super::view::{AppConnectionState, ChannelConfig, ChannelRef};
use lv1_scene_fade_utility::lv1::messages::Lv1Event;
use lv1_scene_fade_utility::lv1::model::{
    ConnectionStatus, Lv1StateSnapshot, SceneListEntry, SceneState,
};

#[tokio::test]
async fn fade_events_update_fade_state() {
    use lv1_scene_fade_utility::fade::types::FadeEvent;
    use super::view::AppFadeState;

    let state = ShellState::default();

    let started = state.apply_fade_event(&FadeEvent::FadeStarted).await;
    assert_eq!(started.fade_state, AppFadeState::Running);

    let completed = state.apply_fade_event(&FadeEvent::FadeCompleted).await;
    assert_eq!(completed.fade_state, AppFadeState::Idle);

    let aborted = state.apply_fade_event(&FadeEvent::FadeAborted).await;
    assert_eq!(aborted.fade_state, AppFadeState::Idle);
}

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

    assert_eq!(inner.scene_configs.len(), 2);
    assert_eq!(inner.scene_configs[0].scene_id, "1::Intro");
    assert_eq!(inner.scene_configs[0].scene_index, 1);
    assert_eq!(inner.scene_configs[0].scene_name, "Intro");
    assert_eq!(inner.scene_configs[0].duration_ms, 0);
    assert!(inner.scene_configs[0].channel_configs.is_empty());
    assert!(inner.scene_configs[0].scoped_channels.is_empty());
    assert_eq!(inner.selected_scene_id.as_deref(), Some("1::Intro"));
}

#[test]
fn scene_list_reconciliation_preserves_matching_config_data() {
    let mut inner = ShellInner::default();
    inner.scene_configs = vec![scene_config(
        2,
        "Verse",
        vec![ChannelConfig {
            group: 0,
            channel: 4,
            fader_db: Some(-5.5),
        }],
        vec![ChannelRef {
            group: 0,
            channel: 4,
        }],
    )];
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
        .scene_configs
        .iter()
        .find(|scene| scene.scene_id == "2::Verse")
        .unwrap();
    assert_eq!(verse.scene_index, 2);
    assert_eq!(verse.scene_name, "Verse");
    assert_eq!(verse.channel_configs.len(), 1);
    assert_eq!(verse.channel_configs[0].group, 0);
    assert_eq!(verse.channel_configs[0].channel, 4);
    assert_eq!(verse.channel_configs[0].fader_db, Some(-5.5));
    assert_eq!(verse.scoped_channels.len(), 1);
    assert_eq!(verse.scoped_channels[0].group, 0);
    assert_eq!(verse.scoped_channels[0].channel, 4);
    assert_eq!(inner.scene_configs.len(), 2);
    assert_eq!(inner.selected_scene_id.as_deref(), Some("2::Verse"));
}

#[test]
fn scene_reconciliation_marks_loaded_show_dirty_when_scene_removed() {
    let mut inner = ShellInner::default();
    inner.show_file_path = Some(std::path::PathBuf::from("/tmp/test.lv1show"));
    inner.scene_configs = vec![scene_config(
        1,
        "Intro",
        vec![ChannelConfig {
            group: 0,
            channel: 2,
            fader_db: Some(-5.0),
        }],
        vec![ChannelRef {
            group: 0,
            channel: 2,
        }],
    )];

    inner.reconcile_scene_fade_configs(&[SceneListEntry {
        index: 2,
        name: "Verse".to_string(),
    }]);

    assert!(inner.show_file_dirty);
    assert_eq!(inner.scene_configs.len(), 1);
    assert_eq!(inner.scene_configs[0].scene_id, "2::Verse");
    assert!(inner.scene_configs[0].channel_configs.is_empty());
    assert!(inner.scene_configs[0].scoped_channels.is_empty());
    assert_eq!(inner.selected_scene_id.as_deref(), Some("2::Verse"));
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
async fn fader_event_updates_live_mirror_without_touching_scene_configs() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;
    {
        let mut inner = state.inner.lock().await;
        inner.scene_configs = vec![scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-8.0),
            }],
            vec![ChannelRef {
                group: 0,
                channel: 2,
            }],
        )];
    }

    let snapshot = state
        .apply_lv1_event_for_generation(
            generation,
            &Lv1Event::FaderChanged {
                group: 0,
                channel: 2,
                gain_db: -6.5,
            },
        )
        .await
        .expect("event should apply to current generation");

    assert_eq!(snapshot.connection, AppConnectionState::Connected);
    assert_eq!(snapshot.scene_configs[0].channel_configs[0].fader_db, Some(-8.0));
    assert_eq!(snapshot.scene_configs[0].scoped_channels.len(), 1);

    let inner = state.inner.lock().await;
    assert_eq!(inner.lv1_snapshot.as_ref().unwrap().channels[0].gain_db, -6.5);
}
