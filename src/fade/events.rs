#[derive(Debug, Clone)]
pub enum FadeEvent {
    FadeStarted,
    FadeCompleted,
    FadeAborted,
    ChannelCompleted { group: i32, channel: i32 },
    ChannelOverride { group: i32, channel: i32 },
    ChannelCancelled { group: i32, channel: i32 },
}
