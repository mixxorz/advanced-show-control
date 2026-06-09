use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRef {
    pub group: i32,
    pub channel: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelConfig {
    pub group: i32,
    pub channel: i32,
    pub fader_db: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneScopeToggles {
    pub faders: bool,
}

impl Default for SceneScopeToggles {
    fn default() -> Self {
        Self { faders: true }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_config_serializes_for_frontend_camel_case() {
        let config = SceneConfig {
            scene_id: "0::Intro".to_string(),
            scene_index: 0,
            scene_name: "Intro".to_string(),
            duration_ms: 1000,
            channel_configs: vec![ChannelConfig {
                group: 0,
                channel: 1,
                fader_db: Some(-6.0),
            }],
            scoped_channels: vec![ChannelRef {
                group: 0,
                channel: 1,
            }],
            scope_toggles: SceneScopeToggles { faders: true },
        };

        let json = serde_json::to_value(config).unwrap();

        assert_eq!(json["sceneId"], "0::Intro");
        assert_eq!(json["sceneIndex"], 0);
        assert_eq!(json["sceneName"], "Intro");
        assert_eq!(json["durationMs"], 1000);
        assert_eq!(json["channelConfigs"][0]["faderDb"], -6.0);
        assert_eq!(json["scopedChannels"][0]["channel"], 1);
        assert_eq!(json["scopeToggles"]["faders"], true);
    }
}
