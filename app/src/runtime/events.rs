use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use tokio::sync::{broadcast, watch};

use super::AppStateSnapshot;

use crate::cue_lists::CueListsProjectionState;
use crate::fade::FadeEvent;
use crate::lv1::Lv1Event;
use crate::scenes::ScenesEvent;
use crate::settings::SettingsEvent;
use crate::show::ShowProjectionState;

const DEFAULT_BROADCAST_CAPACITY: usize = 4096;

#[derive(Debug, Clone)]
pub enum RuntimeLifecycleEvent {
    ActiveGenerationChanged { generation: u64 },
}

/// @cc [owner:mixxorz,label:architecture] event-lifetime-classification
/// `Lv1` and `Fade` facts MUST carry the connection generation that produced them. `Scenes` and
/// `SessionReplaced` generations are runtime context for app-lifetime documents, while `CueLists`,
/// `Show`, and `Settings` are also app-lifetime; consumers MUST NOT discard any of these app-lifetime
/// facts based on generation.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum AppEvent {
    Runtime(RuntimeLifecycleEvent),
    Lv1 {
        generation: u64,
        event: Lv1Event,
    },
    Fade {
        generation: u64,
        event: FadeEvent,
    },
    Scenes {
        generation: u64,
        event: ScenesEvent,
    },
    CueLists(CueListsProjectionState),
    SessionReplaced {
        generation: u64,
        scenes: crate::scenes::ScenesProjectionState,
        cue_lists: crate::cue_lists::CueListsProjectionState,
    },
    Show(ShowProjectionState),
    Settings(SettingsEvent),
}

#[derive(Clone)]
pub struct AppEventBus {
    tx: broadcast::Sender<AppEvent>,
    state: watch::Sender<AppStateSnapshot>,
    persisted_session_revision: Arc<AtomicU64>,
}

impl AppEventBus {
    /// @cc [owner:mixxorz,label:reliability] nonzero-broadcast-capacity
    /// Construction MUST accept zero without panicking by creating a broadcast channel with at
    /// least one slot.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity.max(1));
        let (state, _) = watch::channel(AppStateSnapshot::default());
        Self {
            tx,
            state,
            persisted_session_revision: Arc::new(AtomicU64::new(0)),
        }
    }

    /**
     * @cc [owner:mixxorz,label:architecture] synchronous-fact-publication
     * Publishing MUST synchronously retain and broadcast an already-established fact; event variants
     * MUST NOT contain reply channels or cause publication to await or request actor work.
     */
    /**
     * @cc [owner:mixxorz,label:architecture] retain-before-broadcast
     * Publishing MUST apply any retained app-state projection before broadcasting the fact, so a
     * receiver reacting to that fact can read a snapshot at least as new as the fact.
     */
    /**
     * @cc [owner:mixxorz,label:reliability] publish-without-subscribers
     * Publishing with no broadcast receivers MUST still retain applicable state and MUST return
     * zero rather than fail.
     */
    /// @cc [owner:mixxorz,label:persistence;ordering] owner-persisted-session-revision
    /// Publishing a persisted Scenes state change or any Cue Lists fact MUST synchronously advance
    /// the shared revision before retaining or broadcasting the fact. Projection-only scene facts,
    /// session replacement, and facts from all other domains MUST NOT advance it. Show-owned
    /// persisted mutations MUST call `note_persisted_session_edit` only after accepting a change.
    pub fn publish(&self, event: AppEvent) -> usize {
        if matches!(
            &event,
            AppEvent::Scenes {
                event: ScenesEvent::StateChanged {
                    persisted_scene_edit: true,
                    ..
                },
                ..
            } | AppEvent::CueLists(_)
        ) {
            self.note_persisted_session_edit();
        }
        self.retain(&event);
        self.tx.send(event).unwrap_or(0)
    }

    pub fn note_persisted_session_edit(&self) {
        self.persisted_session_revision
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |revision| {
                revision.checked_add(1)
            })
            .expect("persisted session revision exhausted");
    }

    pub fn persisted_session_revision(&self) -> u64 {
        self.persisted_session_revision.load(Ordering::SeqCst)
    }

    /// @cc [owner:mixxorz,label:consistency] unchanged-state-does-not-notify
    /// Retention MUST notify watch subscribers only when an applicable projection value changes;
    /// duplicate projections and non-retained facts MUST not produce a watch change.
    pub(crate) fn retain(&self, event: &AppEvent) {
        self.state.send_if_modified(|state| state.apply(event));
    }

    pub fn state(&self) -> watch::Receiver<AppStateSnapshot> {
        self.state.subscribe()
    }

    pub fn publish_runtime_generation_changed(&self, generation: u64) -> usize {
        self.publish(AppEvent::Runtime(
            RuntimeLifecycleEvent::ActiveGenerationChanged { generation },
        ))
    }

    pub fn publish_lv1(&self, generation: u64, event: Lv1Event) -> usize {
        self.publish(AppEvent::Lv1 { generation, event })
    }

    pub fn publish_fade(&self, generation: u64, event: FadeEvent) -> usize {
        self.publish(AppEvent::Fade { generation, event })
    }

    pub fn publish_scenes(&self, generation: u64, event: ScenesEvent) -> usize {
        self.publish(AppEvent::Scenes { generation, event })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }
}

impl Default for AppEventBus {
    /// @cc [owner:mixxorz,label:reliability] default-broadcast-capacity
    /// A default bus MUST retain 4,096 events published after subscription and before the subscriber
    /// receives, without reporting lag.
    fn default() -> Self {
        Self::new(DEFAULT_BROADCAST_CAPACITY)
    }
}

pub fn log_lagged_subscriber(name: &str, count: u64) {
    tracing::debug!(
        event = "event_subscriber_lagged",
        subscriber = name,
        missed_events = count,
        "Event subscriber lagged and missed {count} events"
    );
}

#[cfg(test)]
mod tests {
    use super::{AppEvent, AppEventBus, RuntimeLifecycleEvent};
    use crate::cue_lists::CueListsProjectionState;
    use crate::scenes::{ScenesEvent, ScenesProjectionState};

    #[test]
    fn persisted_revision_advances_only_for_persisted_scene_and_cue_facts() {
        let event_bus = AppEventBus::default();

        event_bus.publish_scenes(
            0,
            ScenesEvent::StateChanged {
                state: ScenesProjectionState::default(),
                persisted_scene_edit: false,
            },
        );
        event_bus.publish(AppEvent::SessionReplaced {
            generation: 1,
            scenes: ScenesProjectionState::default(),
            cue_lists: CueListsProjectionState::default(),
        });
        assert_eq!(event_bus.persisted_session_revision(), 0);

        event_bus.publish_scenes(
            0,
            ScenesEvent::StateChanged {
                state: ScenesProjectionState::default(),
                persisted_scene_edit: true,
            },
        );
        assert_eq!(event_bus.persisted_session_revision(), 1);

        event_bus.publish(AppEvent::CueLists(CueListsProjectionState::default()));
        assert_eq!(event_bus.persisted_session_revision(), 2);
    }

    #[tokio::test]
    async fn default_bus_retains_4096_events_for_a_waiting_subscriber() {
        let event_bus = AppEventBus::default();
        let mut receiver = event_bus.subscribe();

        for generation in 0..4096 {
            event_bus.publish_runtime_generation_changed(generation);
        }

        for expected_generation in 0..4096 {
            let event = receiver.recv().await.expect("default bus must not lag");
            assert!(matches!(
                event,
                AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation })
                    if generation == expected_generation
            ));
        }
    }
}
