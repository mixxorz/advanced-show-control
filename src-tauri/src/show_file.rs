pub use crate::show::{
    ImportedShowFile, SHOW_FILE_SCHEMA_VERSION, ShowFile, ShowFileSafety, ShowFileSceneConfig,
    export_show_file, import_show_file,
};

use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
const MAX_BACKUPS_PER_SHOW_FILE: usize = 10;

pub fn read_show_file(path: &Path) -> Result<ShowFile, String> {
    let json = fs::read_to_string(path)
        .map_err(|err| format!("Failed to read session {}: {err}", path.display()))?;

    serde_json::from_str(&json)
        .map_err(|err| format!("Failed to parse session {}: {err}", path.display()))
}

pub fn write_show_file(path: &Path, file: &ShowFile, backup_dir: &Path) -> Result<(), String> {
    if path.exists() {
        create_backup(path, backup_dir)?;
    }

    let json = serde_json::to_string_pretty(file)
        .map_err(|err| format!("Failed to serialize session {}: {err}", path.display()))?;

    crate::atomic_file::StagedFile::prepare(path, json.as_bytes())
        .map_err(|err| format!("Failed to stage session {}: {err}", path.display()))?
        .publish()
        .map_err(|err| format!("Failed to replace session {}: {err}", path.display()))
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
        .join("Advanced Show Control")
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
        "advanced-show-control"
    }

    #[cfg(not(target_os = "linux"))]
    {
        "Advanced Show Control"
    }
}

fn create_backup(path: &Path, backup_dir: &Path) -> Result<(), String> {
    let timestamp = crate::time::current_timestamp_millis();
    let (candidate, staged_path, mut dest) =
        reserve_unique_backup_file(backup_dir, path, &timestamp)?;

    let backup_result = (|| -> Result<(), String> {
        let mut source = fs::File::open(path)
            .map_err(|err| format!("Failed to open source session {}: {err}", path.display()))?;

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
        drop(dest);

        fs::rename(&staged_path, &candidate).map_err(|err| {
            format!(
                "Failed to publish backup {} from {}: {err}",
                candidate.display(),
                staged_path.display()
            )
        })?;

        prune_old_backups(backup_dir, path, MAX_BACKUPS_PER_SHOW_FILE)
    })();

    if backup_result.is_err() {
        let _ = fs::remove_file(&staged_path);
    }

    backup_result
}

fn prune_old_backups(
    backup_dir: &Path,
    source_path: &Path,
    max_backups: usize,
) -> Result<(), String> {
    let stem = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("show");

    let backups: Vec<(SystemTime, String, PathBuf)> = fs::read_dir(backup_dir)
        .map_err(|err| {
            format!(
                "Failed to read backup directory {}: {err}",
                backup_dir.display()
            )
        })?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?.to_string();
            if is_backup_for_show_file(&name, stem) {
                let modified = entry.metadata().ok()?.modified().ok()?;
                Some((modified, name, path))
            } else {
                None
            }
        })
        .collect();

    for path in prune_backup_entries(backups, max_backups).into_iter() {
        let _ = fs::remove_file(path);
    }

    Ok(())
}

fn is_backup_for_show_file(name: &str, stem: &str) -> bool {
    let Some(prefix) = name.strip_suffix(".ascs") else {
        return false;
    };

    let Some((_, source)) = prefix.split_once('-') else {
        return false;
    };

    source == stem || source.starts_with(&format!("{stem}__backup"))
}

fn reserve_unique_backup_file(
    backup_dir: &Path,
    source_path: &Path,
    timestamp: &str,
) -> Result<(PathBuf, PathBuf, fs::File), String> {
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

    for suffix in 0.. {
        let file_name = if suffix == 0 {
            format!("{timestamp}-{stem}.ascs")
        } else {
            format!("{timestamp}-{stem}__backup{suffix}.ascs")
        };
        let candidate = backup_dir.join(&file_name);
        if candidate.exists() {
            continue;
        }

        let staged_path = backup_dir.join(format!(".{file_name}.tmp"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged_path)
        {
            Ok(file) => return Ok((candidate, staged_path, file)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(format!(
                    "Failed to reserve file {}: {err}",
                    staged_path.display()
                ));
            }
        }
    }

    unreachable!("suffix loop is unbounded")
}

fn prune_backup_entries(
    mut backups: Vec<(SystemTime, String, PathBuf)>,
    max_backups: usize,
) -> Vec<PathBuf> {
    backups.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let prune_count = backups.len().saturating_sub(max_backups);
    backups
        .into_iter()
        .take(prune_count)
        .map(|(_, _, path)| path)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenes::{ChannelConfig, ChannelRef, SceneScopeToggles};
    use std::time::{Duration, UNIX_EPOCH};

    fn show_file() -> ShowFile {
        ShowFile {
            schema_version: SHOW_FILE_SCHEMA_VERSION,
            app_version: "0.1.0".to_string(),
            saved_at: "123".to_string(),
            safety: ShowFileSafety { lockout: true },
            cue_lists: Vec::new(),
            active_cue_list_id: None,
            cued_cue_entry_id: None,
            scene_configs: vec![ShowFileSceneConfig {
                internal_scene_id: Some(uuid::Uuid::from_u128(0x11111111111141118111111111111111)),
                scene_index: Some(1),
                scene_name: "Intro".to_string(),
                duration_ms: 4000,
                channel_configs: vec![ChannelConfig {
                    group: 0,
                    channel: 2,
                    fader_db: Some(-12.5),
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
                scoped_channels: vec![ChannelRef {
                    group: 0,
                    channel: 2,
                }],
                scope_toggles: SceneScopeToggles::default(),
            }],
        }
    }

    fn show_file_json(scope_toggles: Option<serde_json::Value>) -> serde_json::Value {
        let mut scene_config = serde_json::json!({
            "internalSceneId": "11111111-1111-4111-8111-111111111111",
            "sceneIndex": 1,
            "sceneName": "Intro",
            "durationMs": 0,
            "channelConfigs": [],
            "scopedChannels": []
        });
        if let Some(scope_toggles) = scope_toggles {
            scene_config["scopeToggles"] = scope_toggles;
        }
        serde_json::json!({
            "schemaVersion": 1,
            "appVersion": "0.1.0",
            "savedAt": "2026-06-09T00:00:00Z",
            "safety": { "lockout": false },
            "sceneConfigs": [scene_config]
        })
    }

    #[test]
    fn default_show_folder_uses_home_documents_when_document_dir_is_missing() {
        let home = PathBuf::from("/home/engineer");

        assert_eq!(
            default_show_folder_from(None, Some(home.clone())),
            home.join("Documents").join("Advanced Show Control")
        );
    }

    #[test]
    fn app_data_folder_name_matches_platform_expectation() {
        #[cfg(target_os = "linux")]
        assert_eq!(app_data_folder_name(), "advanced-show-control");

        #[cfg(not(target_os = "linux"))]
        assert_eq!(app_data_folder_name(), "Advanced Show Control");
    }

    #[test]
    fn show_file_serializes_camel_case_json() {
        let json = serde_json::to_value(show_file()).unwrap();

        assert_eq!(json["schemaVersion"], SHOW_FILE_SCHEMA_VERSION);
        assert_eq!(json["sceneConfigs"][0]["durationMs"], 4000);
        assert_eq!(
            json["sceneConfigs"][0]["channelConfigs"][0]["faderDb"],
            -12.5
        );
        assert_eq!(json["sceneConfigs"][0]["scopeToggles"]["faders"], false);
    }

    #[test]
    fn omitted_show_file_scope_defaults_to_empty() {
        let file: ShowFile = serde_json::from_value(show_file_json(None)).unwrap();

        assert!(!file.scene_configs[0].scope_toggles.faders);
        assert!(!file.scene_configs[0].scope_toggles.pan);
    }

    #[test]
    fn partial_show_file_scope_toggles_default_missing_fields_to_false() {
        let faders_missing: ShowFile =
            serde_json::from_value(show_file_json(Some(serde_json::json!({ "pan": true }))))
                .unwrap();
        let pan_missing: ShowFile =
            serde_json::from_value(show_file_json(Some(serde_json::json!({ "faders": true }))))
                .unwrap();

        assert!(!faders_missing.scene_configs[0].scope_toggles.faders);
        assert!(faders_missing.scene_configs[0].scope_toggles.pan);
        assert!(pan_missing.scene_configs[0].scope_toggles.faders);
        assert!(!pan_missing.scene_configs[0].scope_toggles.pan);
    }

    #[test]
    fn show_file_scope_toggles_preserve_explicit_values() {
        let enabled: ShowFile = serde_json::from_value(show_file_json(Some(serde_json::json!({
            "faders": true,
            "pan": true
        }))))
        .unwrap();
        let disabled: ShowFile = serde_json::from_value(show_file_json(Some(serde_json::json!({
            "faders": false,
            "pan": false
        }))))
        .unwrap();

        assert!(enabled.scene_configs[0].scope_toggles.faders);
        assert!(enabled.scene_configs[0].scope_toggles.pan);
        assert!(!disabled.scene_configs[0].scope_toggles.faders);
        assert!(!disabled.scene_configs[0].scope_toggles.pan);
    }

    #[test]
    fn backup_names_match_only_the_exact_show_stem() {
        assert!(is_backup_for_show_file("100-mix.ascs", "mix"));
        assert!(is_backup_for_show_file("100-mix__backup1.ascs", "mix"));
        assert!(!is_backup_for_show_file("100-mix-1.ascs", "mix"));
        assert!(is_backup_for_show_file("100-mix-1.ascs", "mix-1"));
        assert!(!is_backup_for_show_file(".100-mix.ascs.tmp", "mix"));
    }

    #[test]
    fn prune_backup_entries_uses_age_not_filename_order() {
        let backups = vec![
            (
                UNIX_EPOCH + Duration::from_secs(2),
                "10-foo.ascs".to_string(),
                PathBuf::from("10-foo.ascs"),
            ),
            (
                UNIX_EPOCH + Duration::from_secs(1),
                "2-foo.ascs".to_string(),
                PathBuf::from("2-foo.ascs"),
            ),
            (
                UNIX_EPOCH + Duration::from_secs(3),
                "11-foo.ascs".to_string(),
                PathBuf::from("11-foo.ascs"),
            ),
        ];

        assert_eq!(
            prune_backup_entries(backups, 2),
            vec![PathBuf::from("2-foo.ascs")]
        );
    }
}
