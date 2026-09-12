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

/**
 * @cc [owner:mixxorz,label:architecture;projection] retained-state-is-authoritative
 * The projector MUST seed and refresh app-lifetime fields from the retained watch snapshot,
 * including facts published before subscription; app-lifetime broadcast facts MUST NOT be copied
 * into `ProjectionCache` or recovered by querying their actors.
 */
/**
 * @cc [owner:mixxorz,label:performance;projection] dirty-throttled-emission
 * The projector MUST emit only while dirty and only on interval ticks spaced by
 * `PROJECTOR_INTERVAL`; unchanged retained state and non-material LV1 facts MUST NOT cause an
 * emission, while multiple changes before a tick MUST be coalesced into the latest snapshot.
 */
/**
 * @cc [owner:mixxorz,label:logging;projection] ui-log-input-boundary
 * Frontend logs MUST enter snapshots only through the UI-log receiver and cache; receiving a log
 * marks the view dirty, while lag may lose unavailable log entries but MUST retain already cached
 * entries and keep the projector running.
 */
/**
 * @cc [owner:mixxorz,label:reliability;consistency] lag-recovery-event-cutoff
 * When event lag is detected, the projector MUST discard facts already queued before starting its
 * bounded authoritative recovery. Facts arriving after that drain, including during recovery, MAY
 * remain queued and be processed normally after recovery subject to generation filtering.
 */
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

/// @cc [owner:mixxorz,label:reliability;consistency] lag-discards-queued-facts
/// This helper MUST consume every fact currently available from the lagged receiver and stop once
/// it is empty or closed; it does not govern facts that arrive after the drain completes.
fn drain_retained_events(events: &mut broadcast::Receiver<AppEvent>) {
    while let Ok(_) | Err(broadcast::error::TryRecvError::Lagged(_)) = events.try_recv() {}
}

/// @cc [owner:mixxorz,label:reliability;generation;fallback] lag-recovery-fails-disconnected
/// Recovery MUST clear generation-bound cache state and apply an authoritative connected LV1
/// snapshot only if its captured generation still equals the current generation. It MUST be bounded;
/// unavailable, disconnected, stale, or timed-out LV1 state falls back to disconnected/Idle live
/// projection without discarding retained app state or logs.
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

/// @cc [owner:mixxorz,label:architecture;reliability] sole-best-effort-status-emission
/// This projector boundary MUST be the sole emitter of `app-status-changed`; emission failure MUST be
/// diagnostic-only and MUST NOT terminate the projector or recursively create a frontend log.
fn emit_app_status<R: Runtime>(app: &AppHandle<R>, snapshot: &AppViewState) {
    if let Err(err) = app.emit("app-status-changed", snapshot) {
        tracing::debug!(event = "projector_emit_failed", error = %err, "Failed to emit app-status-changed from projector");
    }
}

/// @cc [owner:mixxorz,label:architecture;generation] projector-event-routing
/// Runtime generation changes MUST reset generation-bound projection, LV1/Fade facts MUST be routed
/// through generation filtering, and app-lifetime facts MUST leave this cache untouched because the
/// retained watch snapshot owns their projection.
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
