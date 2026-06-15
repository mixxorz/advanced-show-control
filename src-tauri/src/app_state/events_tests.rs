use super::events::ProjectionOutcome;
use super::shell::ShellState;
use super::test_support::{
    connected_snapshot, connected_state_with_scene_and_channel, scene_config,
};
use super::view::{AppConnectionState, ChannelConfig, ChannelRef, ShowSnapshot};
use advanced_show_control::fade::events::FadeEvent;
use advanced_show_control::lv1::events::Lv1Event;
use advanced_show_control::lv1::types::{
    ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry, SceneState,
};

#[tokio::test]
async fn fade_events_update_fade_state() {
    use super::view::AppFadeState;
    use advanced_show_control::fade::types::FadeParameter;

    let state = ShellState::default();

    let started = state.apply_fade_event(&FadeEvent::FadeStarted).await;
    assert_eq!(started.fade_state, AppFadeState::Running);

    let completed = state.apply_fade_event(&FadeEvent::FadeCompleted).await;
    assert_eq!(completed.fade_state, AppFadeState::Idle);

    let aborted = state.apply_fade_event(&FadeEvent::FadeAborted).await;
    assert_eq!(aborted.fade_state, AppFadeState::Idle);

    let overridden = state
        .apply_fade_event(&FadeEvent::ChannelOverride {
            group: 3,
            channel: 7,
            parameter: FadeParameter::FaderDb,
        })
        .await;
    assert_eq!(
        overridden.logs.last().unwrap().message,
        "Fade channel override detected: group=3 channel=7"
    );
}

#[tokio::test]
async fn channel_completed_logs_without_clearing_running_state() {
    use super::view::AppFadeState;
    use advanced_show_control::fade::events::FadeEvent;
    use advanced_show_control::fade::types::FadeParameter;

    let state = ShellState::default();
    let started = state.apply_fade_event(&FadeEvent::FadeStarted).await;
    assert_eq!(started.fade_state, AppFadeState::Running);

    let completed = state
        .apply_fade_event(&FadeEvent::ChannelCompleted {
            group: 0,
            channel: 2,
            parameter: FadeParameter::FaderDb,
        })
        .await;

    assert_eq!(completed.fade_state, AppFadeState::Running);
    assert!(
        completed
            .logs
            .iter()
            .any(|log| { log.message == "Fade channel completed: group 0, channel 2" })
    );
}

#[tokio::test]
async fn begin_connection_preserves_scene_configs_when_initial_scene_list_is_empty() {
    let state = ShellState::default();
    state
        .show
        .replace_snapshot(ShowSnapshot {
            lockout: false,
            scene_configs: vec![scene_config(1, "Intro", Vec::new(), Vec::new())],
        })
        .await;
    {
        let mut inner = state.inner.lock().await;
        inner.selected_scene_id = Some("1::Intro".to_string());
    }

    let snapshot = state
        .begin_connection(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
        })
        .await;

    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].scene_id, "1::Intro");
    assert_eq!(snapshot.selected_scene_id.as_deref(), Some("1::Intro"));
}

#[tokio::test]
async fn stale_initial_connection_snapshot_does_not_overwrite_newer_state() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;

    state
        .show
        .replace_snapshot(ShowSnapshot {
            lockout: false,
            scene_configs: vec![scene_config(2, "Verse", Vec::new(), Vec::new())],
        })
        .await;
    {
        let mut inner = state.inner.lock().await;
        inner.selected_scene_id = Some("2::Verse".to_string());
    }

    let _ = state.disconnect().await;

    let snapshot = state
        .begin_connection_for_generation(
            generation,
            Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: Vec::new(),
                channels: Vec::new(),
            },
        )
        .await;

    assert!(snapshot.is_none());

    let current = state.snapshot().await;
    assert_eq!(current.connection, AppConnectionState::Disconnected);
    assert_eq!(current.scene_configs.len(), 1);
    assert_eq!(current.scene_configs[0].scene_id, "2::Verse");
    assert_eq!(current.selected_scene_id.as_deref(), Some("2::Verse"));
}

#[tokio::test]
async fn stale_initial_connection_scene_list_does_not_reconcile_show_configs() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    let _ = state.disconnect().await;

    let snapshot = state
        .begin_connection_for_generation(
            generation,
            Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: vec![SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                }],
                channels: Vec::new(),
            },
        )
        .await;

    assert!(snapshot.is_none());
    assert_eq!(state.snapshot().await.scene_configs.len(), 0);
}

#[tokio::test]
async fn lv1_disconnected_event_snapshot_includes_show_configs() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state
        .show
        .store_scene_config(
            "1::Intro".to_string(),
            vec![ChannelInfo {
                group: 0,
                channel: 1,
                name: "Lead".to_string(),
                gain_db: -6.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
        )
        .await
        .unwrap();

    assert_eq!(
        state
            .apply_lv1_event_to_projection(
                generation,
                &Lv1Event::Disconnected {
                    reason: "test".to_string(),
                },
            )
            .await,
        ProjectionOutcome::Applied
    );
    let snapshot = state
        .snapshot_for_generation(generation)
        .await
        .expect("disconnect should apply to current generation");

    assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].scene_id, "1::Intro");
}

#[tokio::test]
async fn stale_scene_list_changed_event_does_not_mutate_show_configs() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    let _ = state.disconnect().await;

    assert_eq!(
        state
            .apply_lv1_event_to_projection(
                generation,
                &Lv1Event::SceneListChanged(vec![SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                }]),
            )
            .await,
        ProjectionOutcome::Stale
    );
    assert_eq!(state.snapshot().await.scene_configs.len(), 0);
}

#[tokio::test]
async fn scene_list_projection_does_not_append_ui_log() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;

    assert_eq!(
        state
            .apply_lv1_event_to_projection(
                generation,
                &Lv1Event::SceneListChanged(vec![SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                }]),
            )
            .await,
        ProjectionOutcome::Applied
    );
    let snapshot = state
        .snapshot_for_generation(generation)
        .await
        .expect("event should apply to current generation");

    assert!(
        snapshot
            .logs
            .iter()
            .all(|entry| !entry.message.contains("Scene list updated"))
    );
}

#[tokio::test]
async fn channel_topology_projection_does_not_append_ui_log() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;

    assert_eq!(
        state
            .apply_lv1_event_to_projection(
                generation,
                &Lv1Event::ChannelTopologyChanged(vec![ChannelInfo {
                    group: 0,
                    channel: 1,
                    name: "Lead".to_string(),
                    gain_db: -6.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }]),
            )
            .await,
        ProjectionOutcome::Applied
    );
    let snapshot = state
        .snapshot_for_generation(generation)
        .await
        .expect("event should apply to current generation");

    assert!(
        snapshot
            .logs
            .iter()
            .all(|entry| !entry.message.contains("Channel topology updated"))
    );
}

#[test]
fn scene_list_reconciliation_creates_default_configs() {
    let mut state = advanced_show_control::show::state::ShowState {
        lockout: false,
        scene_configs: Vec::new(),
    };
    assert!(state.reconcile_scene_fade_configs(&[
        SceneListEntry {
            index: 1,
            name: "Intro".to_string()
        },
        SceneListEntry {
            index: 2,
            name: "Verse".to_string()
        },
    ]));

    assert_eq!(state.scene_configs.len(), 2);
    assert_eq!(state.scene_configs[0].scene_id, "1::Intro");
    assert_eq!(state.scene_configs[0].scene_index, 1);
    assert_eq!(state.scene_configs[0].scene_name, "Intro");
    assert_eq!(state.scene_configs[0].duration_ms, 0);
    assert!(state.scene_configs[0].channel_configs.is_empty());
    assert!(state.scene_configs[0].scoped_channels.is_empty());
}

#[test]
fn scene_list_reconciliation_preserves_matching_config_data() {
    let mut state = advanced_show_control::show::state::ShowState {
        lockout: false,
        scene_configs: vec![scene_config(
            2,
            "Verse",
            vec![ChannelConfig {
                group: 0,
                channel: 4,
                fader_db: Some(-5.5),
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            vec![ChannelRef {
                group: 0,
                channel: 4,
            }],
        )],
    };

    assert!(state.reconcile_scene_fade_configs(&[
        SceneListEntry {
            index: 2,
            name: "Verse".to_string()
        },
        SceneListEntry {
            index: 3,
            name: "Chorus".to_string()
        },
    ]));

    let verse = state
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
    assert_eq!(state.scene_configs.len(), 2);
}

#[test]
fn scene_reconciliation_marks_loaded_show_dirty_when_scene_removed() {
    let mut state = advanced_show_control::show::state::ShowState {
        lockout: false,
        scene_configs: vec![scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-5.0),
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            vec![ChannelRef {
                group: 0,
                channel: 2,
            }],
        )],
    };

    assert!(state.reconcile_scene_fade_configs(&[SceneListEntry {
        index: 2,
        name: "Verse".to_string()
    }]));
    assert_eq!(state.scene_configs.len(), 1);
    assert_eq!(state.scene_configs[0].scene_id, "2::Verse");
    assert!(state.scene_configs[0].channel_configs.is_empty());
    assert!(state.scene_configs[0].scoped_channels.is_empty());
}

#[tokio::test]
async fn late_scene_list_event_returns_snapshot_without_deadlock() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state
        .begin_connection_for_generation(
            generation,
            Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: Vec::new(),
                channels: Vec::new(),
            },
        )
        .await
        .expect("current generation should connect");

    assert!(
        state
            .apply_lv1_event_to_projection(
                generation,
                &Lv1Event::SceneListChanged(vec![SceneListEntry {
                    index: 0,
                    name: "Intro".to_string(),
                }]),
            )
            .await
            .was_applied()
    );
    let snapshot = state
        .snapshot_for_generation(generation)
        .await
        .expect("scene list should apply to current generation");

    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].scene_id, "0::Intro");
}

#[tokio::test]
async fn scene_list_event_logs_reconciliation_preview() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state
        .show
        .replace_snapshot(ShowSnapshot {
            lockout: false,
            scene_configs: vec![
                scene_config(0, "Intro", Vec::new(), Vec::new()),
                scene_config(1, "Verse", Vec::new(), Vec::new()),
            ],
        })
        .await;
    state
        .begin_connection_for_generation(
            generation,
            Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: Vec::new(),
                channels: Vec::new(),
            },
        )
        .await
        .expect("current generation should connect");

    assert!(
        state
            .apply_lv1_event_to_projection(
                generation,
                &Lv1Event::SceneListChanged(vec![
                    SceneListEntry {
                        index: 0,
                        name: "Verse".to_string(),
                    },
                    SceneListEntry {
                        index: 1,
                        name: "Intro".to_string(),
                    },
                ]),
            )
            .await
            .was_applied()
    );
    let snapshot = state
        .snapshot_for_generation(generation)
        .await
        .expect("scene list should apply to current generation");

    assert!(snapshot.logs.iter().any(|log| {
        log.message.contains("scene reconciliation preview")
            && log
                .message
                .contains("change=ambiguous exact-match-fallback")
            && log.message.contains("move_candidates=[0->1,1->0]")
    }));
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
async fn lv1_disconnected_event_enters_reconnect_state() {
    let state = super::ShellState::default();
    state
        .set_connected_lv1_identity(Some(crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        }))
        .await;
    let (generation, _) = state.begin_connecting().await;
    assert!(
        state
            .apply_lv1_event_to_projection(
                generation,
                &Lv1Event::Disconnected {
                    reason: "test".to_string(),
                },
            )
            .await
            .was_applied()
    );
    let snapshot = state.snapshot_for_generation(generation).await.unwrap();

    assert!(snapshot.reconnect.active);
    assert_eq!(snapshot.reconnect.attempt, 1);
}

#[tokio::test]
async fn lv1_connected_event_refreshes_discovered_row_status() {
    let state = super::ShellState::default();
    let identity = crate::connection_state::Lv1SystemIdentity {
        uuid: Some("uuid-1".to_string()),
        host: Some("LV1-FOH".to_string()),
        address: "192.168.1.35".to_string(),
        port: 50000,
    };
    state
        .set_connected_lv1_identity(Some(identity.clone()))
        .await;
    state
        .set_discovered_lv1_systems(vec![crate::connection_state::DiscoveredLv1System {
            identity,
            latency_ms: Some(10),
            status: crate::connection_state::DiscoveredLv1Status::Available,
        }])
        .await;
    let (generation, _) = state.begin_connecting().await;
    assert!(
        state
            .apply_lv1_event_to_projection(
                generation,
                &Lv1Event::Disconnected {
                    reason: "test".to_string(),
                },
            )
            .await
            .was_applied()
    );
    let disconnected = state
        .snapshot_for_generation(generation)
        .await
        .expect("disconnect should apply to current generation");
    assert_ne!(
        disconnected.discovered_lv1_systems[0].status,
        crate::connection_state::DiscoveredLv1Status::Connected
    );

    assert!(
        state
            .apply_lv1_event_to_projection(generation, &Lv1Event::Connected)
            .await
            .was_applied()
    );
    let connected = state
        .snapshot_for_generation(generation)
        .await
        .expect("connected event should apply to current generation");

    assert_eq!(
        connected.discovered_lv1_systems[0].status,
        crate::connection_state::DiscoveredLv1Status::Connected
    );
}

#[tokio::test]
async fn repeated_lv1_disconnected_events_keep_using_known_reconnect_target() {
    let state = super::ShellState::default();
    state
        .set_connected_lv1_identity(Some(crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        }))
        .await;
    let (generation, _) = state.begin_connecting().await;

    assert!(
        state
            .apply_lv1_event_to_projection(
                generation,
                &Lv1Event::Disconnected {
                    reason: "test".to_string(),
                },
            )
            .await
            .was_applied()
    );
    let first_disconnect = state
        .snapshot_for_generation(generation)
        .await
        .expect("first disconnect should apply to current generation");
    assert!(first_disconnect.reconnect.active);
    let first_attempt = first_disconnect.reconnect.attempt;

    assert!(
        state
            .apply_lv1_event_to_projection(generation, &Lv1Event::Connected)
            .await
            .was_applied()
    );
    let connected = state
        .snapshot_for_generation(generation)
        .await
        .expect("connected event should apply to current generation");
    assert!(!connected.reconnect.active);

    assert!(
        state
            .apply_lv1_event_to_projection(
                generation,
                &Lv1Event::Disconnected {
                    reason: "test".to_string(),
                },
            )
            .await
            .was_applied()
    );
    let second_disconnect = state
        .snapshot_for_generation(generation)
        .await
        .expect("second disconnect should apply to current generation");

    assert!(second_disconnect.reconnect.active);
    assert!(second_disconnect.reconnect.attempt > first_attempt);
}

#[tokio::test]
async fn lv1_disconnected_event_without_connected_identity_stays_out_of_reconnect_state() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;

    assert!(
        state
            .apply_lv1_event_to_projection(
                generation,
                &Lv1Event::Disconnected {
                    reason: "test".to_string(),
                },
            )
            .await
            .was_applied()
    );
    let snapshot = state
        .snapshot_for_generation(generation)
        .await
        .expect("event should apply to current generation");

    assert!(!snapshot.reconnect.active);
}

#[tokio::test]
async fn lv1_scene_event_updates_rust_owned_snapshot() {
    let state = ShellState::default();
    let (generation, _snapshot) = state.begin_connecting().await;
    assert!(
        state
            .apply_lv1_event_to_projection(
                generation,
                &Lv1Event::SceneChanged(SceneState {
                    index: 7,
                    name: "Chorus".to_string(),
                }),
            )
            .await
            .was_applied()
    );

    let snapshot = state
        .snapshot_for_generation(generation)
        .await
        .expect("event should apply to current generation");

    assert_eq!(snapshot.connection, AppConnectionState::Connecting);
    assert_eq!(snapshot.current_scene.unwrap().name, "Chorus");
    assert_eq!(snapshot.logs.len(), 2);
}

#[tokio::test]
async fn duplicate_connecting_attempt_is_rejected() {
    let state = ShellState::default();
    let first = state.try_begin_connecting().await;
    let second = state.try_begin_connecting().await;

    assert!(first.is_some());
    assert!(second.is_none());

    let snapshot = state.snapshot().await;
    let connecting_logs = snapshot
        .logs
        .iter()
        .filter(|log| log.message == "Connecting to LV1")
        .count();
    assert_eq!(connecting_logs, 1);
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
async fn begin_connection_clears_reconnect_state_when_connected() {
    let state = ShellState::default();
    state.set_reconnect_active(true).await;

    let snapshot = state
        .begin_connection(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
        })
        .await;

    assert_eq!(snapshot.connection, AppConnectionState::Connected);
    assert!(!snapshot.reconnect.active);
}

#[tokio::test]
async fn lv1_connected_event_clears_reconnect_state() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.set_reconnect_active(true).await;

    assert!(
        state
            .apply_lv1_event_to_projection(generation, &Lv1Event::Connected)
            .await
            .was_applied()
    );
    let snapshot = state
        .snapshot_for_generation(generation)
        .await
        .expect("event should apply to current generation");

    assert_eq!(snapshot.connection, AppConnectionState::Connected);
    assert!(!snapshot.reconnect.active);
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
        .apply_lv1_event_to_projection(
            first_generation,
            &Lv1Event::SceneChanged(SceneState {
                index: 5,
                name: "Intro".to_string(),
            }),
        )
        .await;
    assert_eq!(stale, ProjectionOutcome::Stale);

    let current = state
        .apply_lv1_event_to_projection(
            second_generation,
            &Lv1Event::SceneChanged(SceneState {
                index: 6,
                name: "Bridge".to_string(),
            }),
        )
        .await;
    assert_eq!(current, ProjectionOutcome::Applied);

    let latest = state
        .snapshot_for_generation(second_generation)
        .await
        .expect("event should apply to current generation");
    assert_eq!(latest.current_scene.unwrap().name, "Bridge");
}

#[tokio::test]
async fn stale_fade_events_are_ignored_after_generation_change() {
    use super::view::AppFadeState;
    use advanced_show_control::fade::events::FadeEvent;

    let state = ShellState::default();
    let (first_generation, _) = state.begin_connecting().await;
    let _ = state.begin_connecting().await;

    let before = state.snapshot().await;

    let stale = state
        .apply_fade_event_to_projection(first_generation, &FadeEvent::FadeStarted)
        .await;

    assert_eq!(stale, ProjectionOutcome::Stale);

    let after = state.snapshot().await;
    assert_eq!(after.fade_state, before.fade_state);
    assert_eq!(after.fade_state, AppFadeState::Idle);
    assert_eq!(after.logs.len(), before.logs.len());
}

#[tokio::test]
async fn disconnect_increments_generation_and_ignores_old_events() {
    let state = ShellState::default();
    let (generation, snapshot) = state.begin_connecting().await;
    assert_eq!(snapshot.connection, AppConnectionState::Connecting);

    let snapshot = state.begin_connection(connected_snapshot()).await;
    assert_eq!(snapshot.connection, AppConnectionState::Connected);

    let (_, disconnected) = state.disconnect().await;
    assert_eq!(disconnected.connection, AppConnectionState::Disconnected);

    let stale = state
        .apply_lv1_event_to_projection(
            generation,
            &Lv1Event::SceneChanged(SceneState {
                index: 9,
                name: "Outro".to_string(),
            }),
        )
        .await;
    assert_eq!(stale, ProjectionOutcome::Stale);
}

#[tokio::test]
async fn manual_disconnect_clears_identities_and_connected_row_status() {
    let state = ShellState::default();
    let connected = crate::connection_state::Lv1SystemIdentity {
        uuid: Some("uuid-1".to_string()),
        host: Some("LV1-FOH".to_string()),
        address: "192.168.1.35".to_string(),
        port: 50000,
    };
    let pending = crate::connection_state::Lv1SystemIdentity {
        uuid: Some("uuid-2".to_string()),
        host: Some("LV1-MON".to_string()),
        address: "192.168.1.36".to_string(),
        port: 50000,
    };
    state
        .set_connected_lv1_identity(Some(connected.clone()))
        .await;
    state.set_pending_lv1_identity(Some(pending.clone())).await;
    state.set_reconnect_active(true).await;
    state
        .set_discovered_lv1_systems(vec![crate::connection_state::DiscoveredLv1System {
            identity: connected,
            latency_ms: Some(10),
            status: crate::connection_state::DiscoveredLv1Status::Available,
        }])
        .await;

    let (_, snapshot) = state.disconnect().await;

    assert_eq!(snapshot.connected_lv1_identity, None);
    assert_eq!(snapshot.pending_lv1_identity, None);
    assert!(!snapshot.reconnect.active);
    assert_ne!(
        snapshot.discovered_lv1_systems[0].status,
        crate::connection_state::DiscoveredLv1Status::Connected
    );
}

#[tokio::test]
async fn fader_event_updates_live_mirror_without_touching_scene_configs() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state
        .begin_connection(connected_state_with_scene_and_channel())
        .await;
    state
        .show
        .replace_snapshot(ShowSnapshot {
            lockout: false,
            scene_configs: vec![scene_config(
                1,
                "Intro",
                vec![ChannelConfig {
                    group: 0,
                    channel: 2,
                    fader_db: Some(-8.0),
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
                vec![ChannelRef {
                    group: 0,
                    channel: 2,
                }],
            )],
        })
        .await;

    assert_eq!(
        state
            .apply_lv1_event_to_projection(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -6.5,
                },
            )
            .await,
        ProjectionOutcome::Applied
    );
    let snapshot = state
        .snapshot_for_generation(generation)
        .await
        .expect("event should apply to current generation");

    assert_eq!(snapshot.connection, AppConnectionState::Connected);
    assert_eq!(
        snapshot.scene_configs[0].channel_configs[0].fader_db,
        Some(-8.0)
    );
    assert_eq!(snapshot.scene_configs[0].scoped_channels.len(), 1);

    let inner = state.inner.lock().await;
    assert_eq!(
        inner.lv1_snapshot.as_ref().unwrap().channels[0].gain_db,
        -6.5
    );
}
