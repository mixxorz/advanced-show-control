use std::time::Duration;

use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::app_state::ShellState;
use crate::lifecycle::ActiveCommandBus;
use crate::logging::UiLogEvent;
use crate::lv1::events::Lv1Event;
use crate::runtime::events::AppEvent;
use crate::runtime::events::log_lagged_subscriber;

use super::ProjectionCache;

pub const PROJECTOR_INTERVAL: Duration = Duration::from_millis(100);

pub struct ProjectorInputs<R: Runtime> {
    pub app: AppHandle<R>,
    pub shell_state: ShellState,
    pub active_command_bus: ActiveCommandBus,
    pub generation: u64,
    pub events: broadcast::Receiver<AppEvent>,
    pub logs: mpsc::Receiver<UiLogEvent>,
    pub start_rx: oneshot::Receiver<()>,
}

pub fn spawn_projector<R: Runtime>(inputs: ProjectorInputs<R>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let ProjectorInputs {
            app,
            shell_state,
            active_command_bus,
            generation,
            mut events,
            mut logs,
            start_rx,
        } = inputs;

        if start_rx.await.is_err() {
            return;
        }

        tracing::debug!(
            event = "projector_started",
            generation = generation,
            "projector started"
        );

        let mut cache = ProjectionCache::new();
        let mut interval = tokio::time::interval(PROJECTOR_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        let mut dirty = false;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if dirty {
                        let show = shell_state.show.get_snapshot().await;
                        let snapshot = cache.build_snapshot(show);
                        emit_snapshot(&app, &snapshot);
                        dirty = false;
                    }
                }
                received = events.recv() => {
                    match received {
                        Ok(app_event) => {
                            if apply_projector_event(&mut cache, &shell_state, generation, &active_command_bus, &app_event).await {
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
                        Some(ui_log) => {
                            cache.append_log(ui_log);
                            dirty = true;
                        }
                        None => break,
                    }
                }
            }
        }
    })
}

fn emit_snapshot<R: Runtime>(app: &AppHandle<R>, snapshot: &crate::app_state::AppViewState) {
    if let Err(err) = app.emit("app-status-changed", snapshot) {
        tracing::debug!(
            event = "projector_emit_failed",
            error = %err,
            "failed to emit app-status-changed from projector"
        );
    }
}

async fn apply_projector_event(
    cache: &mut ProjectionCache,
    shell_state: &ShellState,
    generation: u64,
    active_command_bus: &ActiveCommandBus,
    event: &AppEvent,
) -> bool {
    match event {
        AppEvent::Lv1(event) => {
            if let Lv1Event::SceneListChanged(scenes) = event {
                let _ = shell_state
                    .show
                    .scene_reconciliation_diagnostic(scenes.clone())
                    .await;
            }

            cache.apply_lv1_event(event);

            if matches!(event, Lv1Event::Disconnected { .. }) {
                shell_state
                    .clear_runtime_handles(generation, active_command_bus)
                    .await;
            }
            true
        }
        AppEvent::Fade(event) => {
            cache.apply_fade_event(event);
            true
        }
        AppEvent::SceneRecall(_) => false,
        AppEvent::Show(_) => {
            cache.mark_show_stale();
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::LogSeverity;
    use crate::runtime::events::AppEventBus;
    use crate::show::events::ShowEvent;
    use crate::show::events::ShowSnapshotChange;
    use std::sync::{Arc, Mutex};
    use tauri::{Listener, test::mock_app};

    fn spawn_started_projector(
        handle: AppHandle<impl Runtime>,
        state: ShellState,
        active_command_bus: ActiveCommandBus,
        generation: u64,
        events: broadcast::Receiver<AppEvent>,
        logs: mpsc::Receiver<UiLogEvent>,
    ) -> tokio::task::JoinHandle<()> {
        let (start_tx, start_rx) = oneshot::channel();
        let projector = spawn_projector(ProjectorInputs {
            app: handle,
            shell_state: state,
            active_command_bus,
            generation,
            events,
            logs,
            start_rx,
        });
        let _ = start_tx.send(());
        projector
    }

    #[tokio::test]
    async fn projector_emits_ui_log_entries_from_log_input() {
        let app = mock_app();
        let handle = app.handle().clone();
        let event_bus = AppEventBus::default();
        let state = ShellState::new(event_bus.clone());
        let active_command_bus = ActiveCommandBus::default();
        let (log_tx, log_rx) = mpsc::channel(8);
        let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let received_events = received.clone();
        handle.listen_any("app-status-changed", move |event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            received_events.lock().unwrap().push(payload);
        });

        let projector = spawn_started_projector(
            handle,
            state,
            active_command_bus,
            0,
            event_bus.subscribe(),
            log_rx,
        );

        log_tx
            .send(UiLogEvent {
                severity: LogSeverity::Warning,
                message: "projected log".to_string(),
            })
            .await
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
        let state = ShellState::new(event_bus.clone());
        state.show.set_lockout(true).await;
        let active_command_bus = ActiveCommandBus::default();
        let (_log_tx, log_rx) = mpsc::channel(8);
        let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let received_events = received.clone();
        handle.listen_any("app-status-changed", move |event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            received_events.lock().unwrap().push(payload);
        });

        let projector = spawn_started_projector(
            handle,
            state,
            active_command_bus,
            0,
            event_bus.subscribe(),
            log_rx,
        );

        event_bus.publish(AppEvent::Show(ShowEvent::SnapshotChanged {
            reason: ShowSnapshotChange::Lockout,
        }));
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
        let state = ShellState::new(event_bus.clone());
        let active_command_bus = ActiveCommandBus::default();
        let (_log_tx, log_rx) = mpsc::channel(8);
        let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let received_events = received.clone();
        handle.listen_any("app-status-changed", move |event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            received_events.lock().unwrap().push(payload);
        });

        let projector = spawn_started_projector(
            handle,
            state,
            active_command_bus,
            0,
            event_bus.subscribe(),
            log_rx,
        );

        let event = AppEvent::Show(ShowEvent::SnapshotChanged {
            reason: ShowSnapshotChange::Lockout,
        });
        event_bus.publish(event.clone());
        event_bus.publish(event);

        tokio::time::sleep(PROJECTOR_INTERVAL + Duration::from_millis(60)).await;
        tokio::time::sleep(PROJECTOR_INTERVAL + Duration::from_millis(60)).await;

        projector.abort();
        let snapshots = received.lock().unwrap();
        assert_eq!(snapshots.len(), 1);
    }
}
