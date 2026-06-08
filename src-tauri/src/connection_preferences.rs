use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastConnectedLv1 {
    pub uuid: Option<String>,
    pub host: Option<String>,
    pub address: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionPreferences {
    pub last_connected_lv1: Option<LastConnectedLv1>,
}

pub fn read_connection_preferences(path: &Path) -> Result<ConnectionPreferences, String> {
    match std::fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|err| format!("Failed to parse connection preferences: {err}")),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Ok(ConnectionPreferences::default())
        }
        Err(err) => Err(format!("Failed to read connection preferences: {err}")),
    }
}

pub fn write_connection_preferences(
    path: &Path,
    preferences: &ConnectionPreferences,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create preferences folder: {err}"))?;
    }
    let contents = serde_json::to_string_pretty(preferences)
        .map_err(|err| format!("Failed to serialize connection preferences: {err}"))?;
    std::fs::write(path, contents)
        .map_err(|err| format!("Failed to write connection preferences: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_preferences_path(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "lv1-scene-fade-utility-preferences-{name}-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn missing_preferences_file_loads_defaults() {
        let path = temp_preferences_path("missing");

        let preferences = read_connection_preferences(&path).unwrap();

        assert_eq!(preferences, ConnectionPreferences::default());
    }

    #[test]
    fn preferences_round_trip_last_connected_lv1() {
        let path = temp_preferences_path("round-trip");
        let preferences = ConnectionPreferences {
            last_connected_lv1: Some(LastConnectedLv1 {
                uuid: Some("uuid-1".to_string()),
                host: Some("LV1-FOH".to_string()),
                address: "192.168.1.35".to_string(),
                port: 50000,
            }),
        };

        write_connection_preferences(&path, &preferences).unwrap();
        let loaded = read_connection_preferences(&path).unwrap();

        assert_eq!(loaded, preferences);
        let _ = std::fs::remove_file(path);
    }
}
