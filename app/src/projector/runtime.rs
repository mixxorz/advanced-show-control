use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use tokio::sync::{broadcast, watch};

use crate::lifecycle::RuntimeSnapshotSource;
use crate::logging::UiLogEvent;
use crate::lv1::{ConnectionStatus, Lv1Command};
use crate::projector::AppViewState;
use crate::runtime::AppStateSnapshot;
use crate::runtime::events::{AppEvent, RuntimeLifecycleEvent, log_lagged_subscriber};

use super::ProjectionCache;

pub const PROJECTOR_INTERVAL: Duration = Duration::from_millis(100);
const MAX_PENDING_RECOVERY_EVENTS: usize = 4_096;

#[derive(Clone)]
pub struct ProjectionSink {
    snapshots: watch::Sender<Option<AppViewState>>,
}

pub struct ProjectionSubscription {
    snapshots: watch::Receiver<Option<AppViewState>>,
}

impl ProjectionSink {
    pub fn subscribe(&self) -> ProjectionSubscription {
        ProjectionSubscription {
            snapshots: self.snapshots.subscribe(),
        }
    }

    fn publish(&self, snapshot: AppViewState) {
        self.snapshots.send_replace(Some(snapshot));
    }
}

impl ProjectionSubscription {
    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        self.snapshots.changed().await
    }

    pub fn latest(&mut self) -> Option<AppViewState> {
        self.snapshots.borrow_and_update().clone()
    }
}

/// @cc [owner:mixxorz,label:architecture;projection] latest-snapshot-projection-channel
/// The host-neutral projection channel MUST retain only the latest complete snapshot so a slow or
/// late host bridge can resume from current state without replaying stale intermediate views.
pub fn projection_channel() -> (ProjectionSink, ProjectionSubscription) {
    let (snapshots, receiver) = watch::channel(None);
    (
        ProjectionSink { snapshots },
        ProjectionSubscription {
            snapshots: receiver,
        },
    )
}

pub struct ProjectorInputs {
    pub sink: ProjectionSink,
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
 * The projector MUST publish only while dirty and only on interval ticks spaced by
 * `PROJECTOR_INTERVAL`; unchanged retained state and non-material LV1 facts MUST NOT cause a
 * publication, while multiple changes before a tick MUST be coalesced into the latest snapshot.
 */
/**
 * @cc [owner:mixxorz,label:logging;projection] ui-log-input-boundary
 * Native UI logs MUST enter snapshots only through the UI-log receiver and cache; receiving a log
 * marks the view dirty, while lag may lose unavailable log entries but MUST retain already cached
 * entries and keep the projector running.
 */
/**
 * @cc [owner:mixxorz,label:reliability;consistency] lag-recovery-event-cutoff
 * When event lag is detected, the projector MUST discard facts already queued before starting its
 * bounded authoritative recovery. Facts arriving after that drain, including during recovery, MUST
 * enter an actor-owned bounded queue and be processed after recovery subject to generation
 * filtering. Queue overflow MUST establish a new cutoff and restart bounded recovery.
 */
pub fn spawn_projector(inputs: ProjectorInputs) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let ProjectorInputs {
            sink,
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
        let mut recovery: Option<PendingProjectorRecovery> = None;
        loop {
            if let Some(pending) = recovery.as_mut() {
                tokio::select! {
                    result = &mut pending.future => {
                        let queued_events = std::mem::take(&mut pending.queued_events);
                        let restart = pending.restart_required;
                        if restart {
                            let mut next = start_projector_recovery(runtime_source.clone());
                            next.queued_events = queued_events;
                            recovery = Some(next);
                        } else {
                            apply_recovery_result(&mut cache, result);
                            for event in queued_events {
                                apply_projector_event(&mut cache, &event);
                            }
                            recovery = None;
                        }
                        dirty = true;
                    }
                    _ = interval.tick() => {
                        if dirty {
                            let snapshot = cache.build_snapshot(&state.borrow_and_update());
                            sink.publish(snapshot);
                            dirty = false;
                        }
                    }
                    changed = state.changed() => {
                        if changed.is_err() { break; }
                        dirty = true;
                    }
                    received = events.recv() => match received {
                        Ok(event) => pending.push_event(event),
                        Err(broadcast::error::RecvError::Lagged(count)) => {
                            log_lagged_subscriber("projector", count);
                            drain_retained_events(&mut events);
                            pending.queued_events.clear();
                            pending.restart_required = true;
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
            } else {
                tokio::select! {
                    _ = interval.tick() => {
                        if dirty {
                            let snapshot = cache.build_snapshot(&state.borrow_and_update());
                            sink.publish(snapshot);
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
                            cache.reset_generation_scoped_state();
                            recovery = Some(start_projector_recovery(runtime_source.clone()));
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
/// Recovery MUST remain bounded, preserve responsive retained-state and log reception, and apply an
/// authoritative connected LV1 snapshot only if its captured generation still equals the current
/// generation. Unavailable, disconnected, stale, or timed-out LV1 state falls back to
/// disconnected/Idle live projection without discarding retained app state or logs.
struct PendingProjectorRecovery {
    future: Pin<Box<dyn Future<Output = ProjectorRecoveryResult> + Send>>,
    queued_events: VecDeque<AppEvent>,
    restart_required: bool,
}

impl PendingProjectorRecovery {
    fn push_event(&mut self, event: AppEvent) {
        if self.queued_events.len() == MAX_PENDING_RECOVERY_EVENTS {
            self.queued_events.clear();
            self.restart_required = true;
        }
        self.queued_events.push_back(event);
    }
}

struct ProjectorRecoveryResult {
    generation: Option<u64>,
    snapshot: Option<(u64, crate::lv1::Lv1StateSnapshot)>,
    timed_out: bool,
}

fn start_projector_recovery(runtime_source: RuntimeSnapshotSource) -> PendingProjectorRecovery {
    PendingProjectorRecovery {
        future: Box::pin(recover_projector_after_lag(runtime_source)),
        queued_events: VecDeque::new(),
        restart_required: false,
    }
}

async fn recover_projector_after_lag(
    runtime_source: RuntimeSnapshotSource,
) -> ProjectorRecoveryResult {
    match tokio::time::timeout(Duration::from_millis(500), async {
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
        (current_generation, authoritative_snapshot)
    })
    .await
    {
        Ok((generation, snapshot)) => ProjectorRecoveryResult {
            generation: Some(generation),
            snapshot,
            timed_out: false,
        },
        Err(_) => ProjectorRecoveryResult {
            generation: tokio::time::timeout(
                Duration::from_millis(50),
                runtime_source.current_generation(),
            )
            .await
            .ok(),
            snapshot: None,
            timed_out: true,
        },
    }
}

fn apply_recovery_result(cache: &mut ProjectionCache, result: ProjectorRecoveryResult) {
    if let Some(generation) = result.generation {
        cache.reset_for_generation(generation);
        if let Some((snapshot_generation, snapshot)) = result.snapshot
            && snapshot_generation == generation
        {
            cache.apply_lv1_snapshot(snapshot_generation, snapshot);
        }
    } else {
        cache.reset_generation_scoped_state();
    }
    if result.timed_out {
        tracing::warn!(
            event = "projector_resync_timeout",
            "Projector resynchronization timed out; showing disconnected state"
        );
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
