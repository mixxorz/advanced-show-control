use tokio::sync::mpsc;

use crate::fade::commands::FadeCommand;

#[derive(Clone)]
pub struct FadeEngineHandle {
    tx: mpsc::Sender<FadeCommand>,
}

impl FadeEngineHandle {
    pub fn new(tx: mpsc::Sender<FadeCommand>) -> Self {
        Self { tx }
    }

    pub async fn send(
        &self,
        command: FadeCommand,
    ) -> Result<(), mpsc::error::SendError<FadeCommand>> {
        self.tx.send(command).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handle_sends_abort_without_reply() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let handle = FadeEngineHandle::new(tx);

        assert!(
            handle
                .send(FadeCommand::AbortAll { reply: None })
                .await
                .is_ok()
        );

        if let Some(FadeCommand::AbortAll { reply }) = rx.recv().await {
            assert!(reply.is_none());
        } else {
            panic!("expected AbortAll command");
        }
    }
}
