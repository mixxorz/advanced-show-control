use tokio::sync::mpsc;

use super::commands::ScenesCommand;

#[derive(Clone)]
pub struct ScenesHandle {
    tx: mpsc::Sender<ScenesCommand>,
}

impl ScenesHandle {
    pub(super) fn new(tx: mpsc::Sender<ScenesCommand>) -> Self {
        Self { tx }
    }

    pub async fn send(
        &self,
        command: ScenesCommand,
    ) -> Result<(), mpsc::error::SendError<ScenesCommand>> {
        self.tx.send(command).await
    }
}
