use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub pan_mode: Option<crate::lv1::PanMode>,
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
    pub internal_scene_id: Uuid,
    pub scene_index: Option<i32>,
    pub scene_name: String,
    pub duration_ms: u64,
    pub channel_configs: Vec<ChannelConfig>,
    pub scoped_channels: Vec<ChannelRef>,
    pub scope_toggles: SceneScopeToggles,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowDocument {
    pub lockout: bool,
    pub scene_configs: Vec<SceneConfig>,
    pub cued_scene_internal_id: Option<Uuid>,
}

impl ShowDocument {
    pub fn empty() -> Self {
        Self {
            lockout: false,
            scene_configs: Vec::new(),
            cued_scene_internal_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_config_serializes_internal_id_and_nullable_scene_index() {
        let internal_scene_id =
            uuid::Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let config = SceneConfig {
            internal_scene_id,
            scene_index: Some(0),
            scene_name: "Intro".to_string(),
            duration_ms: 1000,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        };

        let json = serde_json::to_value(config).unwrap();

        assert_eq!(json["internalSceneId"], internal_scene_id.to_string());
        assert_eq!(json["sceneIndex"], 0);
        assert!(json.get("sceneId").is_none());
    }

    #[test]
    fn scene_config_serializes_unlinked_scene_index_as_null() {
        let config = SceneConfig {
            internal_scene_id: uuid::Uuid::parse_str("22222222-2222-4222-8222-222222222222")
                .unwrap(),
            scene_index: None,
            scene_name: "Deleted Verse".to_string(),
            duration_ms: 1000,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        };

        let json = serde_json::to_value(config).unwrap();

        assert_eq!(json["sceneIndex"], serde_json::Value::Null);
        assert!(json.get("sceneId").is_none());
    }

    #[test]
    fn scene_config_serializes_pan_family_fields_for_frontend_camel_case() {
        let config = SceneConfig {
            internal_scene_id: Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
            scene_index: Some(0),
            scene_name: "Intro".to_string(),
            duration_ms: 1000,
            channel_configs: vec![ChannelConfig {
                group: 0,
                channel: 1,
                fader_db: Some(-6.0),
                pan: Some(0.25),
                balance: Some(-0.5),
                width: Some(1.0),
                pan_mode: Some(crate::lv1::PanMode::Stereo),
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
    fn scene_config_serializes_no_pan_mode_for_frontend_camel_case() {
        let config = SceneConfig {
            internal_scene_id: Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap(),
            scene_index: Some(0),
            scene_name: "Intro".to_string(),
            duration_ms: 1000,
            channel_configs: vec![ChannelConfig {
                group: 12,
                channel: 0,
                fader_db: Some(-6.0),
                pan: None,
                balance: None,
                width: None,
                pan_mode: Some(crate::lv1::PanMode::None),
            }],
            scoped_channels: vec![ChannelRef {
                group: 12,
                channel: 0,
            }],
            scope_toggles: SceneScopeToggles {
                faders: true,
                pan: true,
            },
        };

        let json = serde_json::to_value(config).unwrap();

        assert_eq!(json["channelConfigs"][0]["panMode"], "none");
    }

    #[test]
    fn show_document_serializes_cued_scene_internal_id_for_frontend_camel_case() {
        let snapshot = ShowDocument {
            lockout: false,
            scene_configs: Vec::new(),
            cued_scene_internal_id: Some(
                Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap(),
            ),
        };

        let json = serde_json::to_value(snapshot).unwrap();

        assert_eq!(
            json["cuedSceneInternalId"],
            "55555555-5555-4555-8555-555555555555"
        );
    }

    #[test]
    fn empty_show_document_has_no_cued_scene() {
        let snapshot = ShowDocument::empty();

        assert_eq!(snapshot.cued_scene_internal_id, None);
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
