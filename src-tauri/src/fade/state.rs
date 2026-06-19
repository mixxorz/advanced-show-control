use crate::fade::events::FadeEvent;
use crate::fade::tick::ActiveTarget;
use crate::runtime::events::{AppEvent, AppEventBus};

pub(crate) struct EngineState {
    pub(crate) channels: Vec<ActiveTarget>,
    pub(crate) event_bus: AppEventBus,
}

impl EngineState {
    pub(crate) fn new(event_bus: AppEventBus) -> Self {
        Self {
            channels: Vec::new(),
            event_bus,
        }
    }

    pub(crate) fn fan_out(&mut self, event: FadeEvent) {
        self.event_bus.publish(AppEvent::Fade {
            generation: 0,
            event,
        });
    }

    pub(crate) fn is_active(&self) -> bool {
        !self.channels.is_empty()
    }

    pub(crate) fn cancel_all_in_place(&mut self) {
        self.channels.clear();
    }
}
