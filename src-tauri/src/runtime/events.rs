use tokio::sync::{broadcast, watch};

use super::AppStateSnapshot;

use crate::cue_lists::CueListsProjectionState;
use crate::fade::FadeEvent;
use crate::lv1::Lv1Event;
use crate::scenes::ScenesEvent;
use crate::settings::SettingsEvent;
use crate::show::ShowProjectionState;

#[derive(Debug, Clone)]
pub enum RuntimeLifecycleEvent {
    ActiveGenerationChanged { generation: u64 },
}

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
}

impl AppEventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity.max(1));
        let (state, _) = watch::channel(AppStateSnapshot::default());
        Self { tx, state }
    }

    pub fn publish(&self, event: AppEvent) -> usize {
        self.retain(&event);
        self.tx.send(event).unwrap_or(0)
    }

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
    fn default() -> Self {
        Self::new(256)
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
