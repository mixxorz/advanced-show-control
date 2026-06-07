use tokio::sync::mpsc;

use crate::fade::engine::FadeEngineHandle;
use crate::lv1::messages::Lv1ActorError;
use crate::lv1::state::Lv1ActorHandle;
use crate::runtime::commands::{AppCommand, AppCommandError};
use crate::runtime::events::{AppEvent, AppEventBus};

pub struct RuntimeDispatcher {
    rx: mpsc::Receiver<AppCommand>,
    event_bus: AppEventBus,
    lv1: Option<Lv1ActorHandle>,
    fade: Option<FadeEngineHandle>,
}

impl RuntimeDispatcher {
    pub fn new(rx: mpsc::Receiver<AppCommand>, event_bus: AppEventBus) -> Self {
        Self {
            rx,
            event_bus,
            lv1: None,
            fade: None,
        }
    }

    pub fn set_lv1(&mut self, lv1: Option<Lv1ActorHandle>) {
        self.lv1 = lv1;
    }

    pub fn set_fade(&mut self, fade: Option<FadeEngineHandle>) {
        self.fade = fade;
    }

    pub async fn run(mut self) {
        while let Some(command) = self.rx.recv().await {
            self.handle(command).await;
        }
    }

    async fn handle(&mut self, command: AppCommand) {
        match command {
            AppCommand::GetLv1State { reply } => {
                let result = match &self.lv1 {
                    Some(lv1) => Ok(lv1.get_state().await),
                    None => Err(AppCommandError::Lv1Unavailable),
                };
                let _ = reply.send(result);
            }
            AppCommand::SetGain {
                group,
                channel,
                gain_db,
                reply,
            } => {
                let result = match &self.lv1 {
                    Some(lv1) => lv1
                        .set_gain(group, channel, gain_db)
                        .await
                        .map_err(map_lv1_error),
                    None => Err(AppCommandError::Lv1Unavailable),
                };
                publish_failure(&self.event_bus, "set_gain", &result);
                let _ = reply.send(result);
            }
            AppCommand::StartFade { config, reply } => {
                let result = match &self.fade {
                    Some(fade) => {
                        fade.start_fade(config).await;
                        Ok(())
                    }
                    None => Err(AppCommandError::FadeUnavailable),
                };
                publish_failure(&self.event_bus, "start_fade", &result);
                let _ = reply.send(result);
            }
            AppCommand::AbortAllFades { reply } => {
                let result = match &self.fade {
                    Some(fade) => {
                        fade.abort_all().await;
                        Ok(())
                    }
                    None => Err(AppCommandError::FadeUnavailable),
                };
                publish_failure(&self.event_bus, "abort_all_fades", &result);
                let _ = reply.send(result);
            }
            AppCommand::FinishFadeNow { reply } => {
                let result = match &self.fade {
                    Some(fade) => {
                        fade.finish_now().await;
                        Ok(())
                    }
                    None => Err(AppCommandError::FadeUnavailable),
                };
                publish_failure(&self.event_bus, "finish_fade_now", &result);
                let _ = reply.send(result);
            }
        }
    }
}

fn map_lv1_error(error: Lv1ActorError) -> AppCommandError {
    AppCommandError::CommandFailed(error.to_string())
}

fn publish_failure(event_bus: &AppEventBus, command: &str, result: &Result<(), AppCommandError>) {
    if let Err(error) = result {
        event_bus.publish(AppEvent::CommandFailed {
            command: command.to_string(),
            message: error.to_string(),
        });
    }
}
