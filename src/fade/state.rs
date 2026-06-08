use crate::fade::events::FadeEvent;
use crate::fade::tick::ActiveChannel;
use crate::fade::types::FadeSceneIdentity;
use crate::runtime::events::{AppEvent, AppEventBus};

pub(crate) struct EngineState {
    pub(crate) channels: Vec<ActiveChannel>,
    event_bus: AppEventBus,
}

impl EngineState {
    pub(crate) fn new(event_bus: AppEventBus) -> Self {
        Self {
            channels: Vec::new(),
            event_bus,
        }
    }

    pub(crate) fn fan_out(&mut self, event: FadeEvent) {
        self.event_bus.publish(AppEvent::Fade(event));
    }

    pub(crate) fn is_active(&self) -> bool {
        !self.channels.is_empty()
    }

    pub(crate) fn has_active_scene(&self, scene: &FadeSceneIdentity) -> bool {
        self.channels.iter().any(|ch| &ch.scene == scene)
    }

    pub(crate) fn cancel_all_in_place(&mut self) {
        self.channels.clear();
    }
}

pub(crate) async fn finish_scene_channels(
    state: &mut EngineState,
    command_bus: &crate::runtime::commands::AppCommandBus,
    scene: &FadeSceneIdentity,
) {
    let mut completed = Vec::new();
    for ch in &mut state.channels {
        if &ch.scene == scene {
            let target_db = ch.exact_final_send();
            let _ = command_bus.set_gain(ch.group, ch.channel, target_db).await;
            completed.push((ch.group, ch.channel));
        }
    }

    state.channels.retain(|ch| &ch.scene != scene);
    for (group, channel) in completed {
        state.fan_out(FadeEvent::ChannelCompleted { group, channel });
    }
}
