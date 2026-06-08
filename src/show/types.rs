use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelRef {
    pub group: i32,
    pub channel: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub group: i32,
    pub channel: i32,
    pub fader_db: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneScopeToggles {
    pub faders: bool,
}

impl Default for SceneScopeToggles {
    fn default() -> Self {
        Self { faders: true }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneConfig {
    pub scene_id: String,
    pub scene_index: i32,
    pub scene_name: String,
    pub duration_ms: u64,
    pub channel_configs: Vec<ChannelConfig>,
    pub scoped_channels: Vec<ChannelRef>,
    pub scope_toggles: SceneScopeToggles,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShowSnapshot {
    pub lockout: bool,
    pub scene_configs: Vec<SceneConfig>,
}

impl ShowSnapshot {
    pub fn empty() -> Self {
        Self {
            lockout: false,
            scene_configs: Vec::new(),
        }
    }
}

pub fn scene_id(index: i32, name: &str) -> String {
    format!("{index}::{name}")
}
