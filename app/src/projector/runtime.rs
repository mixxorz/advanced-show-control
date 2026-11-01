use std::time::Duration;

use tokio::sync::{broadcast, watch};

use crate::logging::UiLogEvent;
use crate::projector::AppViewState;
use crate::runtime::AppStateSnapshot;

use super::ProjectionCache;

pub const PROJECTOR_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone)]
enum PublishedView {
    AwaitingFirst,
    Ready(Box<AppViewState>),
}

#[derive(Clone, Copy)]
enum PublicationState {
    Clean,
    Dirty,
}

impl PublicationState {
    fn invalidate(&mut self) {
        *self = Self::Dirty;
    }

    fn take_dirty(&mut self) -> bool {
        matches!(std::mem::replace(self, Self::Clean), Self::Dirty)
    }
}

#[derive(Clone)]
pub struct ProjectionSink {
    snapshots: watch::Sender<PublishedView>,
}

pub struct ProjectionSubscription {
    snapshots: watch::Receiver<PublishedView>,
}

impl ProjectionSink {
    pub fn subscribe(&self) -> ProjectionSubscription {
        ProjectionSubscription {
            snapshots: self.snapshots.subscribe(),
        }
    }

    fn publish(&self, snapshot: AppViewState) {
        self.snapshots
            .send_replace(PublishedView::Ready(Box::new(snapshot)));
    }
}

impl ProjectionSubscription {
    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        self.snapshots.changed().await
    }

    pub fn latest(&mut self) -> Option<AppViewState> {
        match &*self.snapshots.borrow_and_update() {
            PublishedView::AwaitingFirst => None,
            PublishedView::Ready(snapshot) => Some((**snapshot).clone()),
        }
    }
}

/// @cc [owner:mixxorz,label:architecture;projection] latest-snapshot-projection-channel
/// The host-neutral projection channel MUST retain only the latest complete snapshot so a slow or
/// late host bridge can resume from current state without replaying stale intermediate views.
pub fn projection_channel() -> (ProjectionSink, ProjectionSubscription) {
    let (snapshots, receiver) = watch::channel(PublishedView::AwaitingFirst);
    (
        ProjectionSink { snapshots },
        ProjectionSubscription {
            snapshots: receiver,
        },
    )
}

pub struct ProjectorInputs {
    pub sink: ProjectionSink,
    pub state: watch::Receiver<AppStateSnapshot>,
    pub logs: broadcast::Receiver<UiLogEvent>,
}

/**
 * @cc [owner:mixxorz,label:architecture;projection] retained-state-is-authoritative
 * The projector MUST seed and refresh app-lifetime and generation-scoped display fields from the
 * retained watch snapshot, including LV1/Fade state published before subscription. It MUST NOT
 * subscribe to operational facts or query actors to recover display state.
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
pub fn spawn_projector(inputs: ProjectorInputs) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let ProjectorInputs {
            sink,
            mut state,
            mut logs,
        } = inputs;
        tracing::debug!(event = "projector_started", "Projector started");
        let mut cache = ProjectionCache::new();
        let mut interval = tokio::time::interval(PROJECTOR_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        let mut publication = PublicationState::Dirty;
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if publication.take_dirty() {
                        let snapshot = cache.build_snapshot(&state.borrow_and_update());
                        sink.publish(snapshot);
                    }
                }
                changed = state.changed() => {
                    if changed.is_err() { break; }
                    publication.invalidate();
                }
                received = logs.recv() => match received {
                    Ok(ui_log) => {
                        cache.append_log(ui_log);
                        publication.invalidate();
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => publication.invalidate(),
                    Err(broadcast::error::RecvError::Closed) => break,
                },
            }
        }
    })
}

#[cfg(test)]
mod tests;
