use std::time::Duration;

use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::broadcast;

use crate::logging::UiLogEvent;
use crate::projector::AppViewState;
use crate::runtime::events::log_lagged_subscriber;
use crate::runtime::events::{AppEvent, RuntimeLifecycleEvent};
use crate::scenes::{ScenesEvent, ScenesProjectionState};
use crate::settings::{AppSettings, SettingsEvent};
use crate::show::{ShowEvent, ShowProjectionState};

use super::ProjectionCache;

pub const PROJECTOR_INTERVAL: Duration = Duration::from_millis(100);

pub struct ProjectorInputs<R: Runtime> {
    pub app: AppHandle<R>,
    pub generation: u64,
    pub initial_show_state: ShowProjectionState,
    pub initial_scenes_state: ScenesProjectionState,
    pub initial_settings: AppSettings,
    pub events: broadcast::Receiver<AppEvent>,
    pub logs: broadcast::Receiver<UiLogEvent>,
}

pub fn spawn_projector<R: Runtime>(inputs: ProjectorInputs<R>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let ProjectorInputs {
            app,
            generation,
            initial_show_state,
            initial_scenes_state,
            initial_settings,
            mut events,
            mut logs,
        } = inputs;

        tracing::debug!(
            event = "projector_started",
            generation = generation,
            "projector started"
        );

        let mut cache = ProjectionCache::new();
        cache.set_active_generation(generation);
        cache.apply_show_state(initial_show_state);
        cache.apply_scenes_state(initial_scenes_state);
        cache.apply_settings(initial_settings);
        let mut interval = tokio::time::interval(PROJECTOR_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        let mut dirty = true;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if dirty {
                        let snapshot = cache.build_snapshot();
                        emit_app_status(&app, &snapshot);
                        dirty = false;
                    }
                }
                received = events.recv() => {
                    match received {
                        Ok(app_event) => {
                            if apply_projector_event(&mut cache, &app_event) {
                                dirty = true;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(count)) => {
                            dirty = true;
                            log_lagged_subscriber("projector", count);
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                received = logs.recv() => {
                    match received {
                        Ok(ui_log) => {
                            cache.append_log(ui_log);
                            dirty = true;
                        }
                        Err(broadcast::error::RecvError::Lagged(_count)) => {
                            dirty = true;
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    })
}

fn emit_app_status<R: Runtime>(app: &AppHandle<R>, snapshot: &AppViewState) {
    if let Err(err) = app.emit("app-status-changed", snapshot) {
        tracing::debug!(
            event = "projector_emit_failed",
            error = %err,
            "failed to emit app-status-changed from projector"
        );
    }
}

fn apply_projector_event(cache: &mut ProjectionCache, event: &AppEvent) -> bool {
    match event {
        AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation }) => {
            cache.set_active_generation(*generation);
            true
        }
        AppEvent::Lv1 { generation, event } => cache.apply_lv1_event(*generation, event),
        AppEvent::Fade { generation, event } => cache.apply_fade_event(*generation, event),
        AppEvent::Scenes { generation, event } => match event {
            ScenesEvent::StateChanged { state, .. } => {
                if !cache.is_active_generation(*generation) {
                    return false;
                }
                cache.apply_scenes_state(state.clone());
                true
            }
            _ => false,
        },
        AppEvent::Show(ShowEvent::StateChanged { state, .. }) => {
            cache.apply_show_state(state.clone());
            true
        }
        AppEvent::Settings(SettingsEvent::StateChanged { settings }) => {
            cache.apply_settings(settings.clone());
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projector::LogSeverity;
    use crate::runtime::events::AppEventBus;
    use crate::show::{ShowEvent, ShowProjectionReason, ShowProjectionState};
    use std::sync::{Arc, Mutex};
    use tauri::{Listener, test::mock_app};

    fn spawn_started_projector(
        handle: AppHandle<impl Runtime>,
        generation: u64,
        events: broadcast::Receiver<AppEvent>,
        logs: broadcast::Receiver<UiLogEvent>,
    ) -> tokio::task::JoinHandle<()> {
        spawn_projector(ProjectorInputs {
            app: handle,
            generation,
            initial_show_state: ShowProjectionState {
                lockout: false,
                show_file_path: None,
                show_file_name: "Untitled Session".to_string(),
                show_file_dirty: false,
                show_file_last_saved_at: None,
                discovered_lv1_systems: Vec::new(),
                connected_lv1_identity: None,
                pending_lv1_identity: None,
                reconnect: Default::default(),
                last_event_at: None,
            },
            initial_scenes_state: ScenesProjectionState {
                scene_configs: Vec::new(),
                cued_scene_internal_id: None,
                selected_scene_internal_id: None,
            },
            initial_settings: AppSettings::default(),
            events,
            logs,
        })
    }

    #[tokio::test]
    async fn projector_emits_ui_log_entries_from_log_input() {
        let app = mock_app();
        let handle = app.handle().clone();
        let event_bus = AppEventBus::default();
        let (log_tx, log_rx) = broadcast::channel(8);
        let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let received_events = received.clone();
        handle.listen_any("app-status-changed", move |event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            received_events.lock().unwrap().push(payload);
        });

        let projector = spawn_started_projector(handle, 0, event_bus.subscribe(), log_rx);

        log_tx
            .send(UiLogEvent {
                severity: LogSeverity::Warning,
                message: "projected log".to_string(),
            })
            .unwrap();
        tokio::time::sleep(PROJECTOR_INTERVAL + Duration::from_millis(60)).await;

        projector.abort();
        let snapshots = received.lock().unwrap();
        assert!(snapshots.iter().any(|snapshot| {
            snapshot["logs"]
                .as_array()
                .is_some_and(|logs| logs.iter().any(|entry| entry["message"] == "projected log"))
        }));
    }

    #[tokio::test]
    async fn show_event_marks_cache_dirty_and_pulls_show_snapshot() {
        let app = mock_app();
        let handle = app.handle().clone();
        let event_bus = AppEventBus::default();
        let (_log_tx, log_rx) = broadcast::channel(8);
        let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let received_events = received.clone();
        handle.listen_any("app-status-changed", move |event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            received_events.lock().unwrap().push(payload);
        });

        let projector = spawn_started_projector(handle, 0, event_bus.subscribe(), log_rx);

        event_bus.publish(AppEvent::Show(ShowEvent::StateChanged {
            reason: ShowProjectionReason::FileMetadata,
            state: ShowProjectionState {
                lockout: true,
                show_file_path: None,
                show_file_name: "Untitled Session".to_string(),
                show_file_dirty: false,
                show_file_last_saved_at: None,
                discovered_lv1_systems: vec![],
                connected_lv1_identity: None,
                pending_lv1_identity: None,
                reconnect: Default::default(),
                last_event_at: None,
            },
        }));
        tokio::time::sleep(PROJECTOR_INTERVAL + Duration::from_millis(60)).await;

        projector.abort();
        let snapshots = received.lock().unwrap();
        assert!(snapshots.iter().any(|snapshot| snapshot["lockout"] == true));
    }

    #[tokio::test]
    async fn scenes_event_marks_cache_dirty_and_projects_scene_configs() {
        let app = mock_app();
        let handle = app.handle().clone();
        let event_bus = AppEventBus::default();
        let (_log_tx, log_rx) = broadcast::channel(8);
        let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let received_events = received.clone();
        handle.listen_any("app-status-changed", move |event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            received_events.lock().unwrap().push(payload);
        });

        let projector = spawn_started_projector(handle, 0, event_bus.subscribe(), log_rx);

        event_bus.publish(AppEvent::Scenes {
            generation: 0,
            event: crate::scenes::ScenesEvent::StateChanged {
                reason: crate::scenes::ScenesProjectionReason::SceneState,
                state: ScenesProjectionState {
                    scene_configs: vec![crate::scenes::SceneConfig {
                        internal_scene_id: uuid::Uuid::from_u128(
                            0x11111111111141118111111111111111,
                        ),
                        scene_index: Some(8),
                        scene_name: "Bridge".to_string(),
                        duration_ms: 2_000,
                        channel_configs: vec![],
                        scoped_channels: vec![],
                        scope_toggles: Default::default(),
                    }],
                    cued_scene_internal_id: Some("cue-id".to_string()),
                    selected_scene_internal_id: Some("selected-id".to_string()),
                },
                persisted_scene_edit: false,
            },
        });
        tokio::time::sleep(PROJECTOR_INTERVAL + Duration::from_millis(60)).await;

        projector.abort();
        let snapshots = received.lock().unwrap();
        assert!(snapshots.iter().any(|snapshot| {
            snapshot["sceneConfigs"]
                .as_array()
                .is_some_and(|scenes| scenes.iter().any(|scene| scene["sceneName"] == "Bridge"))
        }));
    }

    #[tokio::test]
    async fn projector_emits_initial_show_state() {
        let app = mock_app();
        let handle = app.handle().clone();
        let event_bus = AppEventBus::default();
        let (_log_tx, log_rx) = broadcast::channel(8);
        let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let received_events = received.clone();
        handle.listen_any("app-status-changed", move |event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            received_events.lock().unwrap().push(payload);
        });

        let projector = spawn_projector(ProjectorInputs {
            app: handle,
            generation: 0,
            initial_show_state: ShowProjectionState {
                lockout: true,
                show_file_path: None,
                show_file_name: "Seeded Show".to_string(),
                show_file_dirty: false,
                show_file_last_saved_at: None,
                discovered_lv1_systems: Vec::new(),
                connected_lv1_identity: None,
                pending_lv1_identity: None,
                reconnect: Default::default(),
                last_event_at: None,
            },
            initial_scenes_state: ScenesProjectionState {
                scene_configs: Vec::new(),
                cued_scene_internal_id: None,
                selected_scene_internal_id: None,
            },
            initial_settings: AppSettings::default(),
            events: event_bus.subscribe(),
            logs: log_rx,
        });

        tokio::time::sleep(PROJECTOR_INTERVAL + Duration::from_millis(60)).await;

        projector.abort();
        let snapshots = received.lock().unwrap();
        assert!(snapshots.iter().any(|snapshot| snapshot["lockout"] == true));
    }

    #[tokio::test]
    async fn unchanged_events_are_coalesced_into_one_snapshot_per_tick() {
        let app = mock_app();
        let handle = app.handle().clone();
        let event_bus = AppEventBus::default();
        let (_log_tx, log_rx) = broadcast::channel(8);
        let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let received_events = received.clone();
        handle.listen_any("app-status-changed", move |event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            received_events.lock().unwrap().push(payload);
        });

        let projector = spawn_started_projector(handle, 0, event_bus.subscribe(), log_rx);

        let event = AppEvent::Show(ShowEvent::StateChanged {
            reason: ShowProjectionReason::FileMetadata,
            state: ShowProjectionState {
                lockout: true,
                show_file_path: None,
                show_file_name: "Untitled Session".to_string(),
                show_file_dirty: false,
                show_file_last_saved_at: None,
                discovered_lv1_systems: vec![],
                connected_lv1_identity: None,
                pending_lv1_identity: None,
                reconnect: Default::default(),
                last_event_at: None,
            },
        });
        event_bus.publish(event.clone());
        event_bus.publish(event);

        tokio::time::sleep(PROJECTOR_INTERVAL + Duration::from_millis(60)).await;
        tokio::time::sleep(PROJECTOR_INTERVAL + Duration::from_millis(60)).await;

        projector.abort();
        let snapshots = received.lock().unwrap();
        assert_eq!(snapshots.len(), 1);
    }

    #[tokio::test]
    async fn settings_event_marks_cache_dirty_and_projects_settings() {
        let app = mock_app();
        let handle = app.handle().clone();
        let event_bus = AppEventBus::default();
        let (_log_tx, log_rx) = broadcast::channel(8);
        let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let received_events = received.clone();
        handle.listen_any("app-status-changed", move |event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            received_events.lock().unwrap().push(payload);
        });

        let projector = spawn_started_projector(handle, 0, event_bus.subscribe(), log_rx);

        event_bus.publish(AppEvent::Settings(SettingsEvent::StateChanged {
            settings: AppSettings {
                auto_save_sessions: true,
                ..Default::default()
            },
        }));
        tokio::time::sleep(PROJECTOR_INTERVAL + Duration::from_millis(60)).await;

        projector.abort();
        let snapshots = received.lock().unwrap();
        assert!(
            snapshots
                .iter()
                .any(|snapshot| { snapshot["settings"]["autoSaveSessions"] == true })
        );
    }
}
