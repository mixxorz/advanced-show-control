use std::time::Duration;

use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::{broadcast, watch};

use crate::lifecycle::RuntimeSnapshotSource;
use crate::logging::UiLogEvent;
use crate::lv1::{ConnectionStatus, Lv1Command};
use crate::projector::AppViewState;
use crate::runtime::AppStateSnapshot;
use crate::runtime::events::{AppEvent, RuntimeLifecycleEvent, log_lagged_subscriber};

use super::ProjectionCache;

pub const PROJECTOR_INTERVAL: Duration = Duration::from_millis(100);

pub struct ProjectorInputs<R: Runtime> {
    pub app: AppHandle<R>,
    pub generation: u64,
    pub state: watch::Receiver<AppStateSnapshot>,
    pub runtime_source: RuntimeSnapshotSource,
    pub events: broadcast::Receiver<AppEvent>,
    pub logs: broadcast::Receiver<UiLogEvent>,
}

pub fn spawn_projector<R: Runtime>(inputs: ProjectorInputs<R>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let ProjectorInputs {
            app,
            generation,
            mut state,
            runtime_source,
            mut events,
            mut logs,
        } = inputs;
        tracing::debug!(event = "projector_started", generation, "Projector started");
        let mut cache = ProjectionCache::new();
        cache.set_active_generation(generation);
        let mut interval = tokio::time::interval(PROJECTOR_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        let mut dirty = true;
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if dirty {
                        let snapshot = cache.build_snapshot(&state.borrow_and_update());
                        emit_app_status(&app, &snapshot);
                        dirty = false;
                    }
                }
                changed = state.changed() => {
                    if changed.is_err() { break; }
                    dirty = true;
                }
                received = events.recv() => match received {
                    Ok(event) => dirty |= apply_projector_event(&mut cache, &event),
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        log_lagged_subscriber("projector", count);
                        drain_retained_events(&mut events);
                        if !recover_projector_after_lag(&mut cache, &runtime_source).await {
                            tracing::warn!(event = "projector_resync_timeout", "Projector resynchronization timed out; showing disconnected state");
                        }
                        dirty = true;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                received = logs.recv() => match received {
                    Ok(ui_log) => {
                        cache.append_log(ui_log);
                        dirty = true;
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => dirty = true,
                    Err(broadcast::error::RecvError::Closed) => break,
                },
            }
        }
    })
}

fn drain_retained_events(events: &mut broadcast::Receiver<AppEvent>) {
    while let Ok(_) | Err(broadcast::error::TryRecvError::Lagged(_)) = events.try_recv() {}
}

async fn recover_projector_after_lag(
    cache: &mut ProjectionCache,
    runtime_source: &RuntimeSnapshotSource,
) -> bool {
    let recovery = tokio::time::timeout(Duration::from_millis(500), async {
        let runtime_snapshot = runtime_source.connected_lv1().await;
        let authoritative_snapshot = if let Some((generation, lv1)) = runtime_snapshot {
            let (reply, response) = tokio::sync::oneshot::channel();
            if lv1.send(Lv1Command::GetState { reply }).await.is_ok() {
                response
                    .await
                    .ok()
                    .filter(|snapshot| snapshot.connection == ConnectionStatus::Connected)
                    .map(|snapshot| (generation, snapshot))
            } else {
                None
            }
        } else {
            None
        };
        let current_generation = runtime_source.current_generation().await;
        cache.reset_for_generation(current_generation);
        if let Some((generation, snapshot)) = authoritative_snapshot
            && generation == current_generation
        {
            cache.apply_lv1_snapshot(generation, snapshot);
        }
    })
    .await;
    if recovery.is_err() {
        match tokio::time::timeout(
            Duration::from_millis(50),
            runtime_source.current_generation(),
        )
        .await
        {
            Ok(generation) => cache.reset_for_generation(generation),
            Err(_) => cache.reset_generation_scoped_state(),
        }
        return false;
    }
    true
}

fn emit_app_status<R: Runtime>(app: &AppHandle<R>, snapshot: &AppViewState) {
    if let Err(err) = app.emit("app-status-changed", snapshot) {
        tracing::debug!(event = "projector_emit_failed", error = %err, "Failed to emit app-status-changed from projector");
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
        _ => false,
    }
}

#[cfg(test)]
mod tests;
