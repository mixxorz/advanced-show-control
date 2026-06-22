use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::lv1::{Lv1StateSnapshot, PanMode};
use crate::show::types::{
    ChannelConfig, ChannelRef, SceneConfig, SceneScopeToggles, ShowDocument, scene_id,
};

pub const SHOW_FILE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFile {
    pub schema_version: u32,
    pub app_version: String,
    pub saved_at: String,
    pub safety: ShowFileSafety,
    pub scene_configs: Vec<ShowFileSceneConfig>,
    #[serde(default)]
    pub cued_scene_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileSafety {
    pub lockout: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileSceneConfig {
    pub scene_index: i32,
    pub scene_name: String,
    pub duration_ms: u64,
    pub channel_configs: Vec<ShowFileChannelConfig>,
    pub scoped_channels: Vec<ShowFileChannelRef>,
    #[serde(default)]
    pub scope_toggles: ShowFileSceneScopeToggles,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct ShowFileSceneScopeToggles {
    pub faders: bool,
    pub pan: bool,
}

impl Default for ShowFileSceneScopeToggles {
    fn default() -> Self {
        Self {
            faders: true,
            pan: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileChannelConfig {
    pub group: i32,
    pub channel: i32,
    pub fader_db: Option<f64>,
    pub pan: Option<f64>,
    pub balance: Option<f64>,
    pub width: Option<f64>,
    pub pan_mode: Option<PanMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileChannelRef {
    pub group: i32,
    pub channel: i32,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LoadValidationReport {
    pub removed_scenes: Vec<String>,
}

impl LoadValidationReport {
    pub fn removed_anything(&self) -> bool {
        !self.removed_scenes.is_empty()
    }
}

pub struct ImportedShowFile {
    pub snapshot: ShowDocument,
    pub selected_scene_id: Option<String>,
    pub report: LoadValidationReport,
}

pub fn export_show_file(snapshot: ShowDocument, saved_at: String) -> ShowFile {
    ShowFile {
        schema_version: SHOW_FILE_SCHEMA_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        saved_at,
        safety: ShowFileSafety {
            lockout: snapshot.lockout,
        },
        cued_scene_id: snapshot.cued_scene_id,
        scene_configs: snapshot
            .scene_configs
            .into_iter()
            .map(show_scene_to_file_scene)
            .collect(),
    }
}

pub fn import_show_file(
    file: &mut ShowFile,
    lv1: &Lv1StateSnapshot,
) -> Result<ImportedShowFile, String> {
    let report = prune_show_file_to_lv1_scenes(file, lv1)?;
    let kept_scene_ids = file
        .scene_configs
        .iter()
        .map(|config| scene_id(config.scene_index, &config.scene_name))
        .collect::<HashSet<_>>();
    let selected_scene_id = file
        .scene_configs
        .first()
        .map(|config| scene_id(config.scene_index, &config.scene_name));
    let snapshot = ShowDocument {
        lockout: file.safety.lockout,
        scene_configs: file
            .scene_configs
            .iter()
            .map(file_scene_to_show_scene)
            .collect(),
        cued_scene_id: file
            .cued_scene_id
            .clone()
            .filter(|scene_id| kept_scene_ids.contains(scene_id)),
    };

    Ok(ImportedShowFile {
        snapshot,
        selected_scene_id,
        report,
    })
}

pub fn prune_show_file_to_lv1_scenes(
    file: &mut ShowFile,
    lv1: &Lv1StateSnapshot,
) -> Result<LoadValidationReport, String> {
    if lv1.scene_list.is_empty() {
        return Err("Open a session after LV1 scenes are loaded".to_string());
    }

    if file.schema_version != SHOW_FILE_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported session schema version {}",
            file.schema_version
        ));
    }

    let mut report = LoadValidationReport::default();
    file.scene_configs.retain(|config| {
        let scene_matches = lv1
            .scene_list
            .iter()
            .any(|scene| scene.index == config.scene_index && scene.name == config.scene_name);
        if !scene_matches {
            report
                .removed_scenes
                .push(format!("{}: {}", config.scene_index, config.scene_name));
        }
        scene_matches
    });
    Ok(report)
}

fn show_scene_to_file_scene(config: SceneConfig) -> ShowFileSceneConfig {
    ShowFileSceneConfig {
        scene_index: config.scene_index,
        scene_name: config.scene_name,
        duration_ms: config.duration_ms,
        channel_configs: config
            .channel_configs
            .into_iter()
            .map(|target| ShowFileChannelConfig {
                group: target.group,
                channel: target.channel,
                fader_db: target.fader_db,
                pan: target.pan,
                balance: target.balance,
                width: target.width,
                pan_mode: target.pan_mode,
            })
            .collect(),
        scoped_channels: config
            .scoped_channels
            .into_iter()
            .map(|channel| ShowFileChannelRef {
                group: channel.group,
                channel: channel.channel,
            })
            .collect(),
        scope_toggles: ShowFileSceneScopeToggles {
            faders: config.scope_toggles.faders,
            pan: config.scope_toggles.pan,
        },
    }
}

fn file_scene_to_show_scene(config: &ShowFileSceneConfig) -> SceneConfig {
    SceneConfig {
        scene_id: scene_id(config.scene_index, &config.scene_name),
        scene_index: config.scene_index,
        scene_name: config.scene_name.clone(),
        duration_ms: config.duration_ms,
        channel_configs: config
            .channel_configs
            .iter()
            .map(|target| ChannelConfig {
                group: target.group,
                channel: target.channel,
                fader_db: target.fader_db,
                pan: target.pan,
                balance: target.balance,
                width: target.width,
                pan_mode: target.pan_mode.clone(),
            })
            .collect(),
        scoped_channels: config
            .scoped_channels
            .iter()
            .map(|channel| ChannelRef {
                group: channel.group,
                channel: channel.channel,
            })
            .collect(),
        scope_toggles: SceneScopeToggles {
            faders: config.scope_toggles.faders,
            pan: config.scope_toggles.pan,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::{ConnectionStatus, SceneListEntry};

    #[test]
    fn export_show_file_contains_current_configs() {
        let snapshot = ShowDocument {
            lockout: true,
            cued_scene_id: Some("1:Intro".to_string()),
            scene_configs: vec![SceneConfig {
                scene_id: "1:Intro".to_string(),
                scene_index: 1,
                scene_name: "Intro".to_string(),
                duration_ms: 5_000,
                channel_configs: vec![ChannelConfig {
                    group: 0,
                    channel: 2,
                    fader_db: Some(-8.0),
                    pan: Some(-12.0),
                    balance: Some(3.0),
                    width: Some(1.2),
                    pan_mode: Some(PanMode::Stereo),
                }],
                scoped_channels: vec![ChannelRef {
                    group: 0,
                    channel: 2,
                }],
                scope_toggles: SceneScopeToggles {
                    faders: false,
                    pan: true,
                },
            }],
        };

        let file = export_show_file(snapshot, "saved".to_string());

        assert_eq!(file.schema_version, SHOW_FILE_SCHEMA_VERSION);
        assert_eq!(file.saved_at, "saved");
        assert!(file.safety.lockout);
        assert_eq!(file.cued_scene_id, Some("1:Intro".to_string()));
        assert_eq!(file.scene_configs[0].scene_index, 1);
        assert_eq!(
            file.scene_configs[0].channel_configs[0].fader_db,
            Some(-8.0)
        );
        assert!(!file.scene_configs[0].scope_toggles.faders);
        assert!(file.scene_configs[0].scope_toggles.pan);
    }

    #[test]
    fn import_show_file_prunes_missing_scenes_and_filters_cue() {
        let mut file = ShowFile {
            schema_version: SHOW_FILE_SCHEMA_VERSION,
            app_version: "0.1.0".to_string(),
            saved_at: "123".to_string(),
            safety: ShowFileSafety { lockout: true },
            cued_scene_id: Some("2:Missing".to_string()),
            scene_configs: vec![
                ShowFileSceneConfig {
                    scene_index: 1,
                    scene_name: "Intro".to_string(),
                    duration_ms: 5_000,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: ShowFileSceneScopeToggles::default(),
                },
                ShowFileSceneConfig {
                    scene_index: 2,
                    scene_name: "Missing".to_string(),
                    duration_ms: 5_000,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: ShowFileSceneScopeToggles::default(),
                },
            ],
        };
        let lv1 = Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }],
            channels: Vec::new(),
        };

        let imported = import_show_file(&mut file, &lv1).unwrap();

        assert!(imported.report.removed_anything());
        assert_eq!(
            imported.report.removed_scenes,
            vec!["2: Missing".to_string()]
        );
        assert_eq!(imported.snapshot.scene_configs.len(), 1);
        assert_eq!(imported.snapshot.scene_configs[0].scene_id, "1::Intro");
        assert_eq!(imported.snapshot.cued_scene_id, None);
    }
}
