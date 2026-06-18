use super::types::{ChannelInfo, SceneListEntry, SceneState};
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

#[derive(Debug, Clone)]
pub enum Lv1Event {
    Connected,
    Disconnected {
        /// Human-readable cause (e.g. "ping timeout", "TCP error: ..."),
        /// surfaced in logs so reconnect loops are diagnosable in the field.
        reason: String,
    },
    SceneChanged(SceneState),
    SceneListChanged(Vec<SceneListEntry>),
    FaderChanged {
        group: i32,
        channel: i32,
        gain_db: f64,
    },
    MuteChanged {
        group: i32,
        channel: i32,
        muted: bool,
    },
    PanChanged {
        group: i32,
        channel: i32,
        pan: f64,
    },
    BalanceChanged {
        group: i32,
        channel: i32,
        balance: f64,
    },
    WidthChanged {
        group: i32,
        channel: i32,
        width: f64,
    },
    ChannelTopologyChanged(Vec<ChannelInfo>),
}
