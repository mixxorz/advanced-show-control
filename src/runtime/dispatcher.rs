use tokio::sync::{mpsc, oneshot};

use crate::fade::engine::FadeEngineHandle;
use crate::fade::types::FadeConfig;
use crate::lv1::messages::Lv1ActorError;
use crate::lv1::state::Lv1ActorHandle;
use crate::runtime::commands::{AppCommand, AppCommandError};
use crate::runtime::events::{AppEvent, AppEventBus};

enum FadeRouteCommand {
    StartFade {
        fade: FadeEngineHandle,
        config: FadeConfig,
        event_bus: AppEventBus,
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    AbortAllFades {
        fade: FadeEngineHandle,
        event_bus: AppEventBus,
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    FinishFadeNow {
        fade: FadeEngineHandle,
        event_bus: AppEventBus,
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
}

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
        let (fade_route_tx, fade_route_rx) = mpsc::channel(32);
        tokio::spawn(run_fade_route_worker(fade_route_rx));

        while let Some(command) = self.rx.recv().await {
            self.handle(command, &fade_route_tx).await;
        }
    }

    async fn handle(
        &mut self,
        command: AppCommand,
        fade_route_tx: &mpsc::Sender<FadeRouteCommand>,
    ) {
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
                match &self.fade {
                    Some(fade) => {
                        let route = FadeRouteCommand::StartFade {
                            fade: fade.clone(),
                            config,
                            event_bus: self.event_bus.clone(),
                            reply,
                        };
                        match fade_route_tx.try_send(route) {
                            Ok(()) => {}
                            Err(err) => {
                                let route = err.into_inner();
                                let (event_bus, reply) = match route {
                                    FadeRouteCommand::StartFade { event_bus, reply, .. } => (event_bus, reply),
                                    _ => unreachable!("unexpected fade route command"),
                                };
                                let result = Err(AppCommandError::FadeUnavailable);
                                publish_failure(&event_bus, "start_fade", &result);
                                let _ = reply.send(result);
                            }
                        }
                    }
                    None => {
                        let result = Err(AppCommandError::FadeUnavailable);
                        publish_failure(&self.event_bus, "start_fade", &result);
                        let _ = reply.send(result);
                    }
                }
            }
            AppCommand::AbortAllFades { reply } => {
                match &self.fade {
                    Some(fade) => {
                        let route = FadeRouteCommand::AbortAllFades {
                            fade: fade.clone(),
                            event_bus: self.event_bus.clone(),
                            reply,
                        };
                        match fade_route_tx.try_send(route) {
                            Ok(()) => {}
                            Err(err) => {
                                let route = err.into_inner();
                                let (event_bus, reply) = match route {
                                    FadeRouteCommand::AbortAllFades { event_bus, reply, .. } => {
                                        (event_bus, reply)
                                    }
                                    _ => unreachable!("unexpected fade route command"),
                                };
                                let result = Err(AppCommandError::FadeUnavailable);
                                publish_failure(&event_bus, "abort_all_fades", &result);
                                let _ = reply.send(result);
                            }
                        }
                    }
                    None => {
                        let result = Err(AppCommandError::FadeUnavailable);
                        publish_failure(&self.event_bus, "abort_all_fades", &result);
                        let _ = reply.send(result);
                    }
                }
            }
            AppCommand::FinishFadeNow { reply } => {
                match &self.fade {
                    Some(fade) => {
                        let route = FadeRouteCommand::FinishFadeNow {
                            fade: fade.clone(),
                            event_bus: self.event_bus.clone(),
                            reply,
                        };
                        match fade_route_tx.try_send(route) {
                            Ok(()) => {}
                            Err(err) => {
                                let route = err.into_inner();
                                let (event_bus, reply) = match route {
                                    FadeRouteCommand::FinishFadeNow { event_bus, reply, .. } => {
                                        (event_bus, reply)
                                    }
                                    _ => unreachable!("unexpected fade route command"),
                                };
                                let result = Err(AppCommandError::FadeUnavailable);
                                publish_failure(&event_bus, "finish_fade_now", &result);
                                let _ = reply.send(result);
                            }
                        }
                    }
                    None => {
                        let result = Err(AppCommandError::FadeUnavailable);
                        publish_failure(&self.event_bus, "finish_fade_now", &result);
                        let _ = reply.send(result);
                    }
                }
            }
        }
    }
}

async fn run_fade_route_worker(mut rx: mpsc::Receiver<FadeRouteCommand>) {
    while let Some(command) = rx.recv().await {
        match command {
            FadeRouteCommand::StartFade {
                fade,
                config,
                event_bus,
                reply,
            } => {
                let result = fade.start_fade(config).await;
                publish_failure(&event_bus, "start_fade", &result);
                let _ = reply.send(result);
            }
            FadeRouteCommand::AbortAllFades {
                fade,
                event_bus,
                reply,
            } => {
                let result = fade.abort_all().await;
                publish_failure(&event_bus, "abort_all_fades", &result);
                let _ = reply.send(result);
            }
            FadeRouteCommand::FinishFadeNow {
                fade,
                event_bus,
                reply,
            } => {
                let result = fade.finish_now().await;
                publish_failure(&event_bus, "finish_fade_now", &result);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::curve::FadeCurve;
    use crate::fade::engine::FadeEngineHandle;
    use crate::fade::types::{FadeCommand, FadeConfig, FadeTarget};
    use crate::runtime::commands::AppCommandBus;
    use std::time::Duration;

    #[tokio::test]
    async fn fade_commands_are_processed_in_dispatcher_order() {
        let (command_tx, command_rx) = mpsc::channel(8);
        let command_bus = AppCommandBus::new(command_tx);
        let event_bus = AppEventBus::default();

        let (fade_tx, mut fade_rx) = mpsc::channel(8);
        let fade = FadeEngineHandle::new(fade_tx);

        let mut dispatcher = RuntimeDispatcher::new(command_rx, event_bus);
        dispatcher.set_fade(Some(fade));
        tokio::spawn(async move { dispatcher.run().await });

        let start_task = tokio::spawn({
            let command_bus = command_bus.clone();
            async move {
                command_bus
                    .start_fade(FadeConfig {
                        targets: vec![FadeTarget {
                            group: 0,
                            channel: 0,
                            target_db: -10.0,
                        }],
                        duration_ms: 250,
                        curve: FadeCurve::Linear,
                    })
                    .await
            }
        });

        let start_reply = match fade_rx.recv().await.unwrap() {
            FadeCommand::StartFade { reply, .. } => reply,
            other => panic!("unexpected first command: {other:?}"),
        };

        let abort_task = tokio::spawn({
            let command_bus = command_bus.clone();
            async move { command_bus.abort_all_fades().await }
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(fade_rx.try_recv().is_err());

        start_reply.send(Ok(())).unwrap();

        let abort_reply = match fade_rx.recv().await.unwrap() {
            FadeCommand::AbortAll { reply } => reply,
            other => panic!("unexpected second command: {other:?}"),
        };
        abort_reply.send(Ok(())).unwrap();

        assert_eq!(start_task.await.unwrap(), Ok(()));
        assert_eq!(abort_task.await.unwrap(), Ok(()));
    }
}
