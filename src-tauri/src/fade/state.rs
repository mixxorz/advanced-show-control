use crate::fade::events::FadeEvent;
use crate::fade::tick::ActiveTarget;
use crate::runtime::events::AppEventBus;

pub(crate) struct EngineState {
    generation: u64,
    pub(crate) channels: Vec<ActiveTarget>,
    pub(crate) event_bus: AppEventBus,
}

impl EngineState {
    pub(crate) fn new(event_bus: AppEventBus, generation: u64) -> Self {
        Self {
            generation,
            channels: Vec::new(),
            event_bus,
        }
    }

    pub(crate) fn fan_out(&mut self, event: FadeEvent) {
        self.event_bus.publish_fade(self.generation, event);
    }

    pub(crate) fn is_active(&self) -> bool {
        !self.channels.is_empty()
    }

    pub(crate) fn cancel_all_in_place(&mut self) {
        self.channels.clear();
    }
}
