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
