use lv1_scene_fade_utility::lv1::model::{
    ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry,
};

pub(super) fn connected_snapshot() -> Lv1StateSnapshot {
    Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: None,
        scene_list: Vec::new(),
        channels: Vec::new(),
    }
}

pub(super) fn connected_state_with_scene_and_channel() -> Lv1StateSnapshot {
    Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: None,
        scene_list: vec![SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        }],
        channels: vec![ChannelInfo {
            group: 0,
            channel: 2,
            name: "Lead".to_string(),
            gain_db: -8.0,
            muted: false,
        }],
    }
}
