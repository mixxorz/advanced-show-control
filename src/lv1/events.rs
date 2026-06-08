use super::types::{ChannelInfo, SceneListEntry, SceneState};

#[derive(Debug, Clone)]
pub enum Lv1Event {
    Connected,
    Disconnected,
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
    ChannelTopologyChanged(Vec<ChannelInfo>),
}
