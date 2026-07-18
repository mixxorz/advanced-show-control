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
        write_settings_file(&self.file_path, &updated)?;
        self.document = updated;
        Ok(true)
    }

    pub fn last_connected_lv1(&self) -> Option<Lv1SystemIdentity> {
        self.document.last_connected_lv1.clone()
    }

    pub fn set_last_connected_lv1(&mut self, identity: Lv1SystemIdentity) -> Result<bool, String> {
        if self.document.last_connected_lv1.as_ref() == Some(&identity) {
            return Ok(false);
        }
        let mut updated = self.document.clone();
        updated.last_connected_lv1 = Some(identity);
        write_settings_file(&self.file_path, &updated)?;
        self.document = updated;
        Ok(true)
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

fn write_settings_file(file_path: &Path, settings: &PersistedSettings) -> Result<(), String> {
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create settings directory: {err}"))?;
    }
    let contents = serde_json::to_string_pretty(settings)
        .map_err(|err| format!("Failed to serialize settings: {err}"))?;
    std::fs::write(file_path, contents).map_err(|err| format!("Failed to write settings: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection_state::Lv1SystemIdentity;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_settings_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "asc-settings-state-{name}-{}-{unique}",
            std::process::id()
        ))
    }

    fn identity(uuid: &str, host: &str, address: &str) -> Lv1SystemIdentity {
        Lv1SystemIdentity {
            uuid: Some(uuid.to_string()),
            host: Some(host.to_string()),
            address: address.to_string(),
            port: 50000,
        }
    }

    #[test]
    fn invalid_persisted_document_resets_public_and_private_settings() {
        let dir = temp_settings_dir("invalid-document");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), r#"{"lastConnectedLv1":42}"#).unwrap();

        let state = SettingsState::load(dir);

        assert_eq!(state.settings(), AppSettings::default());
        assert_eq!(state.last_connected_lv1(), None);
    }

    #[test]
    fn replacing_public_settings_preserves_remembered_identity() {
        let dir = temp_settings_dir("preserve-identity");
        let identity = identity("uuid-1", "LV1-FOH", "192.168.1.35");
        let mut state = SettingsState::load(dir.clone());
        state.set_last_connected_lv1(identity.clone()).unwrap();

        state
            .replace_settings(AppSettings {
                auto_save_sessions: true,
                ..Default::default()
            })
            .unwrap();

        let reloaded = SettingsState::load(dir);
        assert!(reloaded.settings().auto_save_sessions);
        assert_eq!(reloaded.last_connected_lv1(), Some(identity));
    }

    #[test]
    fn remembered_identity_uses_the_existing_flat_private_schema() {
        let dir = temp_settings_dir("flat-private-schema");
        let identity = identity("uuid-1", "LV1-FOH", "192.168.1.35");
        let mut state = SettingsState::load(dir.clone());
        state
            .set_last_connected_lv1(identity)
            .expect("remembered identity should save");

        let document: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("settings.json"))
                .expect("settings document should exist"),
        )
        .expect("settings document should be JSON");
        assert_eq!(document["lastConnectedLv1"]["uuid"], "uuid-1");
        assert_eq!(document["lastConnectedLv1"]["host"], "LV1-FOH");
        assert!(document.get("settings").is_none());
    }
}
