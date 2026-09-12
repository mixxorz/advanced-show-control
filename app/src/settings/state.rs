use std::path::{Path, PathBuf};

use crate::atomic_file::StagedFile;
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
    /// @cc [owner:mixxorz,label:resilience] settings-load-fallback
    /// Loading MUST use normalized persisted values when the complete document deserializes; a
    /// missing, unreadable, or invalid `settings.json` MUST instead produce normalized defaults,
    /// including no remembered LV1 identity, without rewriting the file.
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

    /// @cc [owner:mixxorz,label:data-integrity] public-settings-replacement-atomicity
    /// A changed public-settings replacement MUST preserve the remembered LV1 identity and update
    /// in-memory state only after the complete normalized document is atomically published. A
    /// staging or publication failure MUST return an error and leave both prior memory and the
    /// destination document unchanged; normalized no-ops MUST perform no write and return `false`.
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

    /// @cc [owner:mixxorz,label:data-integrity] stage-private-identity-update
    /// Staging a changed remembered identity MUST preserve all public settings and MUST NOT mutate
    /// memory or publish the destination; an identical identity MUST return no staged update.
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

    /// @cc [owner:mixxorz,label:data-integrity] publish-settings-after-file
    /// Publication MUST replace `settings.json` before committing the staged document to memory;
    /// replacement failure MUST return an explanatory error and retain the previous memory state.
    pub(crate) fn publish_staged(&mut self, staged: StagedSettingsUpdate) -> Result<(), String> {
        staged.file.publish().map_err(|err| {
            format!(
                "Failed to publish settings {}: {err}",
                self.file_path.display()
            )
        })?;
        self.document = staged.document;
        Ok(())
    }
}

pub(crate) struct StagedSettingsUpdate {
    document: PersistedSettings,
    file: StagedFile,
}

impl StagedSettingsUpdate {
    fn prepare(file_path: PathBuf, document: PersistedSettings) -> Result<Self, String> {
        let contents = serde_json::to_string_pretty(&document)
            .map_err(|err| format!("Failed to serialize settings: {err}"))?;
        let file = StagedFile::prepare(&file_path, contents.as_bytes())
            .map_err(|err| format!("Failed to stage settings {}: {err}", file_path.display()))?;
        Ok(Self { document, file })
    }
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
