use tokio::sync::mpsc;

use super::commands::Lv1Command;
use super::events::Lv1ActorError;

/// A cloneable handle to the LV1 actor. Use this to send commands.
#[derive(Clone)]
pub struct Lv1ActorHandle {
    tx: mpsc::Sender<Lv1Command>,
}

impl Lv1ActorHandle {
    pub(super) fn new(tx: mpsc::Sender<Lv1Command>) -> Self {
        Self { tx }
    }

    pub async fn send(&self, command: Lv1Command) -> Result<(), Lv1ActorError> {
        self.tx
            .send(command)
            .await
            .map_err(|_| Lv1ActorError::CommandChannelClosed)
    }

    pub async fn reserve(&self) -> Result<mpsc::Permit<'_, Lv1Command>, Lv1ActorError> {
        self.tx
            .reserve()
            .await
            .map_err(|_| Lv1ActorError::CommandChannelClosed)
    }
}
