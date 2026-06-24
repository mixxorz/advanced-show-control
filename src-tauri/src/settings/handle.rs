use tokio::sync::mpsc;

use super::SettingsCommand;

#[derive(Clone)]
pub struct SettingsHandle {
    tx: mpsc::Sender<SettingsCommand>,
}

impl SettingsHandle {
    pub(crate) fn new(tx: mpsc::Sender<SettingsCommand>) -> Self {
        Self { tx }
    }

    pub async fn send(
        &self,
        command: SettingsCommand,
    ) -> Result<(), mpsc::error::SendError<SettingsCommand>> {
        self.tx.send(command).await
    }
}
