use tokio::sync::broadcast;

use crate::fade::types::FadeEvent;
use crate::lv1::events::Lv1Event;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutomationEvent {
    RuleTriggered { rule_id: String },
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    Lv1(Lv1Event),
    Fade(FadeEvent),
    Automation(AutomationEvent),
    CommandFailed { command: String, message: String },
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
    eprintln!("{name} event subscriber lagged and missed {count} events");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::types::SceneState;

    #[tokio::test]
    async fn publish_succeeds_without_subscribers() {
        let bus = AppEventBus::new(16);

        let sent = bus.publish(AppEvent::CommandFailed {
            command: "test".to_string(),
            message: "no subscriber".to_string(),
        });

        assert_eq!(sent, 0);
    }

    #[tokio::test]
    async fn zero_capacity_constructor_creates_usable_bus() {
        let bus = AppEventBus::new(0);
        let mut rx = bus.subscribe();

        bus.publish(AppEvent::CommandFailed {
            command: "zero".to_string(),
            message: "capacity".to_string(),
        });

        let event = rx.recv().await.unwrap();
        match event {
            AppEvent::CommandFailed { command, message } => {
                assert_eq!(command, "zero");
                assert_eq!(message, "capacity");
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
    async fn lagged_subscriber_reports_missed_events() {
        let bus = AppEventBus::new(1);
        let mut rx = bus.subscribe();

        bus.publish(AppEvent::CommandFailed {
            command: "first".to_string(),
            message: "one".to_string(),
        });
        bus.publish(AppEvent::CommandFailed {
            command: "second".to_string(),
            message: "two".to_string(),
        });

        let err = rx.recv().await.unwrap_err();
        assert!(matches!(err, broadcast::error::RecvError::Lagged(1)));
    }
}
