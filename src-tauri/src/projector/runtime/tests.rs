use super::*;
use crate::cue_lists::{CueList, CueListDocument, CueListsProjectionState};
use crate::lv1::Lv1Event;
use crate::projector::LogSeverity;
use crate::runtime::events::AppEventBus;
use crate::scenes::{SceneConfig, ScenesEvent, ScenesProjectionState};
use crate::settings::{AppSettings, SettingsEvent};
use crate::show::ShowState;
use serde_json::Value;
use tauri::{
    Listener,
    test::{MockRuntime, mock_app},
};
use tokio::sync::mpsc;
use uuid::Uuid;

struct ProjectorTest {
    _app: tauri::App<MockRuntime>,
    events: AppEventBus,
    logs: broadcast::Sender<UiLogEvent>,
    snapshots: mpsc::UnboundedReceiver<Value>,
    task: tokio::task::JoinHandle<()>,
}

impl ProjectorTest {
    fn new(events: AppEventBus) -> Self {
        let app = mock_app();
        let handle = app.handle().clone();
        let (sent, snapshots) = mpsc::unbounded_channel();
        handle.listen_any("app-status-changed", move |event| {
            let _ = sent.send(serde_json::from_str::<Value>(event.payload()).unwrap());
        });
        let (logs, log_rx) = broadcast::channel(8);
        let task = spawn_projector(ProjectorInputs {
            app: handle,
            generation: 0,
            state: events.state(),
            runtime_source: crate::lifecycle::AppLifecycle::default().runtime_snapshot_source(),
            events: events.subscribe(),
            logs: log_rx,
        });
        Self {
            _app: app,
            events,
            logs,
            snapshots,
            task,
        }
    }

    async fn snapshot(&mut self) -> Value {
        tokio::time::timeout(Duration::from_secs(1), self.snapshots.recv())
            .await
            .expect("projector should emit a snapshot")
            .unwrap()
    }

    async fn assert_no_snapshot(&mut self) {
        assert!(
            tokio::time::timeout(PROJECTOR_INTERVAL * 2, self.snapshots.recv())
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
    assert_eq!(snapshot["lockout"], true);
    assert_eq!(snapshot["showFileName"], "Seeded Show");
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
        snapshot["logs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["message"] == "projected log")
    );
}

#[tokio::test]
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
    assert_eq!(test.snapshot().await["lockout"], false);
    test.events.publish(show_event(true));
    assert_eq!(test.snapshot().await["lockout"], true);
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
    assert_eq!(snapshot["sceneConfigs"][0]["sceneName"], "Bridge");
    assert_eq!(
        snapshot["selectedSceneInternalId"],
        Uuid::from_u128(1).to_string()
    );
}

#[tokio::test]
async fn cue_state_changes_are_projected() {
    let mut test = ProjectorTest::new(AppEventBus::default());
    test.events.publish(AppEvent::CueLists(cue_state()));
    let snapshot = test.snapshot().await;
    assert_eq!(snapshot["cueLists"][0]["name"], "Main");
    assert_eq!(snapshot["lastCueRecallStatus"], "recalling");
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
    assert_eq!(
        test.snapshot().await["sceneSettingsClipboardAvailable"],
        true
    );
    test.events.publish(AppEvent::SessionReplaced {
        generation: 99,
        scenes: ScenesProjectionState::default(),
        cue_lists: cue_state(),
    });
    let snapshot = test.snapshot().await;
    assert_eq!(snapshot["sceneSettingsClipboardAvailable"], false);
    assert_eq!(snapshot["cueLists"][0]["name"], "Main");
}

#[tokio::test]
async fn unchanged_state_does_not_emit_another_snapshot() {
    let mut test = ProjectorTest::new(AppEventBus::default());
    test.events.publish(show_event(true));
    test.events.publish(show_event(true));
    assert_eq!(test.snapshot().await["lockout"], true);
    test.events.publish(show_event(true));
    test.assert_no_snapshot().await;
}

#[tokio::test]
async fn bus_lag_resets_live_state_without_losing_latest_app_state() {
    let mut test = ProjectorTest::new(AppEventBus::new(1));
    test.events.publish_lv1(0, Lv1Event::Connected);
    assert_eq!(test.snapshot().await["connection"], "connected");
    test.events.publish(show_event(true));
    test.events.publish(AppEvent::CueLists(cue_state()));
    for sequence in 1..8 {
        test.events
            .publish_lv1(0, Lv1Event::PingReceived { sequence });
    }
    let snapshot = test.snapshot().await;
    assert_eq!(snapshot["connection"], "disconnected");
    assert_eq!(snapshot["lockout"], true);
    assert_eq!(snapshot["cueLists"][0]["name"], "Main");
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
    assert_eq!(test.snapshot().await["settings"]["autoSaveSessions"], true);
}
