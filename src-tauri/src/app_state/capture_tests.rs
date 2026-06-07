use super::shell::ShellState;
use super::test_support::connected_state_with_scene_and_channel;
use lv1_scene_fade_utility::lv1::messages::Lv1Event;
use lv1_scene_fade_utility::lv1::model::{
    ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry,
};

#[tokio::test]
async fn set_scene_fade_enabled_marks_show_file_dirty() {
    let state = ShellState::default();
    state
        .begin_connection(Lv1StateSnapshot {
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
        })
        .await;

    let snapshot = state
        .set_scene_fade_enabled("1::Intro".to_string(), true)
        .await
        .unwrap();

    assert!(snapshot.scene_fade_configs[0].fade_enabled);
    assert!(snapshot.show_file_dirty);
}

#[tokio::test]
async fn set_scene_duration_ms_updates_duration_and_marks_dirty() {
    let state = ShellState::default();
    state
        .begin_connection(Lv1StateSnapshot {
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
        })
        .await;

    let snapshot = state
        .set_scene_duration_ms("1::Intro".to_string(), 8_000)
        .await
        .unwrap();

    assert_eq!(snapshot.scene_fade_configs[0].duration_ms, 8_000);
    assert!(snapshot.show_file_dirty);
}

#[tokio::test]
async fn listen_mode_requires_selected_scene_and_known_channels() {
    let state = ShellState::default();

    let err = state.set_listen_mode(true).await.unwrap_err();
    assert_eq!(err, "Select a scene before starting Listen Mode");

    let snapshot = state
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
    assert_eq!(snapshot.selected_scene_id.as_deref(), Some("1::Intro"));

    let err = state.set_listen_mode(true).await.unwrap_err();
    assert_eq!(err, "LV1 channel list is empty");
}

#[tokio::test]
async fn fader_events_write_targets_only_while_listen_mode_is_active() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;

    state
        .apply_lv1_event_for_generation(
            generation,
            &Lv1Event::FaderChanged {
                group: 0,
                channel: 2,
                gain_db: -4.5,
            },
        )
        .await;
    assert!(
        state.snapshot().await.scene_fade_configs[0]
            .fade_targets
            .is_empty()
    );

    state.set_listen_mode(true).await.unwrap();
    state
        .apply_lv1_event_for_generation(
            generation,
            &Lv1Event::FaderChanged {
                group: 0,
                channel: 2,
                gain_db: -4.5,
            },
        )
        .await;

    let view = state.snapshot().await;
    let targets = &view.scene_fade_configs[0].fade_targets;
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].group, 0);
    assert_eq!(targets[0].channel, 2);
    assert_eq!(targets[0].channel_name, "Lead");
    assert_eq!(targets[0].target_db, -4.5);
    assert!(targets[0].enabled);
    assert!(view.show_file_dirty);
}

#[tokio::test]
async fn repeated_fader_event_updates_existing_target() {
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
                channel: 2,
                gain_db: -4.5,
            },
        )
        .await;
    state
        .apply_lv1_event_for_generation(
            generation,
            &Lv1Event::FaderChanged {
                group: 0,
                channel: 2,
                gain_db: -3.0,
            },
        )
        .await;

    let targets = &state.snapshot().await.scene_fade_configs[0].fade_targets;
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].target_db, -3.0);
    assert_eq!(targets[0].channel_name, "Lead");
}

#[tokio::test]
async fn repeated_fader_event_preserves_disabled_target_state() {
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
                channel: 2,
                gain_db: -4.5,
            },
        )
        .await;
    state
        .set_fade_target_enabled("1::Intro".to_string(), 0, 2, false)
        .await
        .unwrap();

    state
        .apply_lv1_event_for_generation(
            generation,
            &Lv1Event::FaderChanged {
                group: 0,
                channel: 2,
                gain_db: -3.25,
            },
        )
        .await;

    let targets = &state.snapshot().await.scene_fade_configs[0].fade_targets;
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].target_db, -3.25);
    assert!(!targets[0].enabled);
    assert_eq!(targets[0].channel_name, "Lead");
}

#[tokio::test]
async fn removed_target_can_be_recaptured_while_listen_mode_is_active() {
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
                channel: 2,
                gain_db: -4.5,
            },
        )
        .await;
    state.remove_fade_target("1::Intro", 0, 2).await.unwrap();
    let snapshot = state.snapshot().await;
    assert!(snapshot.scene_fade_configs[0].fade_targets.is_empty());
    assert!(snapshot.show_file_dirty);

    state
        .apply_lv1_event_for_generation(
            generation,
            &Lv1Event::FaderChanged {
                group: 0,
                channel: 2,
                gain_db: -2.0,
            },
        )
        .await;
    let targets = &state.snapshot().await.scene_fade_configs[0].fade_targets;
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].target_db, -2.0);
}
