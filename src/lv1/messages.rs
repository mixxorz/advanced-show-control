use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Lv1ActorError {
    #[error("LV1 actor command channel is closed")]
    CommandChannelClosed,
    #[error("LV1 actor reply channel is closed")]
    ReplyChannelClosed,
    #[error("LV1 actor is not connected")]
    NotConnected,
    #[error("LV1 actor failed to send command to LV1")]
    CommandSendFailed,
}
