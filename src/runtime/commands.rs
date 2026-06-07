use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use crate::fade::types::FadeConfig;
use crate::lv1::model::Lv1StateSnapshot;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum AppCommandError {
    #[error("app command dispatcher is closed")]
    DispatcherClosed,
    #[error("app command reply channel is closed")]
    ReplyChannelClosed,
    #[error("LV1 actor is unavailable")]
    Lv1Unavailable,
    #[error("fade engine is unavailable")]
    FadeUnavailable,
    #[error("command failed: {0}")]
    CommandFailed(String),
}

pub enum AppCommand {
    GetLv1State {
        reply: oneshot::Sender<Result<Lv1StateSnapshot, AppCommandError>>,
    },
    SetGain {
        group: i32,
        channel: i32,
        gain_db: f64,
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    StartFade {
        config: FadeConfig,
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    AbortAllFades {
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    FinishFadeNow {
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
}

#[derive(Clone)]
pub struct AppCommandBus {
    tx: mpsc::Sender<AppCommand>,
}

impl AppCommandBus {
    pub fn new(tx: mpsc::Sender<AppCommand>) -> Self {
        Self { tx }
    }

    pub async fn get_lv1_state(&self) -> Result<Lv1StateSnapshot, AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AppCommand::GetLv1State { reply })
            .await
            .map_err(|_| AppCommandError::DispatcherClosed)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }

    pub async fn set_gain(
        &self,
        group: i32,
        channel: i32,
        gain_db: f64,
    ) -> Result<(), AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AppCommand::SetGain {
                group,
                channel,
                gain_db,
                reply,
            })
            .await
            .map_err(|_| AppCommandError::DispatcherClosed)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }

    pub async fn start_fade(&self, config: FadeConfig) -> Result<(), AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AppCommand::StartFade { config, reply })
            .await
            .map_err(|_| AppCommandError::DispatcherClosed)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }

    pub async fn abort_all_fades(&self) -> Result<(), AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AppCommand::AbortAllFades { reply })
            .await
            .map_err(|_| AppCommandError::DispatcherClosed)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }

    pub async fn finish_fade_now(&self) -> Result<(), AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AppCommand::FinishFadeNow { reply })
            .await
            .map_err(|_| AppCommandError::DispatcherClosed)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::curve::FadeCurve;
    use crate::fade::types::{FadeConfig, FadeTarget};

    #[tokio::test]
    async fn closed_dispatcher_returns_error() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let bus = AppCommandBus::new(tx);

        let err = bus.abort_all_fades().await.unwrap_err();

        assert_eq!(err, AppCommandError::DispatcherClosed);
    }

    #[tokio::test]
    async fn start_fade_sends_acknowledged_command() {
        let (tx, mut rx) = mpsc::channel(1);
        let bus = AppCommandBus::new(tx);
        let config = FadeConfig {
            targets: vec![FadeTarget {
                group: 0,
                channel: 1,
                target_db: -12.0,
            }],
            duration_ms: 1_000,
            curve: FadeCurve::Linear,
        };

        let task = tokio::spawn(async move { bus.start_fade(config).await });

        match rx.recv().await.unwrap() {
            AppCommand::StartFade { config, reply } => {
                assert_eq!(config.targets[0].channel, 1);
                reply.send(Ok(())).unwrap();
            }
            _ => panic!("unexpected command"),
        }

        assert_eq!(task.await.unwrap(), Ok(()));
    }
}
