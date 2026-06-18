use tokio::sync::{mpsc, oneshot};

use crate::fade::commands::FadeCommand;
use crate::fade::types::FadeConfig;
use crate::runtime::commands::AppCommandError;

#[derive(Clone)]
pub struct FadeEngineHandle {
    tx: mpsc::Sender<FadeCommand>,
}

impl FadeEngineHandle {
    pub fn new(tx: mpsc::Sender<FadeCommand>) -> Self {
        Self { tx }
    }

    pub async fn start_fade(&self, config: FadeConfig) -> Result<(), AppCommandError> {
        self.start_fade_with_generation(None, config).await
    }

    pub async fn start_fade_if_generation(
        &self,
        expected_generation: u64,
        config: FadeConfig,
    ) -> Result<(), AppCommandError> {
        self.start_fade_with_generation(Some(expected_generation), config)
            .await
    }

    async fn start_fade_with_generation(
        &self,
        expected_generation: Option<u64>,
        config: FadeConfig,
    ) -> Result<(), AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(FadeCommand::RecallSceneFade {
                config,
                expected_generation,
                reply,
            })
            .await
            .map_err(|_| AppCommandError::FadeUnavailable)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }

    pub async fn abort_all(&self) -> Result<(), AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(FadeCommand::AbortAll { reply })
            .await
            .map_err(|_| AppCommandError::FadeUnavailable)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }
}
