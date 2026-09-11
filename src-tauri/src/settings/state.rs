use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::connection_state::Lv1SystemIdentity;
use serde::{Deserialize, Serialize};

use super::AppSettings;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PersistedSettings {
    #[serde(flatten)]
    settings: AppSettings,
    last_connected_lv1: Option<Lv1SystemIdentity>,
}

impl PersistedSettings {
    fn normalized(mut self) -> Self {
        self.settings = self.settings.normalized();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsState {
    document: PersistedSettings,
    file_path: PathBuf,
}

impl SettingsState {
    pub fn load(settings_dir: PathBuf) -> Self {
        let file_path = settings_dir.join("settings.json");
        let document = load_settings_file(&file_path);
        Self {
            document,
            file_path,
        }
    }

    pub fn settings(&self) -> AppSettings {
        self.document.settings.clone()
    }

    pub fn replace_settings(&mut self, settings: AppSettings) -> Result<bool, String> {
        let normalized = settings.normalized();
        if normalized == self.document.settings {
            return Ok(false);
        }
        let mut updated = self.document.clone();
        updated.settings = normalized;
        let staged = StagedSettingsUpdate::prepare(self.file_path.clone(), updated)?;
        self.publish_staged(staged)?;
        Ok(true)
    }

    pub fn last_connected_lv1(&self) -> Option<Lv1SystemIdentity> {
        self.document.last_connected_lv1.clone()
    }

    pub(crate) fn stage_last_connected_lv1(
        &self,
        identity: Lv1SystemIdentity,
    ) -> Result<Option<StagedSettingsUpdate>, String> {
        if self.document.last_connected_lv1.as_ref() == Some(&identity) {
            return Ok(None);
        }

        let mut updated = self.document.clone();
        updated.last_connected_lv1 = Some(identity);
        StagedSettingsUpdate::prepare(self.file_path.clone(), updated).map(Some)
    }

    pub(crate) fn publish_staged(
        &mut self,
        mut staged: StagedSettingsUpdate,
    ) -> Result<(), String> {
        crate::atomic_file::replace(&staged.staged_path, &self.file_path).map_err(|err| {
            format!(
                "Failed to publish settings {} from {}: {err}",
                self.file_path.display(),
                staged.staged_path.display()
            )
        })?;
        self.document = staged
            .document
            .take()
            .expect("staged settings document should be available until publication");
        Ok(())
    }
}

pub(crate) struct StagedSettingsUpdate {
    document: Option<PersistedSettings>,
    staged_path: PathBuf,
}

impl StagedSettingsUpdate {
    fn prepare(file_path: PathBuf, document: PersistedSettings) -> Result<Self, String> {
        let contents = serde_json::to_string_pretty(&document)
            .map_err(|err| format!("Failed to serialize settings: {err}"))?;
        let parent = file_path
            .parent()
            .ok_or_else(|| format!("Settings path has no parent: {}", file_path.display()))?;
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create settings directory: {err}"))?;
        let (staged_path, mut staged_file) = reserve_staged_settings_file(parent)?;

        let write_result = staged_file
            .write_all(contents.as_bytes())
            .and_then(|_| staged_file.sync_all())
            .map_err(|err| format!("Failed to write staged settings: {err}"));
        if let Err(error) = write_result {
            drop(staged_file);
            let _ = fs::remove_file(&staged_path);
            return Err(error);
        }

        Ok(Self {
            document: Some(document),
            staged_path,
        })
    }
}

impl Drop for StagedSettingsUpdate {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.staged_path);
    }
}

fn reserve_staged_settings_file(parent: &Path) -> Result<(PathBuf, fs::File), String> {
    let timestamp = crate::time::current_timestamp_millis();
    for suffix in 0.. {
        let name = if suffix == 0 {
            format!(".settings.json.tmp-{timestamp}")
        } else {
            format!(".settings.json.tmp-{timestamp}-{suffix}")
        };
        let staged_path = parent.join(name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged_path)
        {
            Ok(file) => return Ok((staged_path, file)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(format!("Failed to reserve staged settings file: {err}")),
        }
    }

    unreachable!("suffix loop is unbounded")
}

fn load_settings_file(file_path: &Path) -> PersistedSettings {
    match std::fs::read_to_string(file_path) {
        Ok(contents) => match serde_json::from_str::<PersistedSettings>(&contents) {
            Ok(document) => {
                tracing::info!(
                    event = "settings_loaded",
                    path = %file_path.display(),
                    "Settings loaded"
                );
                document.normalized()
            }
            Err(err) => {
                tracing::warn!(
                    event = "settings_file_invalid",
                    path = %file_path.display(),
                    error = %err,
                    "Settings file could not be read; using defaults"
                );
                PersistedSettings::default().normalized()
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            tracing::info!(
                event = "settings_defaults_loaded",
                path = %file_path.display(),
                "Settings file not found; using defaults"
            );
            PersistedSettings::default().normalized()
        }
        Err(err) => {
            tracing::warn!(
                event = "settings_file_unavailable",
                path = %file_path.display(),
                error = %err,
                "Settings file could not be opened; using defaults"
            );
            PersistedSettings::default().normalized()
        }
    }
}
