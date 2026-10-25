use super::*;
use crate::cue_lists::{CueList, CueListDocument, CueListsProjectionState};
use crate::fade::{FadeEvent, FadeParameter};
use crate::lv1::{ChannelInfo, Lv1Event, SceneListEntry, SceneObservation, SceneState};
use crate::projector::{AppConnectionState, AppFadeState, LogSeverity};
use crate::runtime::events::{AppEvent, AppEventBus};
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
        let (sink, snapshots) = projection_channel();
        let (logs, log_rx) = broadcast::channel(8);
        let task = spawn_projector(ProjectorInputs {
            sink,
            state: events.state(),
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
    let events = AppEventBus::new(1);
    let AppEvent::Show(mut state) = show_event(true) else {
        unreachable!()
    };
    state.show_file_name = "Seeded Show".into();
    events.publish(AppEvent::Show(state));
    events.publish_runtime_generation_changed(7);
    events.publish_lv1(7, Lv1Event::Connected);
    for (index, name) in [(1, "Earlier scene"), (2, "Latest scene")] {
        events.publish_lv1(
            7,
            Lv1Event::SceneChanged(SceneObservation {
                sequence: index as u64,
                scene: SceneState {
                    index,
                    name: name.into(),
                },
            }),
        );
    }
    events.publish_lv1(
        7,
        Lv1Event::SceneListChanged(vec![SceneListEntry {
            index: 2,
            name: "Latest scene".into(),
        }]),
    );
    events.publish_lv1(
        7,
        Lv1Event::ChannelTopologyChanged(vec![ChannelInfo {
            group: 1,
            channel: 2,
            name: "Vox".into(),
            gain_db: -5.0,
            muted: false,
            pan: None,
            balance: None,
            width: None,
            pan_mode: None,
        }]),
    );
    events.publish_fade(7, FadeEvent::FadeStarted);
    for sequence in 1..8 {
        events.publish_lv1(7, Lv1Event::PingReceived { sequence });
        events.publish_lv1(
            7,
            Lv1Event::FaderChanged {
                group: 1,
                channel: 2,
                gain_db: -3.0,
            },
        );
    }
    let mut test = ProjectorTest::new(events);
    let snapshot = test.snapshot().await;
    assert!(snapshot.lockout);
    assert_eq!(snapshot.show_file_name, "Seeded Show");
    assert_eq!(snapshot.connection, AppConnectionState::Connected);
    assert_eq!(snapshot.current_scene.unwrap().name, "Latest scene");
    assert_eq!(snapshot.scenes[0].name, "Latest scene");
    assert_eq!(snapshot.channels[0].name, "Vox");
    assert_eq!(snapshot.fade_state, AppFadeState::Running);
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

#[tokio::test(start_paused = true)]
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

    // Overflow only the log input; already cached entries and retained state remain available.
    for index in 0..16 {
        test.logs
            .send(UiLogEvent {
                severity: LogSeverity::Info,
                message: format!("burst log {index}"),
            })
            .unwrap();
    }
    test.events.publish(show_event(true));
    let after_lag = test.snapshot().await;
    assert!(after_lag.lockout);
    assert_eq!(after_lag.logs.first().unwrap().message, "projected log");
    assert_eq!(after_lag.logs.last().unwrap().message, "burst log 15");
    assert!(after_lag.state_version > snapshot.state_version);
}

#[tokio::test(start_paused = true)]
async fn non_material_facts_do_not_emit_another_snapshot() {
    let mut test = ProjectorTest::new(AppEventBus::default());
    test.events.publish_lv1(0, Lv1Event::Connected);
    test.snapshot().await;
    for event in [
        Lv1Event::Connected,
        Lv1Event::PingReceived { sequence: 1 },
        Lv1Event::FaderChanged {
            group: 1,
            channel: 1,
            gain_db: -1.0,
        },
        Lv1Event::MuteChanged {
            group: 1,
            channel: 1,
            muted: true,
        },
        Lv1Event::PanChanged {
            group: 1,
            channel: 1,
            pan: 0.5,
        },
        Lv1Event::BalanceChanged {
            group: 1,
            channel: 1,
            balance: 0.25,
        },
        Lv1Event::WidthChanged {
            group: 1,
            channel: 1,
            width: 0.75,
        },
    ] {
        test.events.publish_lv1(0, event);
    }
    test.events.publish_fade(
        0,
        FadeEvent::ChannelCompleted {
            group: 1,
            channel: 1,
            parameter: FadeParameter::FaderDb,
        },
    );
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

#[tokio::test(start_paused = true)]
async fn changes_before_the_next_tick_are_coalesced_into_the_latest_snapshot() {
    let mut test = ProjectorTest::new(AppEventBus::default());
    let first = test.snapshot().await;
    test.events.publish(show_event(true));
    test.events.publish(show_event(false));
    test.events.publish(show_event(true));
    test.events.publish_lv1(0, Lv1Event::Connected);
    test.events.publish_fade(0, FadeEvent::FadeStarted);

    tokio::time::advance(PROJECTOR_INTERVAL - Duration::from_millis(1)).await;
    tokio::select! {
        biased;
        _ = test.snapshots.changed() => panic!("projector must wait for its interval tick"),
        _ = tokio::task::yield_now() => {}
    }
    let latest = test.snapshot().await;
    assert_eq!(latest.state_version, first.state_version + 1);
    assert!(latest.lockout);
    assert_eq!(latest.connection, AppConnectionState::Connected);
    assert_eq!(latest.fade_state, AppFadeState::Running);
    test.assert_no_snapshot().await;
}

#[tokio::test(start_paused = true)]
async fn operational_bus_overflow_does_not_reset_latest_display_state() {
    let mut test = ProjectorTest::new(AppEventBus::new(1));
    let mut operational_events = test.events.subscribe();
    test.events.publish_lv1(0, Lv1Event::Connected);
    assert_eq!(
        test.snapshot().await.connection,
        AppConnectionState::Connected
    );
    test.events.publish(show_event(true));
    test.events.publish(AppEvent::CueLists(cue_state()));
    test.events.publish_fade(0, FadeEvent::FadeStarted);
    for sequence in 1..8 {
        test.events
            .publish_lv1(0, Lv1Event::PingReceived { sequence });
    }
    assert!(matches!(
        operational_events.try_recv(),
        Err(broadcast::error::TryRecvError::Lagged(_))
    ));
    let snapshot = test.snapshot().await;
    assert_eq!(snapshot.connection, AppConnectionState::Connected);
    assert_eq!(snapshot.fade_state, AppFadeState::Running);
    assert!(snapshot.lockout);
    assert_eq!(snapshot.cue_lists[0].name, "Main");
}

#[tokio::test(start_paused = true)]
async fn generation_changes_clear_live_display_and_ignore_stale_facts() {
    let mut test = ProjectorTest::new(AppEventBus::default());
    test.events.publish(show_event(true));
    test.events.publish_lv1(0, Lv1Event::Connected);
    test.events.publish_lv1(
        0,
        Lv1Event::SceneChanged(SceneObservation {
            sequence: 1,
            scene: SceneState {
                index: 2,
                name: "Previous generation".into(),
            },
        }),
    );
    test.events.publish_fade(0, FadeEvent::FadeStarted);
    let before = test.snapshot().await;
    assert_eq!(before.connection, AppConnectionState::Connected);
    assert_eq!(before.fade_state, AppFadeState::Running);

    test.events.publish_runtime_generation_changed(1);
    test.events.publish_lv1(0, Lv1Event::Connected);
    test.events.publish_fade(0, FadeEvent::FadeStarted);
    let after = test.snapshot().await;
    assert_eq!(after.connection, AppConnectionState::Disconnected);
    assert!(after.current_scene.is_none());
    assert!(after.scenes.is_empty());
    assert!(after.channels.is_empty());
    assert_eq!(after.fade_state, AppFadeState::Idle);
    assert!(after.lockout);
    assert!(after.state_version > before.state_version);

    test.events.publish_lv1(0, Lv1Event::Connected);
    test.events.publish_fade(0, FadeEvent::FadeStarted);
    test.assert_no_snapshot().await;
    test.events.publish_lv1(1, Lv1Event::Connected);
    assert_eq!(
        test.snapshot().await.connection,
        AppConnectionState::Connected
    );
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
