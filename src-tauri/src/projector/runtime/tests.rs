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
        let (sink, snapshots) = projection_channel();
        let (logs, log_rx) = broadcast::channel(8);
        let task = spawn_projector(ProjectorInputs {
            sink,
            generation: 0,
            state: events.state(),
            runtime_source: crate::lifecycle::AppLifecycle::default().runtime_snapshot_source(),
            events: events.subscribe(),
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
        last_recall_status: Some("recalling".into()),
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
                selected_scene_internal_id: Some(Uuid::from_u128(1).to_string()),
                ..Default::default()
            },
            persisted_scene_edit: false,
        },
    });
    let snapshot = test.snapshot().await;
    assert_eq!(snapshot.scene_configs[0].scene_name, "Bridge");
    assert_eq!(
        snapshot.selected_scene_internal_id.unwrap(),
        Uuid::from_u128(1).to_string()
    );
}

#[tokio::test]
async fn cue_state_changes_are_projected() {
    let mut test = ProjectorTest::new(AppEventBus::default());
    test.events.publish(AppEvent::CueLists(cue_state()));
    let snapshot = test.snapshot().await;
    assert_eq!(snapshot.cue_lists[0].name, "Main");
    assert_eq!(snapshot.last_cue_recall_status.unwrap(), "recalling");
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
async fn settings_state_changes_are_projected() {
    let mut test = ProjectorTest::new(AppEventBus::default());
    test.events
        .publish(AppEvent::Settings(SettingsEvent::StateChanged {
            settings: AppSettings {
                auto_save_sessions: true,
                ..Default::default()
            },
        }));
    assert!(test.snapshot().await.settings.auto_save_sessions);
}
