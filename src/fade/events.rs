use crate::fade::types::FadeParameter;

#[derive(Debug, Clone)]
pub enum FadeEvent {
    FadeStarted,
    FadeCompleted,
    FadeAborted,
    ChannelCompleted {
        group: i32,
        channel: i32,
        parameter: FadeParameter,
    },
    ChannelOverride {
        group: i32,
        channel: i32,
        parameter: FadeParameter,
    },
    ChannelCancelled {
        group: i32,
        channel: i32,
        parameter: FadeParameter,
    },
    WriteFailed {
        reason: String,
    },
}
