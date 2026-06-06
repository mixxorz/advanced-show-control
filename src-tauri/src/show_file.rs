use lv1_scene_fade_utility::lv1::state::{ChannelInfo, Lv1StateSnapshot, SceneListEntry};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const SHOW_FILE_SCHEMA_VERSION: u32 = 1;
#[allow(dead_code)]
pub const DEFAULT_DURATION_MS: u64 = 4000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFile {
    pub schema_version: u32,
    pub app_version: String,
    pub saved_at: String,
    pub safety: ShowFileSafety,
    pub scene_fade_configs: Vec<ShowFileSceneFadeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileSafety {
    pub lockout: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileSceneFadeConfig {
    pub scene_index: i32,
    pub scene_name: String,
    pub fade_enabled: bool,
    pub duration_ms: u64,
    pub fade_targets: Vec<ShowFileFadeTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShowFileFadeTarget {
    pub group: i32,
    pub channel: i32,
    pub channel_name: String,
    pub target_db: f64,
    pub enabled: bool,
    pub updated_at: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LoadValidationReport {
    pub removed_scenes: Vec<String>,
    pub removed_targets: Vec<String>,
}

impl LoadValidationReport {
    #[allow(dead_code)]
    pub fn removed_anything(&self) -> bool {
        !self.removed_scenes.is_empty() || !self.removed_targets.is_empty()
    }
}

pub fn validate_show_file(
    file: &mut ShowFile,
    lv1: &Lv1StateSnapshot,
) -> Result<LoadValidationReport, String> {
    if lv1.scene_list.is_empty() || lv1.channels.is_empty() {
        return Err("Open a show file after LV1 scenes and channels are loaded".to_string());
    }

    if file.schema_version != SHOW_FILE_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported show file schema version {}",
            file.schema_version
        ));
    }

    let mut report = LoadValidationReport::default();

    file.scene_fade_configs.retain_mut(|config| {
        let scene_matches = lv1
            .scene_list
            .iter()
            .any(|scene| scene.index == config.scene_index && scene.name == config.scene_name);

        if !scene_matches {
            report
                .removed_scenes
                .push(format!("{}: {}", config.scene_index, config.scene_name));
            return false;
        }

        config.fade_targets.retain(|target| {
            let target_matches = lv1.channels.iter().any(|channel| {
                channel.group == target.group
                    && channel.channel == target.channel
                    && channel.name == target.channel_name
            });

            if !target_matches {
                report.removed_targets.push(format!(
                    "{} {}/{} {}",
                    config.scene_name, target.group, target.channel, target.channel_name
                ));
            }

            target_matches
        });

        true
    });

    Ok(report)
}

pub fn read_show_file(path: &Path) -> Result<ShowFile, String> {
    let json = fs::read_to_string(path)
        .map_err(|err| format!("Failed to read show file {}: {err}", path.display()))?;

    serde_json::from_str(&json)
        .map_err(|err| format!("Failed to parse show file {}: {err}", path.display()))
}

pub fn write_show_file(path: &Path, file: &ShowFile, backup_dir: &Path) -> Result<(), String> {
    if path.exists() {
        create_backup(path, backup_dir)?;
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "Failed to create parent directory {}: {err}",
                parent.display()
            )
        })?;
    }

    let parent = path
        .parent()
        .ok_or_else(|| format!("Show file path has no parent: {}", path.display()))?;
    let temp_path = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("show"),
        current_timestamp()
    ));

    let json = serde_json::to_string_pretty(file)
        .map_err(|err| format!("Failed to serialize show file {}: {err}", path.display()))?;

    let write_result = (|| -> Result<(), String> {
        let mut temp_file = fs::File::create(&temp_path).map_err(|err| {
            format!(
                "Failed to create temp show file {}: {err}",
                temp_path.display()
            )
        })?;
        temp_file
            .write_all(json.as_bytes())
            .and_then(|_| temp_file.sync_all())
            .map_err(|err| {
                format!(
                    "Failed to write temp show file {}: {err}",
                    temp_path.display()
                )
            })?;
        fs::rename(&temp_path, path).map_err(|err| {
            format!(
                "Failed to replace show file {} from {}: {err}",
                path.display(),
                temp_path.display()
            )
        })
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }

    write_result
}

pub fn default_show_folder() -> PathBuf {
    dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LV1 Scene Fade Utility")
}

pub fn backup_folder() -> PathBuf {
    dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LV1 Scene Fade Utility")
        .join("backups")
}

fn create_backup(path: &Path, backup_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(backup_dir).map_err(|err| {
        format!(
            "Failed to create backup directory {}: {err}",
            backup_dir.display()
        )
    })?;

    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("show");
    let timestamp = current_timestamp();
    let mut candidate = backup_dir.join(format!("{timestamp}-{stem}.lv1show"));
    let mut suffix = 1usize;

    while candidate.exists() {
        candidate = backup_dir.join(format!("{timestamp}-{stem}-{suffix}.lv1show"));
        suffix += 1;
    }

    fs::copy(path, &candidate).map_err(|err| {
        format!(
            "Failed to create backup {} from {}: {err}",
            candidate.display(),
            path.display()
        )
    })?;

    Ok(())
}

fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn lv1_snapshot() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: lv1_scene_fade_utility::lv1::state::ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![
                SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                },
                SceneListEntry {
                    index: 2,
                    name: "Verse".to_string(),
                },
            ],
            channels: vec![
                ChannelInfo {
                    group: 0,
                    channel: 1,
                    name: "Kick".to_string(),
                    gain_db: -5.0,
                    muted: false,
                },
                ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Lead".to_string(),
                    gain_db: -8.0,
                    muted: false,
                },
            ],
        }
    }

    fn show_file() -> ShowFile {
        ShowFile {
            schema_version: 1,
            app_version: "0.1.0".to_string(),
            saved_at: "123".to_string(),
            safety: ShowFileSafety { lockout: true },
            scene_fade_configs: vec![ShowFileSceneFadeConfig {
                scene_index: 1,
                scene_name: "Intro".to_string(),
                fade_enabled: true,
                duration_ms: 4000,
                fade_targets: vec![ShowFileFadeTarget {
                    group: 0,
                    channel: 2,
                    channel_name: "Lead".to_string(),
                    target_db: -12.5,
                    enabled: true,
                    updated_at: "456".to_string(),
                }],
            }],
        }
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "lv1-scene-fade-utility-{}-{}-{}",
            name,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn save_show_file_writes_json_and_creates_backup_on_overwrite() {
        let temp_dir = temp_test_dir("write");
        let show_path = temp_dir.join("test.lv1show");
        let backup_dir = temp_dir.join("backups");
        let file = show_file();

        write_show_file(&show_path, &file, &backup_dir).unwrap();
        write_show_file(&show_path, &file, &backup_dir).unwrap();

        let json = fs::read_to_string(&show_path).unwrap();
        assert!(json.contains("\"sceneFadeConfigs\""));

        let backups = fs::read_dir(&backup_dir).unwrap().count();
        assert_eq!(backups, 1);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn read_show_file_parses_json() {
        let temp_dir = temp_test_dir("read");
        let show_path = temp_dir.join("test.lv1show");
        let json = serde_json::to_string_pretty(&show_file()).unwrap();

        fs::write(&show_path, json).unwrap();

        let loaded = read_show_file(&show_path).unwrap();
        assert_eq!(loaded, show_file());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn show_file_serializes_camel_case_json() {
        let json = serde_json::to_string_pretty(&show_file()).unwrap();

        assert!(json.contains("\"schemaVersion\": 1"));
        assert!(json.contains("\"sceneFadeConfigs\""));
        assert!(json.contains("\"durationMs\": 4000"));
        assert!(json.contains("\"channelName\": \"Lead\""));
    }

    #[test]
    fn validation_keeps_exact_scene_and_target_matches() {
        let report = validate_show_file(&mut show_file(), &lv1_snapshot()).unwrap();

        assert_eq!(report.removed_scenes.len(), 0);
        assert_eq!(report.removed_targets.len(), 0);
    }

    #[test]
    fn validation_deletes_scene_when_name_differs() {
        let mut file = show_file();
        file.scene_fade_configs[0].scene_name = "Renamed Intro".to_string();

        let report = validate_show_file(&mut file, &lv1_snapshot()).unwrap();

        assert!(file.scene_fade_configs.is_empty());
        assert_eq!(report.removed_scenes, vec!["1: Renamed Intro".to_string()]);
    }

    #[test]
    fn validation_deletes_target_when_channel_name_differs() {
        let mut file = show_file();
        file.scene_fade_configs[0].fade_targets[0].channel_name = "Vocal".to_string();

        let report = validate_show_file(&mut file, &lv1_snapshot()).unwrap();

        assert!(file.scene_fade_configs[0].fade_targets.is_empty());
        assert_eq!(report.removed_targets, vec!["Intro 0/2 Vocal".to_string()]);
    }

    #[test]
    fn validation_requires_scene_and_channel_lists() {
        let mut file = show_file();
        let mut snapshot = lv1_snapshot();
        snapshot.scene_list.clear();

        assert_eq!(
            validate_show_file(&mut file, &snapshot).unwrap_err(),
            "Open a show file after LV1 scenes and channels are loaded"
        );
    }
}
