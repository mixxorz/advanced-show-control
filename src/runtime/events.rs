use tokio::sync::broadcast;

use crate::fade::events::FadeEvent;
use crate::lv1::events::Lv1Event;

#[derive(Debug, Clone)]
pub enum AppEvent {
    Lv1(Lv1Event),
    Fade(FadeEvent),
    SceneRecall(crate::scene_recall::events::SceneRecallEvent),
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

    #[tokio::test]
    async fn publish_succeeds_without_subscribers() {
        let bus = AppEventBus::new(16);

        let sent = bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(SceneState {
            index: 1,
            name: "test".to_string(),
        })));

        assert_eq!(sent, 0);
    }

    #[tokio::test]
    async fn zero_capacity_constructor_creates_usable_bus() {
        let bus = AppEventBus::new(0);
        let mut rx = bus.subscribe();

        bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(SceneState {
            index: 2,
            name: "capacity".to_string(),
        })));

        let event = rx.recv().await.unwrap();
        match event {
            AppEvent::Lv1(Lv1Event::SceneChanged(scene)) => {
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

        bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(SceneState {
            index: 7,
            name: "Chorus".to_string(),
        })));

        let event = rx.recv().await.unwrap();
        match event {
            AppEvent::Lv1(Lv1Event::SceneChanged(scene)) => {
                assert_eq!(scene.index, 7);
                assert_eq!(scene.name, "Chorus");
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

        let sent = bus.publish(AppEvent::SceneRecall(
            crate::scene_recall::events::SceneRecallEvent::Skipped {
                scene_label: "1: Intro".to_string(),
                reason: "test".to_string(),
            },
        ));

        assert_eq!(sent, 0);
    }
}
