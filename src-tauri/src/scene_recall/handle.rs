use tokio::sync::mpsc;

use super::commands::SceneRecallCommand;

#[derive(Clone)]
pub struct SceneRecallFaderHandle {
    tx: mpsc::Sender<SceneRecallCommand>,
}

impl SceneRecallFaderHandle {
    pub(super) fn new(tx: mpsc::Sender<SceneRecallCommand>) -> Self {
        Self { tx }
    }

    pub async fn send(
        &self,
        command: SceneRecallCommand,
    ) -> Result<(), mpsc::error::SendError<SceneRecallCommand>> {
        self.tx.send(command).await
    }
}
