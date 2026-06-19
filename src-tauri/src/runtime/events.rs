use tokio::sync::broadcast;

use crate::fade::events::FadeEvent;
use crate::lv1::events::Lv1Event;
use crate::scene_recall::events::SceneRecallEvent;
use crate::show::events::ShowEvent;

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
    SceneRecall {
        generation: u64,
        event: SceneRecallEvent,
    },
    Show(ShowEvent),
}

#[derive(Clone)]
pub struct AppEventBus {
    tx: broadcast::Sender<AppEvent>,
}

impl AppEventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity.max(1));
        Self { tx }
    }

    pub fn publish(&self, event: AppEvent) -> usize {
        self.tx.send(event).unwrap_or(0)
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

    pub fn publish_scene_recall(&self, generation: u64, event: SceneRecallEvent) -> usize {
        self.publish(AppEvent::SceneRecall { generation, event })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::types::SceneState;
    use crate::show::events::ShowProjectionReason;

    #[tokio::test]
    async fn publish_succeeds_without_subscribers() {
        let bus = AppEventBus::new(16);

        let sent = bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneChanged(SceneState {
                index: 1,
                name: "test".to_string(),
            }),
        });

        assert_eq!(sent, 0);
    }

    #[tokio::test]
    async fn zero_capacity_constructor_creates_usable_bus() {
        let bus = AppEventBus::new(0);
        let mut rx = bus.subscribe();

        bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneChanged(SceneState {
                index: 2,
                name: "capacity".to_string(),
            }),
        });

        let event = rx.recv().await.unwrap();
        match event {
            AppEvent::Lv1 {
                generation: 0,
                event: Lv1Event::SceneChanged(scene),
            } => {
                assert_eq!(scene.index, 2);
                assert_eq!(scene.name, "capacity");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn subscriber_receives_published_event() {
        let bus = AppEventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneChanged(SceneState {
                index: 7,
                name: "Chorus".to_string(),
            }),
        });

        let event = rx.recv().await.unwrap();
        match event {
            AppEvent::Lv1 { generation, event } => {
                assert_eq!(generation, 0);
                match event {
                    Lv1Event::SceneChanged(scene) => {
                        assert_eq!(scene.index, 7);
                        assert_eq!(scene.name, "Chorus");
                    }
                    other => panic!("unexpected event: {other:?}"),
                }
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn lagged_subscriber_does_not_publish_back_to_event_bus() {
        let bus = AppEventBus::new(1);
        let mut rx = bus.subscribe();

        log_lagged_subscriber("test", 1);

        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn runtime_fact_publish_without_subscribers_is_safe() {
        let bus = AppEventBus::new(1);

        let sent = bus.publish(AppEvent::SceneRecall {
            generation: 0,
            event: crate::scene_recall::events::SceneRecallEvent::Skipped {
                scene_label: "1: Intro".to_string(),
                reason: "test".to_string(),
            },
        });

        assert_eq!(sent, 0);
    }

    #[tokio::test]
    async fn runtime_events_carry_generation() {
        let bus = AppEventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish(AppEvent::Lv1 {
            generation: 42,
            event: Lv1Event::Connected,
        });

        let event = rx.recv().await.unwrap();
        match event {
            AppEvent::Lv1 { generation, event } => {
                assert_eq!(generation, 42);
                assert!(matches!(event, Lv1Event::Connected));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn lifecycle_events_publish_active_generation_changes() {
        let bus = AppEventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish_runtime_generation_changed(7);

        let event = rx.recv().await.unwrap();
        match event {
            AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation }) => {
                assert_eq!(generation, 7);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn subscriber_receives_published_show_event() {
        let bus = AppEventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish(AppEvent::Show(ShowEvent::StateChanged {
            reason: ShowProjectionReason::ShowState,
            state: crate::show::events::ShowProjectionState {
                lockout: false,
                scene_configs: vec![],
                cued_scene_id: None,
                selected_scene_id: None,
                show_file_path: None,
                show_file_name: "Untitled Show".to_string(),
                show_file_dirty: false,
                show_file_last_saved_at: None,
                discovered_lv1_systems: vec![],
                connected_lv1_identity: None,
                pending_lv1_identity: None,
                reconnect: Default::default(),
                last_event_at: None,
            },
        }));

        let event = rx.recv().await.unwrap();
        assert!(matches!(
            event,
            AppEvent::Show(ShowEvent::StateChanged {
                reason: ShowProjectionReason::ShowState,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn show_fact_publish_without_subscribers_is_safe() {
        let bus = AppEventBus::new(1);

        let sent = bus.publish(AppEvent::Show(ShowEvent::StateChanged {
            reason: ShowProjectionReason::ShowState,
            state: crate::show::events::ShowProjectionState {
                lockout: false,
                scene_configs: vec![],
                cued_scene_id: None,
                selected_scene_id: None,
                show_file_path: None,
                show_file_name: "Untitled Show".to_string(),
                show_file_dirty: false,
                show_file_last_saved_at: None,
                discovered_lv1_systems: vec![],
                connected_lv1_identity: None,
                pending_lv1_identity: None,
                reconnect: Default::default(),
                last_event_at: None,
            },
        }));

        assert_eq!(sent, 0);
    }
}
