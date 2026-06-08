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
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(FadeCommand::RecallSceneFade { config, reply })
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
