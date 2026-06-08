use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelRef {
    pub group: i32,
    pub channel: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub channel: ChannelRef,
    pub scoped: bool,
    pub target_db: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneConfig {
    pub scene_id: String,
    pub duration_ms: u64,
    pub channels: Vec<ChannelConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShowSnapshot {
    pub lockout: bool,
    pub scene_configs: Vec<SceneConfig>,
}

pub fn scene_id(index: i32, name: &str) -> String {
    format!("{index}:{name}")
}
