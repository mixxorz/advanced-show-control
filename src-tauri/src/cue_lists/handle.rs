use tokio::sync::mpsc;

use super::CueListsCommand;

#[derive(Clone)]
pub struct CueListsHandle {
    tx: mpsc::Sender<CueListsCommand>,
}

impl CueListsHandle {
    pub fn new(tx: mpsc::Sender<CueListsCommand>) -> Self {
        Self { tx }
    }

    pub async fn send(
        &self,
        command: CueListsCommand,
    ) -> Result<(), mpsc::error::SendError<CueListsCommand>> {
        self.tx.send(command).await
    }
}
