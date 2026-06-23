pub use crate::show::{
    ImportedShowFile, LoadValidationReport, SHOW_FILE_SCHEMA_VERSION, ShowFile,
    ShowFileChannelConfig, ShowFileChannelRef, ShowFileSafety, ShowFileSceneConfig,
    ShowFileSceneScopeToggles, export_show_file, import_show_file, prune_show_file_to_lv1_scenes,
};

use std::fs;
use std::fs::OpenOptions;
use std::io::{self, Write};
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

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "Failed to create parent directory {}: {err}",
                parent.display()
            )
        })?;
    }

    let json = serde_json::to_string_pretty(file)
        .map_err(|err| format!("Failed to serialize session {}: {err}", path.display()))?;

    let parent = path
        .parent()
        .ok_or_else(|| format!("Session path has no parent: {}", path.display()))?;
    let timestamp = crate::time::current_timestamp_millis();
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("show");
    let (temp_path, mut temp_file) = reserve_unique_temp_file(parent, file_name, &timestamp)?;

    let write_result = (|| -> Result<(), String> {
        temp_file
            .write_all(json.as_bytes())
            .and_then(|_| temp_file.sync_all())
            .map_err(|err| {
                format!(
                    "Failed to write temp session {}: {err}",
                    temp_path.display()
                )
            })?;
        drop(temp_file);
        fs::rename(&temp_path, path).map_err(|err| {
            format!(
                "Failed to replace session {} from {}: {err}",
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
    let (candidate, mut dest) = reserve_unique_backup_file(backup_dir, path, &timestamp)?;
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

    prune_old_backups(backup_dir, path, MAX_BACKUPS_PER_SHOW_FILE)?;

    Ok(())
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
            format!("{timestamp}-{stem}.ascs")
        } else {
            format!("{timestamp}-{stem}__backup{suffix}.ascs")
        }
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::{ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry};
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
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                },
                ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Lead".to_string(),
                    gain_db: -8.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
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
            cued_scene_internal_id: None,
            scene_configs: vec![ShowFileSceneConfig {
                internal_scene_id: Some(uuid::Uuid::from_u128(0x11111111111141118111111111111111)),
                scene_index: Some(1),
                scene_name: "Intro".to_string(),
                duration_ms: 4000,
                channel_configs: vec![ShowFileChannelConfig {
                    group: 0,
                    channel: 2,
                    fader_db: Some(-12.5),
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
                scoped_channels: vec![ShowFileChannelRef {
                    group: 0,
                    channel: 2,
                }],
                scope_toggles: ShowFileSceneScopeToggles::default(),
            }],
        }
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "advanced-show-control-{}-{}-{}",
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

        assert_eq!(folder, home.join("Documents").join("Advanced Show Control"));

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn app_data_folder_name_matches_platform_expectation() {
        #[cfg(target_os = "linux")]
        assert_eq!(app_data_folder_name(), "advanced-show-control");

        #[cfg(not(target_os = "linux"))]
        assert_eq!(app_data_folder_name(), "Advanced Show Control");
    }

    #[test]
    fn save_show_file_writes_json_and_creates_backup_on_overwrite() {
        let temp_dir = temp_test_dir("write");
        let show_path = temp_dir.join("test.ascs");
        let backup_dir = temp_dir.join("backups");
        let file = show_file();

        write_show_file(&show_path, &file, &backup_dir).unwrap();
        write_show_file(&show_path, &file, &backup_dir).unwrap();

        let json = fs::read_to_string(&show_path).unwrap();
        assert!(json.contains("\"sceneConfigs\""));

        let backups = fs::read_dir(&backup_dir).unwrap().count();
        assert_eq!(backups, 1);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn reserve_unique_backup_file_adds_suffix_when_candidate_exists() {
        let backup_dir = temp_test_dir("backup-path");
        let candidate = backup_dir.join("123-test.ascs");
        fs::write(&candidate, "taken").unwrap();

        let (path, _file) =
            reserve_unique_backup_file(&backup_dir, Path::new("test.ascs"), "123").unwrap();

        assert_eq!(
            path.file_name().and_then(|value| value.to_str()),
            Some("123-test__backup1.ascs")
        );

        let _ = fs::remove_dir_all(&backup_dir);
    }

    #[test]
    fn reserve_unique_temp_file_adds_suffix_when_candidate_exists() {
        let temp_dir = temp_test_dir("temp-path");
        let candidate = temp_dir.join(".test.ascs.tmp-123");
        fs::write(&candidate, "taken").unwrap();

        let (path, _file) = reserve_unique_temp_file(&temp_dir, "test.ascs", "123").unwrap();

        assert_eq!(
            path.file_name().and_then(|value| value.to_str()),
            Some(".test.ascs.tmp-123-1")
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn show_file_deserializes_missing_cued_scene_id_as_none() {
        let json = r#"{
            "schemaVersion": 1,
            "appVersion": "0.1.0",
            "savedAt": "2026-06-18T00:00:00Z",
            "safety": { "lockout": false },
            "sceneConfigs": []
        }"#;

        let file: ShowFile = serde_json::from_str(json).unwrap();

        assert_eq!(file.cued_scene_internal_id, None);
    }

    #[test]
    fn read_show_file_parses_json() {
        let temp_dir = temp_test_dir("read");
        let show_path = temp_dir.join("test.ascs");
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
        assert!(json.contains("\"sceneConfigs\""));
        assert!(json.contains("\"durationMs\": 4000"));
        assert!(json.contains("\"channelConfigs\""));
        assert!(json.contains("\"scopedChannels\""));
        assert!(json.contains("\"scopeToggles\""));
        assert!(json.contains("\"faders\": true"));
        assert!(json.contains("\"faderDb\": -12.5"));
    }

    #[test]
    fn old_show_file_scene_configs_default_fader_scope_enabled() {
        let json = r#"
        {
          "schemaVersion": 1,
          "appVersion": "0.1.0",
          "savedAt": "2026-06-09T00:00:00Z",
          "safety": { "lockout": false },
          "sceneConfigs": [{
            "internalSceneId": "11111111-1111-4111-8111-111111111111",
            "sceneIndex": 1,
            "sceneName": "Intro",
            "durationMs": 0,
            "channelConfigs": [],
            "scopedChannels": []
          }]
        }
        "#;

        let file: ShowFile = serde_json::from_str(json).unwrap();

        assert!(file.scene_configs[0].scope_toggles.faders);
    }

    #[test]
    fn partial_show_file_scope_toggles_default_fader_scope_enabled() {
        let json = r#"
        {
          "schemaVersion": 1,
          "appVersion": "0.1.0",
          "savedAt": "2026-06-09T00:00:00Z",
          "safety": { "lockout": false },
          "sceneConfigs": [{
            "internalSceneId": "11111111-1111-4111-8111-111111111111",
            "sceneIndex": 1,
            "sceneName": "Intro",
            "durationMs": 0,
            "channelConfigs": [],
            "scopedChannels": [],
            "scopeToggles": { "pan": true }
          }]
        }
        "#;

        let file: ShowFile = serde_json::from_str(json).unwrap();

        assert!(file.scene_configs[0].scope_toggles.faders);
        assert!(file.scene_configs[0].scope_toggles.pan);
    }

    #[test]
    fn pruning_keeps_exact_scene_matches() {
        let report = prune_show_file_to_lv1_scenes(&mut show_file(), &lv1_snapshot()).unwrap();

        assert_eq!(report.removed_scenes.len(), 0);
    }

    #[test]
    fn pruning_deletes_scene_when_name_differs() {
        let mut file = show_file();
        file.scene_configs[0].scene_name = "Renamed Intro".to_string();

        let report = prune_show_file_to_lv1_scenes(&mut file, &lv1_snapshot()).unwrap();

        assert!(file.scene_configs.is_empty());
        assert_eq!(report.removed_scenes, vec!["1: Renamed Intro".to_string()]);
    }

    #[test]
    fn pruning_does_not_remove_channel_configs_or_scope() {
        let mut file = show_file();
        file.scene_configs[0].channel_configs[0].group = 99;
        file.scene_configs[0].scoped_channels[0].group = 99;

        let report = prune_show_file_to_lv1_scenes(&mut file, &lv1_snapshot()).unwrap();

        assert!(!report.removed_anything());
        assert_eq!(file.scene_configs[0].channel_configs.len(), 1);
        assert_eq!(file.scene_configs[0].scoped_channels.len(), 1);
    }

    #[test]
    fn pruning_requires_scene_list_but_allows_empty_channels() {
        let mut file = show_file();
        let mut snapshot = lv1_snapshot();
        snapshot.channels.clear();

        let report = prune_show_file_to_lv1_scenes(&mut file, &snapshot).unwrap();

        assert_eq!(report.removed_scenes.len(), 0);
    }

    #[test]
    fn pruning_still_requires_scene_list() {
        let mut file = show_file();
        let mut snapshot = lv1_snapshot();
        snapshot.scene_list.clear();

        assert_eq!(
            prune_show_file_to_lv1_scenes(&mut file, &snapshot).unwrap_err(),
            "Open a session after LV1 scenes are loaded"
        );
    }

    #[test]
    fn create_backup_prunes_old_backups_for_same_show_file() {
        let backup_dir = temp_test_dir("backup-prune");
        let source = backup_dir.join("show.ascs");
        fs::write(&source, "current").unwrap();

        for index in 0..11 {
            fs::write(
                backup_dir.join(format!("100{index}-show.ascs")),
                format!("old-{index}"),
            )
            .unwrap();
        }
        fs::write(backup_dir.join("1000-other.ascs"), "keep").unwrap();

        create_backup(&source, &backup_dir).unwrap();

        let mut entries: Vec<_> = fs::read_dir(&backup_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        entries.sort();

        let show_backups: Vec<_> = entries
            .iter()
            .filter(|name| {
                name.strip_suffix(".ascs")
                    .and_then(|prefix| prefix.split_once('-'))
                    .is_some_and(|(_, source)| source == "show")
            })
            .collect();

        assert!(entries.iter().any(|name| name == "1000-other.ascs"));
        assert_eq!(show_backups.len(), 10);

        let _ = fs::remove_dir_all(&backup_dir);
    }

    #[test]
    fn prune_old_backups_does_not_match_hyphenated_neighbor_show_files() {
        let backup_dir = temp_test_dir("backup-boundary");
        let source = backup_dir.join("foo.ascs");

        fs::write(backup_dir.join("100-foo.ascs"), "foo-old").unwrap();
        fs::write(backup_dir.join("101-foo-bar.ascs"), "foo-bar-old").unwrap();
        fs::write(&source, "current").unwrap();

        prune_old_backups(&backup_dir, &source, 0).unwrap();

        let entries: Vec<_> = fs::read_dir(&backup_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();

        assert!(entries.contains(&"101-foo-bar.ascs".to_string()));

        let _ = fs::remove_dir_all(&backup_dir);
    }

    #[test]
    fn prune_backup_entries_uses_age_not_lexicographic_filename_order() {
        use std::time::{Duration, UNIX_EPOCH};

        let older = UNIX_EPOCH + Duration::from_secs(1);
        let middle = UNIX_EPOCH + Duration::from_secs(2);
        let newer = UNIX_EPOCH + Duration::from_secs(3);

        let backups = vec![
            (
                middle,
                "10-foo.ascs".to_string(),
                PathBuf::from("10-foo.ascs"),
            ),
            (older, "2-foo.ascs".to_string(), PathBuf::from("2-foo.ascs")),
            (
                newer,
                "11-foo.ascs".to_string(),
                PathBuf::from("11-foo.ascs"),
            ),
        ];

        let pruned = prune_backup_entries(backups, 2);

        assert_eq!(pruned, vec![PathBuf::from("2-foo.ascs")]);
    }

    #[test]
    fn prune_backup_entries_keeps_mix_dash_digit_backups_separate_from_mix_backups() {
        use std::time::{Duration, UNIX_EPOCH};

        let older = UNIX_EPOCH + Duration::from_secs(1);
        let newer = UNIX_EPOCH + Duration::from_secs(2);

        let backup_dir = temp_test_dir("backup-mix-boundary");
        let exact = backup_dir.join("100-mix.ascs");
        let hyphenated = backup_dir.join("101-mix-1.ascs");
        fs::write(&exact, "mix").unwrap();
        fs::write(&hyphenated, "mix-1").unwrap();

        let exact_entries = vec![
            (older, "100-mix.ascs".to_string(), exact.clone()),
            (newer, "101-mix-1.ascs".to_string(), hyphenated.clone()),
        ];

        assert_eq!(
            prune_backup_entries(exact_entries, 0),
            vec![exact.clone(), hyphenated.clone()]
        );
        assert!(is_backup_for_show_file("100-mix.ascs", "mix"));
        assert!(!is_backup_for_show_file("101-mix-1.ascs", "mix"));
        assert!(is_backup_for_show_file("101-mix-1.ascs", "mix-1"));

        let _ = fs::remove_dir_all(&backup_dir);
    }

    #[test]
    fn create_backup_keeps_unrelated_backups() {
        let backup_dir = temp_test_dir("backup-unrelated");
        let source = backup_dir.join("setlist.ascs");
        fs::write(&source, "current").unwrap();

        for index in 0..2 {
            fs::write(
                backup_dir.join(format!("100{index}-setlist.ascs")),
                format!("old-{index}"),
            )
            .unwrap();
        }
        fs::write(backup_dir.join("1000-other.ascs"), "keep").unwrap();

        create_backup(&source, &backup_dir).unwrap();

        let entries: Vec<_> = fs::read_dir(&backup_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();

        assert!(entries.iter().any(|name| name == "1000-other.ascs"));

        let _ = fs::remove_dir_all(&backup_dir);
    }
}
