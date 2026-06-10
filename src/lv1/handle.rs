use tokio::sync::{mpsc, oneshot};

use super::commands::Lv1Command;
use super::events::Lv1ActorError;
use super::types::Lv1StateSnapshot;

/// A cloneable handle to the LV1 actor. Use this to send commands.
#[derive(Clone)]
pub struct Lv1ActorHandle {
    tx: mpsc::Sender<Lv1Command>,
}

impl Lv1ActorHandle {
    pub(crate) fn new(tx: mpsc::Sender<Lv1Command>) -> Self {
        Self { tx }
    }

    /// Get a point-in-time snapshot of the current state.
    pub async fn get_state(&self) -> Lv1StateSnapshot {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.tx.send(Lv1Command::GetState { reply: reply_tx }).await;
        reply_rx
            .await
            .expect("actor dropped before responding to GetState")
    }

    /// Send a `/Set/Track/Out/Gain` command to LV1. Fire and forget.
    pub async fn set_gain(
        &self,
        group: i32,
        channel: i32,
        gain_db: f64,
    ) -> Result<(), Lv1ActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Lv1Command::SetGain {
                group,
                channel,
                gain_db,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Lv1ActorError::CommandChannelClosed)?;

        reply_rx
            .await
            .map_err(|_| Lv1ActorError::ReplyChannelClosed)?
    }

    /// Send a `/Set/Track/Out/Mute` command to LV1. Fire and forget.
    pub async fn set_mute(
        &self,
        group: i32,
        channel: i32,
        muted: bool,
    ) -> Result<(), Lv1ActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Lv1Command::SetMute {
                group,
                channel,
                muted,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Lv1ActorError::CommandChannelClosed)?;

        reply_rx
            .await
            .map_err(|_| Lv1ActorError::ReplyChannelClosed)?
    }

    /// Send a `/Set/Track/Pan` command to LV1. Fire and forget.
    pub async fn set_pan(&self, group: i32, channel: i32, value: f64) -> Result<(), Lv1ActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Lv1Command::SetPan {
                group,
                channel,
                value,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Lv1ActorError::CommandChannelClosed)?;

        reply_rx
            .await
            .map_err(|_| Lv1ActorError::ReplyChannelClosed)?
    }

    /// Send a `/Set/Track/Pan/Balance` command to LV1. Fire and forget.
    pub async fn set_balance(
        &self,
        group: i32,
        channel: i32,
        value: f64,
    ) -> Result<(), Lv1ActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Lv1Command::SetBalance {
                group,
                channel,
                value,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Lv1ActorError::CommandChannelClosed)?;

        reply_rx
            .await
            .map_err(|_| Lv1ActorError::ReplyChannelClosed)?
    }

    /// Send a `/Set/Track/Pan/Width` command to LV1. Fire and forget.
    pub async fn set_width(
        &self,
        group: i32,
        channel: i32,
        value: f64,
    ) -> Result<(), Lv1ActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Lv1Command::SetWidth {
                group,
                channel,
                value,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Lv1ActorError::CommandChannelClosed)?;

        reply_rx
            .await
            .map_err(|_| Lv1ActorError::ReplyChannelClosed)?
    }

    /// Wait until all previously queued commands have been processed.
    pub async fn flush(&self) -> Result<(), Lv1ActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Lv1Command::Flush { reply: reply_tx })
            .await
            .map_err(|_| Lv1ActorError::CommandChannelClosed)?;

        reply_rx
            .await
            .map_err(|_| Lv1ActorError::ReplyChannelClosed)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handle_sends_pan_family_commands() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(3);
        let handle = Lv1ActorHandle::new(tx);

        let pan = tokio::spawn({
            let handle = handle.clone();
            async move { handle.set_pan(1, 2, -0.5).await }
        });
        if let Some(Lv1Command::SetPan {
            group,
            channel,
            value,
            reply,
        }) = rx.recv().await
        {
            assert_eq!((group, channel, value), (1, 2, -0.5));
            let _ = reply.send(Ok(()));
        } else {
            panic!("expected SetPan command");
        }
        assert_eq!(pan.await.unwrap(), Ok(()));

        let balance = tokio::spawn({
            let handle = handle.clone();
            async move { handle.set_balance(3, 4, 0.25).await }
        });
        if let Some(Lv1Command::SetBalance {
            group,
            channel,
            value,
            reply,
        }) = rx.recv().await
        {
            assert_eq!((group, channel, value), (3, 4, 0.25));
            let _ = reply.send(Ok(()));
        } else {
            panic!("expected SetBalance command");
        }
        assert_eq!(balance.await.unwrap(), Ok(()));

        let width = tokio::spawn({
            let handle = handle.clone();
            async move { handle.set_width(5, 6, 0.75).await }
        });
        if let Some(Lv1Command::SetWidth {
            group,
            channel,
            value,
            reply,
        }) = rx.recv().await
        {
            assert_eq!((group, channel, value), (5, 6, 0.75));
            let _ = reply.send(Ok(()));
        } else {
            panic!("expected SetWidth command");
        }
        assert_eq!(width.await.unwrap(), Ok(()));
    }
}
