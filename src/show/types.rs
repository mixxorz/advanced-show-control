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
    pub pan: Option<f64>,
    pub balance: Option<f64>,
    pub width: Option<f64>,
    pub pan_mode: Option<crate::lv1::types::PanMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct SceneScopeToggles {
    pub faders: bool,
    pub pan: bool,
}

impl Default for SceneScopeToggles {
    fn default() -> Self {
        Self {
            faders: true,
            pan: false,
        }
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
                pan: Some(0.25),
                balance: Some(-0.5),
                width: Some(1.0),
                pan_mode: Some(crate::lv1::types::PanMode::Stereo),
            }],
            scoped_channels: vec![ChannelRef {
                group: 0,
                channel: 1,
            }],
            scope_toggles: SceneScopeToggles {
                faders: true,
                pan: false,
            },
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

    #[test]
    fn scene_config_serializes_pan_family_fields_for_frontend_camel_case() {
        let config = SceneConfig {
            scene_id: "0::Intro".to_string(),
            scene_index: 0,
            scene_name: "Intro".to_string(),
            duration_ms: 1000,
            channel_configs: vec![ChannelConfig {
                group: 0,
                channel: 1,
                fader_db: Some(-6.0),
                pan: Some(0.25),
                balance: Some(-0.5),
                width: Some(1.0),
                pan_mode: Some(crate::lv1::types::PanMode::Stereo),
            }],
            scoped_channels: vec![ChannelRef {
                group: 0,
                channel: 1,
            }],
            scope_toggles: SceneScopeToggles {
                faders: true,
                pan: true,
            },
        };

        let json = serde_json::to_value(config).unwrap();

        assert_eq!(json["channelConfigs"][0]["pan"], 0.25);
        assert_eq!(json["channelConfigs"][0]["balance"], -0.5);
        assert_eq!(json["channelConfigs"][0]["width"], 1.0);
        assert_eq!(json["channelConfigs"][0]["panMode"], "stereo");
        assert_eq!(json["scopeToggles"]["pan"], true);
    }

    #[test]
    fn missing_pan_scope_defaults_to_false() {
        let json = serde_json::json!({
            "faders": true
        });

        let toggles: SceneScopeToggles = serde_json::from_value(json).unwrap();

        assert!(!toggles.pan);
        assert!(toggles.faders);
    }

    #[test]
    fn missing_fader_scope_defaults_to_true_when_scope_toggles_exist() {
        let json = serde_json::json!({
            "pan": true
        });

        let toggles: SceneScopeToggles = serde_json::from_value(json).unwrap();

        assert!(toggles.faders);
        assert!(toggles.pan);
    }
}
