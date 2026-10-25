use super::*;
use crate::cue_lists::{CueList, CueListDocument, CueListsProjectionState};
use crate::lv1::Lv1Event;
use crate::projector::LogSeverity;
use crate::runtime::events::AppEventBus;
use crate::scenes::{SceneConfig, ScenesEvent, ScenesProjectionState};
use crate::settings::{AppSettings, SettingsEvent};
use crate::show::ShowState;
use uuid::Uuid;

struct ProjectorTest {
    events: AppEventBus,
    logs: broadcast::Sender<UiLogEvent>,
    snapshots: ProjectionSubscription,
    task: tokio::task::JoinHandle<()>,
}

impl ProjectorTest {
    fn new(events: AppEventBus) -> Self {
        Self::with_runtime_source(
            events,
            crate::lifecycle::AppLifecycle::default().runtime_snapshot_source(),
        )
    }

    fn with_runtime_source(
        events: AppEventBus,
        runtime_source: crate::lifecycle::RuntimeSnapshotSource,
    ) -> Self {
        let event_rx = events.subscribe();
        Self::with_event_receiver(events, runtime_source, event_rx)
    }

    fn with_event_receiver(
        events: AppEventBus,
        runtime_source: crate::lifecycle::RuntimeSnapshotSource,
        event_rx: broadcast::Receiver<AppEvent>,
    ) -> Self {
        let (sink, snapshots) = projection_channel();
        let (logs, log_rx) = broadcast::channel(8);
        let task = spawn_projector(ProjectorInputs {
            sink,
            generation: 0,
            state: events.state(),
            runtime_source,
            events: event_rx,
            logs: log_rx,
        });
        Self {
            events,
            logs,
            snapshots,
            task,
        }
    }

    async fn snapshot(&mut self) -> AppViewState {
        tokio::time::timeout(Duration::from_secs(1), self.snapshots.changed())
            .await
            .expect("projector should publish a snapshot")
            .expect("projector should remain available");
        self.snapshots
            .latest()
            .expect("snapshot should be available")
    }

    async fn assert_no_snapshot(&mut self) {
        assert!(
            tokio::time::timeout(PROJECTOR_INTERVAL * 2, self.snapshots.changed())
                .await
                .is_err()
        );
    }
}

impl Drop for ProjectorTest {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn show_event(enabled: bool) -> AppEvent {
    let mut state = ShowState::default();
    state.set_lockout(enabled);
    AppEvent::Show(state.projection_state())
}

fn cue_state() -> CueListsProjectionState {
    CueListsProjectionState {
        document: CueListDocument {
            cue_lists: vec![CueList {
                id: Uuid::from_u128(1),
                name: "Main".into(),
                entries: vec![],
            }],
            active_cue_list_id: Some(Uuid::from_u128(1)),
            cued_cue_entry_id: None,
        },
        current_cue_entry_id: None,
    }
}

#[tokio::test]
async fn projector_starts_from_latest_state_even_when_published_before_subscription() {
    let events = AppEventBus::default();
    let AppEvent::Show(mut state) = show_event(true) else {
        unreachable!()
    };
    state.show_file_name = "Seeded Show".into();
    events.publish(AppEvent::Show(state));
    let mut test = ProjectorTest::new(events);
    let snapshot = test.snapshot().await;
    assert!(snapshot.lockout);
    assert_eq!(snapshot.show_file_name, "Seeded Show");
}

#[tokio::test]
async fn updates_are_retained_for_late_projection_and_survive_generation_change() {
    let events = AppEventBus::default();
    let updates = crate::updates::UpdateState {
        status: crate::updates::UpdateStatus::Available,
        available_version: Some("2026.601.1200".into()),
        current_version: Some("2026.601.1100".into()),
        error: None,
    };
    events.publish(AppEvent::Updates(updates.clone()));
    let mut test = ProjectorTest::new(events);
    assert_eq!(test.snapshot().await.updates, updates);
    test.events.publish_runtime_generation_changed(9);
    assert_eq!(test.snapshot().await.updates, updates);
    let ready = crate::updates::UpdateState {
        status: crate::updates::UpdateStatus::Ready,
        ..updates
    };
    test.events.publish(AppEvent::Updates(ready.clone()));
    assert_eq!(test.snapshot().await.updates, ready);
}

#[tokio::test]
async fn projector_emits_ui_log_entries_from_log_input() {
    let mut test = ProjectorTest::new(AppEventBus::default());
    test.logs
        .send(UiLogEvent {
            severity: LogSeverity::Warning,
            message: "projected log".into(),
        })
        .unwrap();
    let snapshot = test.snapshot().await;
    assert!(
        snapshot
            .logs
            .iter()
            .any(|entry| entry.message == "projected log")
    );
}

#[tokio::test(start_paused = true)]
async fn ping_event_does_not_emit_app_status_changed() {
    let mut test = ProjectorTest::new(AppEventBus::default());
    test.snapshot().await;
    test.events
        .publish_lv1(0, Lv1Event::PingReceived { sequence: 1 });
    test.assert_no_snapshot().await;
}

#[tokio::test]
async fn show_state_changes_are_projected() {
    let mut test = ProjectorTest::new(AppEventBus::default());
    assert!(!test.snapshot().await.lockout);
    test.events.publish(show_event(true));
    assert!(test.snapshot().await.lockout);
}

#[tokio::test]
async fn scene_state_changes_are_projected() {
    let mut test = ProjectorTest::new(AppEventBus::default());
    test.events.publish(AppEvent::Scenes {
        generation: 0,
        event: ScenesEvent::StateChanged {
            state: ScenesProjectionState {
                scene_configs: vec![SceneConfig {
                    internal_scene_id: Uuid::from_u128(1),
                    scene_index: Some(8),
                    scene_name: "Bridge".into(),
                    duration_ms: 2_000,
                    channel_configs: vec![],
                    scoped_channels: vec![],
                    scope_toggles: Default::default(),
                }],
                selected_scene_internal_id: Some(Uuid::from_u128(1)),
                ..Default::default()
            },
            persisted_scene_edit: false,
        },
    });
    let snapshot = test.snapshot().await;
    assert_eq!(snapshot.scene_configs[0].scene_name, "Bridge");
    assert_eq!(
        snapshot.selected_scene_internal_id.unwrap(),
        Uuid::from_u128(1)
    );
}

#[tokio::test]
async fn cue_state_changes_are_projected() {
    let mut test = ProjectorTest::new(AppEventBus::default());
    test.events.publish(AppEvent::CueLists(cue_state()));
    let snapshot = test.snapshot().await;
    assert_eq!(snapshot.cue_lists[0].name, "Main");
}

#[tokio::test]
async fn session_replacement_projects_scenes_and_cues_together() {
    let mut test = ProjectorTest::new(AppEventBus::default());
    test.events.publish(AppEvent::Scenes {
        generation: 0,
        event: ScenesEvent::StateChanged {
            state: ScenesProjectionState {
                scene_settings_clipboard_available: true,
                ..Default::default()
            },
            persisted_scene_edit: false,
        },
    });
    assert!(test.snapshot().await.scene_settings_clipboard_available);
    test.events.publish(AppEvent::SessionReplaced {
        generation: 99,
        scenes: ScenesProjectionState::default(),
        cue_lists: cue_state(),
    });
    let snapshot = test.snapshot().await;
    assert!(!snapshot.scene_settings_clipboard_available);
    assert_eq!(snapshot.cue_lists[0].name, "Main");
    assert_eq!(snapshot.session_revision, 1);
}

#[tokio::test(start_paused = true)]
async fn unchanged_state_does_not_emit_another_snapshot() {
    let mut test = ProjectorTest::new(AppEventBus::default());
    test.events.publish(show_event(true));
    test.events.publish(show_event(true));
    assert!(test.snapshot().await.lockout);
    test.events.publish(show_event(true));
    test.assert_no_snapshot().await;
}

#[tokio::test]
async fn bus_lag_resets_live_state_without_losing_latest_app_state() {
    let mut test = ProjectorTest::new(AppEventBus::new(1));
    test.events.publish_lv1(0, Lv1Event::Connected);
    assert_eq!(
        test.snapshot().await.connection,
        crate::projector::AppConnectionState::Connected
    );
    test.events.publish(show_event(true));
    test.events.publish(AppEvent::CueLists(cue_state()));
    for sequence in 1..8 {
        test.events
            .publish_lv1(0, Lv1Event::PingReceived { sequence });
    }
    let snapshot = test.snapshot().await;
    assert_eq!(
        snapshot.connection,
        crate::projector::AppConnectionState::Disconnected
    );
    assert!(snapshot.lockout);
    assert_eq!(snapshot.cue_lists[0].name, "Main");
}

#[tokio::test]
async fn lag_recovery_keeps_log_input_responsive_and_replays_post_cutoff_facts() {
    let events = AppEventBus::new(1);
    let (lv1, mut commands) = tokio::sync::mpsc::channel(1);
    let runtime_source = crate::lifecycle::RuntimeSnapshotSource::with_lv1_for_test(
        crate::lv1::test_actor_handle(lv1),
    );
    let mut test = ProjectorTest::with_runtime_source(events, runtime_source);
    test.snapshot().await;

    test.events.publish_lv1(0, Lv1Event::Connected);
    test.events
        .publish_lv1(0, Lv1Event::PingReceived { sequence: 1 });
    let Some(Lv1Command::GetState { reply }) = commands.recv().await else {
        panic!("lag recovery should request authoritative LV1 state");
    };

    test.events.publish_lv1(0, Lv1Event::Connected);
    test.logs
        .send(UiLogEvent {
            severity: LogSeverity::Info,
            message: "during recovery".into(),
        })
        .unwrap();
    let snapshot = test.snapshot().await;
    assert!(
        snapshot
            .logs
            .iter()
            .any(|log| log.message == "during recovery")
    );

    reply
        .send(crate::lv1::Lv1StateSnapshot {
            connection: ConnectionStatus::Disconnected,
            scene: None,
            scene_list: None,
            channels: vec![],
            ping_sequence: 0,
        })
        .unwrap();
    let snapshot = test.snapshot().await;
    assert_eq!(
        snapshot.connection,
        crate::projector::AppConnectionState::Connected
    );
}

#[test]
fn recovery_result_revalidates_current_generation_before_applying_snapshot() {
    let mut cache = ProjectionCache::new();
    cache.set_active_generation(1);
    let result = ProjectorRecoveryResult {
        snapshot: Some((
            1,
            crate::lv1::Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: vec![].into(),
                channels: vec![],
                ping_sequence: 0,
            },
        )),
        timed_out: false,
    };

    apply_recovery_result(&mut cache, result, 2);

    let snapshot = cache.build_snapshot(&Default::default());
    assert_eq!(
        snapshot.connection,
        crate::projector::AppConnectionState::Disconnected
    );
}

fn recovery_snapshot(name: &str) -> crate::lv1::Lv1StateSnapshot {
    crate::lv1::Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: Some(crate::lv1::SceneState {
            index: 1,
            name: name.into(),
        }),
        scene_list: Some(vec![crate::lv1::SceneListEntry {
            index: 1,
            name: name.into(),
        }]),
        channels: vec![],
        ping_sequence: 0,
    }
}

async fn next_recovery_reply(
    commands: &mut tokio::sync::mpsc::Receiver<Lv1Command>,
) -> tokio::sync::oneshot::Sender<crate::lv1::Lv1StateSnapshot> {
    let command = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("projector should request authoritative LV1 state");
    let Some(Lv1Command::GetState { reply }) = command else {
        panic!("recovery should request an LV1 snapshot");
    };
    reply
}

async fn drain_projector_input(facts: &broadcast::Sender<AppEvent>) {
    // This transport has exactly one receiver, owned by the running projector. Waiting for its
    // public channel to drain prevents the next batch from introducing another broadcast lag.
    let started = std::time::Instant::now();
    while !facts.is_empty() {
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "projector should consume its input while recovery is pending"
        );
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn recovery_queue_capacity_replays_facts_and_overflow_requeries_at_a_new_cutoff() {
    for overflow in [false, true] {
        let (lv1, mut commands) = tokio::sync::mpsc::channel(1);
        let runtime_source = crate::lifecycle::RuntimeSnapshotSource::with_lv1_for_test(
            crate::lv1::test_actor_handle(lv1),
        );
        let (facts, event_rx) = broadcast::channel(64);
        let mut test =
            ProjectorTest::with_event_receiver(AppEventBus::default(), runtime_source, event_rx);
        test.snapshot().await;
        // Deliberately lag once to start recovery, before sending the controlled batches below.
        for sequence in 0..65 {
            facts
                .send(AppEvent::Lv1 {
                    generation: 0,
                    event: Lv1Event::PingReceived { sequence },
                })
                .unwrap();
        }
        let old_reply = next_recovery_reply(&mut commands).await;

        for sequence in 0..MAX_PENDING_RECOVERY_EVENTS {
            let event = match sequence {
                0 => AppEvent::Lv1 {
                    generation: 0,
                    event: Lv1Event::SceneListChanged(vec![crate::lv1::SceneListEntry {
                        index: 2,
                        name: "Before cutoff".into(),
                    }]),
                },
                1 => AppEvent::Fade {
                    generation: 0,
                    event: crate::fade::FadeEvent::FadeStarted,
                },
                _ => AppEvent::Lv1 {
                    generation: 0,
                    event: Lv1Event::PingReceived {
                        sequence: sequence as u64,
                    },
                },
            };
            facts.send(event).unwrap();
            if (sequence + 1) % 64 == 0 {
                drain_projector_input(&facts).await;
            }
        }
        drain_projector_input(&facts).await;
        if overflow {
            facts
                .send(AppEvent::Lv1 {
                    generation: 0,
                    event: Lv1Event::SceneChanged(crate::lv1::SceneObservation {
                        sequence: 1,
                        scene: crate::lv1::SceneState {
                            index: 3,
                            name: "After cutoff".into(),
                        },
                    }),
                })
                .unwrap();
            drain_projector_input(&facts).await;
        }

        old_reply
            .send(recovery_snapshot("Initial recovery"))
            .unwrap();
        if overflow {
            let reply = next_recovery_reply(&mut commands).await;
            let pending_snapshot = test.snapshot().await;
            assert_eq!(
                pending_snapshot.connection,
                crate::projector::AppConnectionState::Disconnected
            );
            assert!(pending_snapshot.current_scene.is_none());
            assert_eq!(
                pending_snapshot.fade_state,
                crate::projector::AppFadeState::Idle
            );
            reply.send(recovery_snapshot("Fresh recovery")).unwrap();
        }

        let snapshot = test.snapshot().await;
        assert_eq!(
            snapshot.connection,
            crate::projector::AppConnectionState::Connected
        );
        if overflow {
            assert_eq!(snapshot.current_scene.unwrap().name, "After cutoff");
            assert_eq!(snapshot.scenes[0].name, "Fresh recovery");
            assert_eq!(snapshot.fade_state, crate::projector::AppFadeState::Idle);
        } else {
            assert_eq!(snapshot.current_scene.unwrap().name, "Initial recovery");
            assert_eq!(snapshot.scenes[0].name, "Before cutoff");
            assert_eq!(snapshot.fade_state, crate::projector::AppFadeState::Running);
        }
        assert!(matches!(
            commands.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
    }
}

#[tokio::test(start_paused = true)]
async fn recovery_timeout_projects_disconnected_idle_and_preserves_app_state_and_logs() {
    let (lv1, mut commands) = tokio::sync::mpsc::channel(1);
    let runtime_source = crate::lifecycle::RuntimeSnapshotSource::with_lv1_for_test(
        crate::lv1::test_actor_handle(lv1),
    );
    let events = AppEventBus::new(1);
    events.publish(show_event(true));
    events.publish(AppEvent::CueLists(cue_state()));
    let mut test = ProjectorTest::with_runtime_source(events, runtime_source);
    test.snapshot().await;
    test.events.publish_lv1(0, Lv1Event::Connected);
    test.snapshot().await;
    test.events
        .publish_fade(0, crate::fade::FadeEvent::FadeStarted);
    test.logs
        .send(UiLogEvent {
            severity: LogSeverity::Info,
            message: "before recovery".into(),
        })
        .unwrap();
    let before = test.snapshot().await;
    assert_eq!(
        before.connection,
        crate::projector::AppConnectionState::Connected
    );
    assert_eq!(before.fade_state, crate::projector::AppFadeState::Running);

    for sequence in 0..2 {
        test.events
            .publish_lv1(0, Lv1Event::PingReceived { sequence });
    }
    let mut reply = next_recovery_reply(&mut commands).await;
    let AppEvent::Show(mut show) = show_event(true) else {
        unreachable!()
    };
    show.show_file_name = "Recovery session".into();
    test.events.publish(AppEvent::Show(show));
    test.logs
        .send(UiLogEvent {
            severity: LogSeverity::Warning,
            message: "during recovery".into(),
        })
        .unwrap();
    let pending = test.snapshot().await;
    assert_eq!(pending.show_file_name, "Recovery session");
    assert!(
        !reply.is_closed(),
        "request should still be pending before its deadline"
    );

    tokio::time::advance(Duration::from_millis(501)).await;
    tokio::time::timeout(Duration::from_secs(1), reply.closed())
        .await
        .expect("timed-out recovery should discard its pending reply");
    let snapshot = test.snapshot().await;
    assert!(snapshot.state_version > pending.state_version);
    assert_eq!(
        snapshot.connection,
        crate::projector::AppConnectionState::Disconnected
    );
    assert_eq!(snapshot.fade_state, crate::projector::AppFadeState::Idle);
    assert!(snapshot.current_scene.is_none());
    assert!(snapshot.lockout);
    assert_eq!(snapshot.show_file_name, "Recovery session");
    assert_eq!(snapshot.cue_lists[0].name, "Main");
    assert_eq!(
        snapshot
            .logs
            .iter()
            .map(|log| log.message.as_str())
            .collect::<Vec<_>>(),
        vec!["before recovery", "during recovery"]
    );
    assert!(reply.send(recovery_snapshot("Too late")).is_err());
}

#[tokio::test]
async fn settings_state_changes_are_projected() {
    let mut test = ProjectorTest::new(AppEventBus::default());
    test.events
        .publish(AppEvent::Settings(SettingsEvent::StateChanged {
            settings: AppSettings {
                auto_save_sessions: true,
                asc_recall_interval_ms: 1_200,
                ..Default::default()
            },
        }));
    let settings = test.snapshot().await.settings;
    assert!(settings.auto_save_sessions);
    assert_eq!(settings.asc_recall_interval_ms, 1_200);
}
