use std::time::Duration;

use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::broadcast;

use crate::cue_lists::{CueListsCommand, CueListsEvent, CueListsHandle, CueListsProjectionState};
use crate::lifecycle::RuntimeSnapshotSource;
use crate::logging::UiLogEvent;
use crate::lv1::{ConnectionStatus, Lv1Command};
use crate::projector::AppViewState;
use crate::runtime::events::log_lagged_subscriber;
use crate::runtime::events::{AppEvent, RuntimeLifecycleEvent};
use crate::scenes::{ScenesCommand, ScenesEvent, ScenesHandle, ScenesProjectionState};
use crate::settings::{AppSettings, SettingsCommand, SettingsEvent, SettingsHandle};
use crate::show::{ShowCommand, ShowEvent, ShowProjectionState, ShowStateHandle};

use super::ProjectionCache;

pub const PROJECTOR_INTERVAL: Duration = Duration::from_millis(100);

pub struct ProjectorInputs<R: Runtime> {
    pub app: AppHandle<R>,
    pub generation: u64,
    pub initial_show_state: ShowProjectionState,
    pub initial_scenes_state: ScenesProjectionState,
    pub initial_cue_lists_state: CueListsProjectionState,
    pub initial_settings: AppSettings,
    pub runtime_source: RuntimeSnapshotSource,
    pub show: ShowStateHandle,
    pub scenes: ScenesHandle,
    pub cue_lists: CueListsHandle,
    pub settings: SettingsHandle,
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
            initial_cue_lists_state,
            initial_settings,
            runtime_source,
            show,
            scenes,
            cue_lists,
            settings,
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
        cache.apply_cue_lists_state(initial_cue_lists_state);
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
                            log_lagged_subscriber("projector", count);
                            drain_retained_events(&mut events);
                            if !recover_projector_after_lag(
                                &mut cache,
                                &runtime_source,
                                &show,
                                &scenes,
                                &cue_lists,
                                &settings,
                            )
                            .await
                            {
                                tracing::warn!(
                                    event = "projector_resync_timeout",
                                    "Projector resynchronization timed out; showing disconnected state"
                                );
                            }
                            dirty = true;
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

fn drain_retained_events(events: &mut broadcast::Receiver<AppEvent>) {
    loop {
        match events.try_recv() {
            Ok(_) => {}
            Err(broadcast::error::TryRecvError::Empty) => break,
            Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
            Err(broadcast::error::TryRecvError::Closed) => break,
        }
    }
}

async fn recover_projector_after_lag(
    cache: &mut ProjectionCache,
    runtime_source: &RuntimeSnapshotSource,
    show: &ShowStateHandle,
    scenes: &ScenesHandle,
    cue_lists: &CueListsHandle,
    settings: &SettingsHandle,
) -> bool {
    let recovery = tokio::time::timeout(
        Duration::from_millis(500),
        recover_projector_after_lag_inner(cache, runtime_source, show, scenes, cue_lists, settings),
    )
    .await;
    if recovery.is_err() {
        let Ok(generation) = tokio::time::timeout(
            Duration::from_millis(50),
            runtime_source.current_generation(),
        )
        .await
        else {
            cache.reset_generation_scoped_state();
            return false;
        };
        cache.reset_for_generation(generation);
        return false;
    }
    true
}

async fn recover_projector_after_lag_inner(
    cache: &mut ProjectionCache,
    runtime_source: &RuntimeSnapshotSource,
    show: &ShowStateHandle,
    scenes: &ScenesHandle,
    cue_lists: &CueListsHandle,
    settings: &SettingsHandle,
) {
    let runtime_snapshot = runtime_source.connected_lv1().await;
    let authoritative_snapshot = if let Some((snapshot_generation, lv1)) = runtime_snapshot {
        let (reply, response) = tokio::sync::oneshot::channel();
        if lv1.send(Lv1Command::GetState { reply }).await.is_ok() {
            let snapshot = response.await.ok();
            if snapshot_generation == runtime_source.current_generation().await {
                snapshot
                    .filter(|snapshot| snapshot.connection == ConnectionStatus::Connected)
                    .map(|snapshot| (snapshot_generation, snapshot))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    let current_generation = runtime_source.current_generation().await;
    cache.reset_for_generation(current_generation);
    if let Some((snapshot_generation, snapshot)) = authoritative_snapshot
        && snapshot_generation == current_generation
    {
        cache.apply_lv1_snapshot(current_generation, snapshot);
    }
    resync_projector_sources(cache, show, scenes, cue_lists, settings).await;
}

async fn resync_projector_sources(
    cache: &mut ProjectionCache,
    show: &ShowStateHandle,
    scenes: &ScenesHandle,
    cue_lists: &CueListsHandle,
    settings: &SettingsHandle,
) {
    let (show_state, scenes_state, cue_state, settings_state) = tokio::join!(
        query_show_projection(show),
        query_scenes_projection(scenes),
        query_cue_projection(cue_lists),
        query_settings(settings),
    );
    if let Some(state) = show_state {
        cache.apply_show_state(state);
    }
    if let Some(state) = scenes_state {
        cache.apply_scenes_state(state);
    }
    if let Some(state) = cue_state {
        cache.apply_cue_lists_state(state);
    }
    if let Some(state) = settings_state {
        cache.apply_settings(state);
    }
}

async fn query_show_projection(handle: &ShowStateHandle) -> Option<ShowProjectionState> {
    let (reply, response) = tokio::sync::oneshot::channel();
    handle
        .send(ShowCommand::InitialProjectionState { reply })
        .await
        .ok()?;
    response.await.ok()
}

async fn query_scenes_projection(handle: &ScenesHandle) -> Option<ScenesProjectionState> {
    let (reply, response) = tokio::sync::oneshot::channel();
    handle
        .send(ScenesCommand::InitialProjectionState { reply })
        .await
        .ok()?;
    response.await.ok()
}

async fn query_cue_projection(handle: &CueListsHandle) -> Option<CueListsProjectionState> {
    let (reply, response) = tokio::sync::oneshot::channel();
    handle
        .send(CueListsCommand::InitialProjectionState { reply })
        .await
        .ok()?;
    response.await.ok()
}

async fn query_settings(handle: &SettingsHandle) -> Option<AppSettings> {
    let (reply, response) = tokio::sync::oneshot::channel();
    handle
        .send(SettingsCommand::GetSettings { reply })
        .await
        .ok()?;
    response.await.ok()
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
            cache.reset_for_generation(*generation);
            true
        }
        AppEvent::Lv1 { generation, event } => cache.apply_lv1_event(*generation, event),
        AppEvent::Fade { generation, event } => cache.apply_fade_event(*generation, event),
        AppEvent::Scenes {
            generation: _,
            event,
        } => match event {
            ScenesEvent::StateChanged { state, .. } => {
                // Scene document state is app-lifetime; only LV1 facts are generation-bound.
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
        AppEvent::CueLists(CueListsEvent::StateChanged { state, .. }) => {
            cache.apply_cue_lists_state(state.clone());
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::Lv1Event;
    use crate::projector::LogSeverity;
    use crate::runtime::events::AppEventBus;
    use crate::runtime::generation::RuntimeGeneration;
    use crate::show::{ShowEvent, ShowProjectionReason, ShowProjectionState};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tauri::{Listener, test::mock_app};

    fn spawn_started_projector(
        handle: AppHandle<impl Runtime>,
        generation: u64,
        events: broadcast::Receiver<AppEvent>,
        logs: broadcast::Receiver<UiLogEvent>,
    ) -> tokio::task::JoinHandle<()> {
        let source_events = AppEventBus::default();
        let (show, show_task, _show_peers, lockout) =
            crate::show::build_show_actor(source_events.clone());
        let (settings, settings_task, initial_settings) =
            crate::settings::build_settings_actor(std::env::temp_dir(), source_events.clone());
        let runtime_generation = crate::runtime::generation::RuntimeGeneration::new();
        let runtime_source = crate::lifecycle::AppLifecycle::default().runtime_snapshot_source();
        let (scenes, scenes_task, _scene_peers) = crate::scenes::build_scenes_actor(
            generation,
            runtime_generation.clone(),
            source_events.clone(),
            source_events.subscribe(),
            settings.clone(),
            initial_settings.clone(),
            lockout,
        );
        let (cue_lists, cue_task, _cue_peers) = crate::cue_lists::build_cue_lists_actor_with_scenes(
            source_events.clone(),
            scenes.clone(),
        );
        show_task.spawn();
        settings_task.spawn();
        scenes_task.spawn();
        cue_task.spawn();
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
                last_event_at: None,
            },
            initial_scenes_state: ScenesProjectionState {
                scene_configs: Vec::new(),
                selected_scene_internal_id: None,
                scene_settings_clipboard_available: false,
                ready_generation: None,
            },
            initial_cue_lists_state: CueListsProjectionState {
                document: crate::cue_lists::CueListDocument::default(),
                last_recall_status: None,
            },
            initial_settings,
            runtime_source,
            show,
            scenes,
            cue_lists,
            settings,
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
    async fn ping_event_does_not_emit_app_status_changed() {
        let app = mock_app();
        let handle = app.handle().clone();
        let event_bus = AppEventBus::default();
        let (_log_tx, log_rx) = broadcast::channel(8);
        let emitted = Arc::new(AtomicUsize::new(0));
        let emitted_events = emitted.clone();
        handle.listen_any("app-status-changed", move |_| {
            emitted_events.fetch_add(1, Ordering::SeqCst);
        });

        let projector = spawn_started_projector(handle, 0, event_bus.subscribe(), log_rx);

        tokio::time::timeout(Duration::from_secs(1), async {
            while emitted.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("projector did not emit its initial snapshot");
        assert_eq!(emitted.load(Ordering::SeqCst), 1);

        event_bus.publish_lv1(0, Lv1Event::PingReceived { sequence: 1 });
        tokio::time::sleep(PROJECTOR_INTERVAL + Duration::from_millis(60)).await;

        projector.abort();
        assert_eq!(emitted.load(Ordering::SeqCst), 1);
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
                    selected_scene_internal_id: Some("selected-id".to_string()),
                    scene_settings_clipboard_available: false,
                    ready_generation: Some(0),
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
    async fn cue_lists_event_marks_cache_dirty_and_projects_cue_lists() {
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

        event_bus.publish(AppEvent::CueLists(CueListsEvent::StateChanged {
            reason: crate::cue_lists::CueListsProjectionReason::CueListState,
            state: CueListsProjectionState {
                document: crate::cue_lists::CueListDocument {
                    cue_lists: vec![crate::cue_lists::CueList {
                        id: uuid::Uuid::from_u128(1),
                        name: "Main".to_string(),
                        entries: vec![],
                    }],
                    active_cue_list_id: Some(uuid::Uuid::from_u128(1)),
                    cued_cue_entry_id: None,
                },
                last_recall_status: Some("recalling".to_string()),
            },
            persisted_cue_list_edit: true,
        }));
        tokio::time::sleep(PROJECTOR_INTERVAL + Duration::from_millis(60)).await;

        projector.abort();
        let snapshots = received.lock().unwrap();
        assert!(snapshots.iter().any(|snapshot| {
            snapshot["cueLists"]
                .as_array()
                .is_some_and(|lists| lists.iter().any(|list| list["name"] == "Main"))
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
                last_event_at: None,
            },
            initial_scenes_state: ScenesProjectionState {
                scene_configs: Vec::new(),
                selected_scene_internal_id: None,
                scene_settings_clipboard_available: false,
                ready_generation: None,
            },
            initial_cue_lists_state: CueListsProjectionState {
                document: crate::cue_lists::CueListDocument::default(),
                last_recall_status: None,
            },
            initial_settings: AppSettings::default(),
            runtime_source: crate::lifecycle::AppLifecycle::default().runtime_snapshot_source(),
            show: crate::show::ShowStateHandle::new_empty(AppEventBus::default()),
            scenes: {
                let source_events = AppEventBus::default();
                let (_settings, settings_task, initial_settings) =
                    crate::settings::build_settings_actor(
                        std::env::temp_dir(),
                        source_events.clone(),
                    );
                let (_show, show_task, _show_peers, lockout) =
                    crate::show::build_show_actor(source_events.clone());
                let (settings, _, _) = crate::settings::build_settings_actor(
                    std::env::temp_dir(),
                    source_events.clone(),
                );
                settings_task.spawn();
                show_task.spawn();
                let (scenes, scenes_task, _) = crate::scenes::build_scenes_actor(
                    0,
                    RuntimeGeneration::new(),
                    source_events.clone(),
                    source_events.subscribe(),
                    settings,
                    initial_settings,
                    lockout,
                );
                scenes_task.spawn();
                scenes
            },
            cue_lists: {
                let source_events = AppEventBus::default();
                let (settings, settings_task, initial_settings) =
                    crate::settings::build_settings_actor(
                        std::env::temp_dir(),
                        source_events.clone(),
                    );
                let (_show, show_task, _show_peers, lockout) =
                    crate::show::build_show_actor(source_events.clone());
                settings_task.spawn();
                show_task.spawn();
                let (scenes, scenes_task, _) = crate::scenes::build_scenes_actor(
                    0,
                    RuntimeGeneration::new(),
                    source_events.clone(),
                    source_events.subscribe(),
                    settings,
                    initial_settings,
                    lockout,
                );
                scenes_task.spawn();
                let (cue, cue_task, _) =
                    crate::cue_lists::build_cue_lists_actor_with_scenes(source_events, scenes);
                cue_task.spawn();
                cue
            },
            settings: {
                let (settings, task, _) = crate::settings::build_settings_actor(
                    std::env::temp_dir(),
                    AppEventBus::default(),
                );
                task.spawn();
                settings
            },
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
    async fn lag_resync_timeout_resets_lv1_projection_to_disconnected() {
        let source_events = AppEventBus::default();
        let show = crate::show::ShowStateHandle::new_stalled();
        let (settings, settings_task, initial_settings) =
            crate::settings::build_settings_actor(std::env::temp_dir(), source_events.clone());
        let (_show_for_lockout, show_task, _show_peers, lockout) =
            crate::show::build_show_actor(source_events.clone());
        show_task.spawn();
        let runtime_generation = RuntimeGeneration::new();
        let (scenes, scenes_task, _) = crate::scenes::build_scenes_actor(
            0,
            runtime_generation,
            source_events.clone(),
            source_events.subscribe(),
            settings.clone(),
            initial_settings,
            lockout,
        );
        let (cue_lists, cue_task, _) =
            crate::cue_lists::build_cue_lists_actor_with_scenes(source_events, scenes.clone());
        settings_task.spawn();
        scenes_task.spawn();
        cue_task.spawn();

        let mut cache = ProjectionCache::new();
        cache.set_active_generation(7);
        cache.apply_lv1_snapshot(
            7,
            crate::lv1::Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: Vec::new(),
                channels: Vec::new(),
                ping_sequence: 0,
            },
        );
        let runtime_source = crate::lifecycle::AppLifecycle::default().runtime_snapshot_source();

        let completed = recover_projector_after_lag(
            &mut cache,
            &runtime_source,
            &show,
            &scenes,
            &cue_lists,
            &settings,
        )
        .await;

        assert!(!completed);
        assert_eq!(cache.active_generation(), 0);
        assert_eq!(
            cache.build_snapshot().connection,
            crate::projector::AppConnectionState::Disconnected
        );
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
