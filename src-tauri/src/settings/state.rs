use std::path::{Path, PathBuf};

use super::AppSettings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsState {
    settings: AppSettings,
    file_path: PathBuf,
}

impl SettingsState {
    pub fn load(settings_dir: PathBuf) -> Self {
        let file_path = settings_dir.join("settings.json");
        let settings = load_settings_file(&file_path);
        Self {
            settings,
            file_path,
        }
    }

    pub fn settings(&self) -> AppSettings {
        self.settings.clone()
    }

    pub fn replace_settings(&mut self, settings: AppSettings) -> Result<bool, String> {
        let normalized = settings.normalized();
        if normalized == self.settings {
            return Ok(false);
        }
        write_settings_file(&self.file_path, &normalized)?;
        self.settings = normalized;
        Ok(true)
    }
}

fn load_settings_file(file_path: &Path) -> AppSettings {
    match std::fs::read_to_string(file_path) {
        Ok(contents) => match serde_json::from_str::<AppSettings>(&contents) {
            Ok(settings) => {
                tracing::info!(
                    event = "settings_loaded",
                    path = %file_path.display(),
                    "Settings loaded"
                );
                settings.normalized()
            }
            Err(err) => {
                tracing::warn!(
                    event = "settings_file_invalid",
                    path = %file_path.display(),
                    error = %err,
                    "Settings file could not be read; using defaults"
                );
                AppSettings::default().normalized()
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            tracing::info!(
                event = "settings_defaults_loaded",
                path = %file_path.display(),
                "Settings file not found; using defaults"
            );
            AppSettings::default().normalized()
        }
        Err(err) => {
            tracing::warn!(
                event = "settings_file_unavailable",
                path = %file_path.display(),
                error = %err,
                "Settings file could not be opened; using defaults"
            );
            AppSettings::default().normalized()
        }
    }
}

fn write_settings_file(file_path: &Path, settings: &AppSettings) -> Result<(), String> {
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            tracing::error!(
                event = "settings_write_failed",
                path = %file_path.display(),
                error = %err,
                "Settings could not be saved"
            );
            format!("Failed to create settings directory: {err}")
        })?;
    }
    let contents = serde_json::to_string_pretty(settings).map_err(|err| {
        tracing::error!(
            event = "settings_write_failed",
            path = %file_path.display(),
            error = %err,
            "Settings could not be saved"
        );
        format!("Failed to serialize settings: {err}")
    })?;
    std::fs::write(file_path, contents).map_err(|err| {
        tracing::error!(
            event = "settings_write_failed",
            path = %file_path.display(),
            error = %err,
            "Settings could not be saved"
        );
        format!("Failed to write settings: {err}")
    })
}
