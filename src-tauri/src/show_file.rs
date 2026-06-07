use lv1_scene_fade_utility::lv1::model::{
    ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::OpenOptions;
use std::io::{self, Write};
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

    let json = serde_json::to_string_pretty(file)
        .map_err(|err| format!("Failed to serialize show file {}: {err}", path.display()))?;

    let parent = path
        .parent()
        .ok_or_else(|| format!("Show file path has no parent: {}", path.display()))?;
    let timestamp = current_timestamp();
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("show");
    let (temp_path, mut temp_file) = reserve_unique_temp_file(parent, file_name, &timestamp)?;

    let write_result = (|| -> Result<(), String> {
        temp_file
            .write_all(json.as_bytes())
            .and_then(|_| temp_file.sync_all())
            .map_err(|err| {
                format!(
                    "Failed to write temp show file {}: {err}",
                    temp_path.display()
                )
            })?;
        drop(temp_file);
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
    default_show_folder_from(dirs::document_dir(), dirs::home_dir())
}

pub fn backup_folder() -> PathBuf {
    backup_folder_from(dirs::data_dir(), dirs::home_dir())
}

fn default_show_folder_from(document_dir: Option<PathBuf>, home_dir: Option<PathBuf>) -> PathBuf {
    document_dir
        .or_else(|| home_dir.as_ref().map(|home| home.join("Documents")))
        .or(home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LV1 Scene Fade Utility")
}

fn backup_folder_from(data_dir: Option<PathBuf>, home_dir: Option<PathBuf>) -> PathBuf {
    data_dir
        .or_else(|| home_dir.clone())
        .unwrap_or_else(|| PathBuf::from("."))
        .join(app_data_folder_name())
        .join("backups")
}

fn app_data_folder_name() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "lv1-scene-fade-utility"
    }

    #[cfg(not(target_os = "linux"))]
    {
        "LV1 Scene Fade Utility"
    }
}

fn create_backup(path: &Path, backup_dir: &Path) -> Result<(), String> {
    let timestamp = current_timestamp();
    let (candidate, mut dest) = reserve_unique_backup_file(backup_dir, path, &timestamp)?;
    let mut source = fs::File::open(path)
        .map_err(|err| format!("Failed to open source show file {}: {err}", path.display()))?;

    io::copy(&mut source, &mut dest).map_err(|err| {
        format!(
            "Failed to create backup {} from {}: {err}",
            candidate.display(),
            path.display()
        )
    })?;

    dest.sync_all().map_err(|err| {
        format!(
            "Failed to flush backup {} from {}: {err}",
            candidate.display(),
            path.display()
        )
    })?;

    Ok(())
}

fn reserve_unique_backup_file(
    backup_dir: &Path,
    source_path: &Path,
    timestamp: &str,
) -> Result<(PathBuf, fs::File), String> {
    fs::create_dir_all(backup_dir).map_err(|err| {
        format!(
            "Failed to create backup directory {}: {err}",
            backup_dir.display()
        )
    })?;

    let stem = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("show");

    reserve_unique_file(backup_dir, |suffix| {
        if suffix == 0 {
            format!("{timestamp}-{stem}.lv1show")
        } else {
            format!("{timestamp}-{stem}-{suffix}.lv1show")
        }
    })
}

fn reserve_unique_temp_file(
    parent_dir: &Path,
    file_name: &str,
    timestamp: &str,
) -> Result<(PathBuf, fs::File), String> {
    reserve_unique_file(parent_dir, |suffix| {
        if suffix == 0 {
            format!(".{file_name}.tmp-{timestamp}")
        } else {
            format!(".{file_name}.tmp-{timestamp}-{suffix}")
        }
    })
}

fn reserve_unique_file<F>(
    directory: &Path,
    candidate_name: F,
) -> Result<(PathBuf, fs::File), String>
where
    F: Fn(usize) -> String,
{
    for suffix in 0.. {
        let candidate = directory.join(candidate_name(suffix));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(format!(
                    "Failed to reserve file {}: {err}",
                    candidate.display()
                ));
            }
        }
    }

    unreachable!("suffix loop is unbounded")
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
            connection: ConnectionStatus::Connected,
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
    fn default_show_folder_uses_home_documents_when_document_dir_is_missing() {
        let home = temp_test_dir("home-documents");
        let folder = default_show_folder_from(None, Some(home.clone()));

        assert_eq!(
            folder,
            home.join("Documents").join("LV1 Scene Fade Utility")
        );

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn app_data_folder_name_matches_platform_expectation() {
        #[cfg(target_os = "linux")]
        assert_eq!(app_data_folder_name(), "lv1-scene-fade-utility");

        #[cfg(not(target_os = "linux"))]
        assert_eq!(app_data_folder_name(), "LV1 Scene Fade Utility");
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
    fn reserve_unique_backup_file_adds_suffix_when_candidate_exists() {
        let backup_dir = temp_test_dir("backup-path");
        let candidate = backup_dir.join("123-test.lv1show");
        fs::write(&candidate, "taken").unwrap();

        let (path, _file) =
            reserve_unique_backup_file(&backup_dir, Path::new("test.lv1show"), "123").unwrap();

        assert_eq!(
            path.file_name().and_then(|value| value.to_str()),
            Some("123-test-1.lv1show")
        );

        let _ = fs::remove_dir_all(&backup_dir);
    }

    #[test]
    fn reserve_unique_temp_file_adds_suffix_when_candidate_exists() {
        let temp_dir = temp_test_dir("temp-path");
        let candidate = temp_dir.join(".test.lv1show.tmp-123");
        fs::write(&candidate, "taken").unwrap();

        let (path, _file) = reserve_unique_temp_file(&temp_dir, "test.lv1show", "123").unwrap();

        assert_eq!(
            path.file_name().and_then(|value| value.to_str()),
            Some(".test.lv1show.tmp-123-1")
        );

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
