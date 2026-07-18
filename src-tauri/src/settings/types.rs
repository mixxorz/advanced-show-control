#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_use_agreed_values() {
        let settings = AppSettings::default();

        assert!(!settings.auto_load_last_show_file);
        assert!(!settings.auto_save_sessions);
        assert_eq!(settings.keyboard_shortcuts.go.key, "Space");
        assert_eq!(settings.keyboard_shortcuts.cue.key, "C");
        assert_eq!(settings.time_display, TimeDisplayFormat::TwentyFourHour);
        assert_eq!(settings.fader_override_sensitivity, 9);
        assert!(!settings.enable_extensive_diagnostics);
    }

    #[test]
    fn default_settings_enable_same_scene_finishing_with_500ms_threshold() {
        let settings = AppSettings::default();

        assert!(settings.same_scene_recall_enabled);
        assert_eq!(settings.same_scene_recall_threshold_ms, 500);
    }

    #[test]
    fn partial_settings_use_same_scene_defaults() {
        let settings: AppSettings = serde_json::from_str(r#"{"autoSaveSessions":true}"#)
            .expect("partial settings should deserialize");

        assert!(settings.same_scene_recall_enabled);
        assert_eq!(settings.same_scene_recall_threshold_ms, 500);
    }

    #[test]
    fn normalization_clamps_same_scene_threshold() {
        let settings = AppSettings {
            same_scene_recall_threshold_ms: 9_999,
            ..Default::default()
        }
        .normalized();

        assert_eq!(settings.same_scene_recall_threshold_ms, 5_000);
    }

    #[test]
    fn normalization_clamps_sensitivity_and_trims_shortcuts() {
        let settings = AppSettings {
            fader_override_sensitivity: 99,
            keyboard_shortcuts: KeyboardShortcutSettings {
                go: KeyboardShortcut {
                    key: "  Enter  ".to_string(),
                    modifiers: KeyboardShortcutModifiers {
                        shift: true,
                        ..Default::default()
                    },
                },
                cue: KeyboardShortcut {
                    key: "   ".to_string(),
                    modifiers: KeyboardShortcutModifiers::default(),
                },
            },
            ..Default::default()
        }
        .normalized();

        assert_eq!(settings.fader_override_sensitivity, 10);
        assert_eq!(settings.keyboard_shortcuts.go.key, "Enter");
        assert_eq!(settings.keyboard_shortcuts.cue.key, "C");
    }

    #[test]
    fn normalization_clamps_sensitivity_to_minimum() {
        let settings = AppSettings {
            fader_override_sensitivity: 0,
            ..Default::default()
        }
        .normalized();

        assert_eq!(settings.fader_override_sensitivity, 1);
    }

    #[test]
    fn normalization_canonicalizes_shortcut_key_labels() {
        let named_keys = [
            ("space", "Space"),
            ("enter", "Enter"),
            ("escape", "Escape"),
            ("tab", "Tab"),
            ("backspace", "Backspace"),
            ("delete", "Delete"),
            ("home", "Home"),
            ("end", "End"),
            ("pageup", "PageUp"),
            ("pagedown", "PageDown"),
            ("arrowup", "ArrowUp"),
            ("arrowdown", "ArrowDown"),
            ("arrowleft", "ArrowLeft"),
            ("arrowright", "ArrowRight"),
        ];

        assert_eq!(normalize_key(" c "), Some("C".to_string()));
        for (input, expected) in named_keys {
            assert_eq!(
                normalize_key(&input.to_uppercase()),
                Some(expected.to_string())
            );
        }
        assert_eq!(
            normalize_key("  CustomKey  "),
            Some("CustomKey".to_string())
        );
        assert_eq!(normalize_key("   "), None);
    }

    #[test]
    fn normalization_uppercases_single_unicode_scalar_keys() {
        assert_eq!(normalize_key("é"), Some("É".to_string()));
        assert_eq!(normalize_key("ß"), Some("SS".to_string()));
    }

    #[test]
    fn partial_shortcut_settings_deserialize_with_agreed_defaults() {
        let settings: AppSettings =
            serde_json::from_str(r#"{"keyboardShortcuts":{"cue":{"key":"C"}}}"#)
                .expect("settings should deserialize");

        assert_eq!(settings.keyboard_shortcuts.go.key, "Space");
        assert_eq!(settings.keyboard_shortcuts.cue.key, "C");
        assert!(!settings.enable_extensive_diagnostics);
    }
}

use crate::connection_state::Lv1SystemIdentity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct AppSettings {
    pub auto_load_last_show_file: bool,
    pub auto_save_sessions: bool,
    pub keyboard_shortcuts: KeyboardShortcutSettings,
    pub time_display: TimeDisplayFormat,
    pub fader_override_sensitivity: u8,
    pub enable_extensive_diagnostics: bool,
    pub same_scene_recall_enabled: bool,
    pub same_scene_recall_threshold_ms: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            auto_load_last_show_file: false,
            auto_save_sessions: false,
            keyboard_shortcuts: KeyboardShortcutSettings::default(),
            time_display: TimeDisplayFormat::TwentyFourHour,
            fader_override_sensitivity: 9,
            enable_extensive_diagnostics: false,
            same_scene_recall_enabled: true,
            same_scene_recall_threshold_ms: 500,
        }
    }
}

impl AppSettings {
    pub fn normalized(mut self) -> Self {
        self.fader_override_sensitivity = self.fader_override_sensitivity.clamp(1, 10);
        self.same_scene_recall_threshold_ms = self.same_scene_recall_threshold_ms.clamp(0, 5_000);
        self.keyboard_shortcuts = self.keyboard_shortcuts.normalized();
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PersistedSettings {
    #[serde(flatten)]
    pub settings: AppSettings,
    pub last_connected_lv1: Option<Lv1SystemIdentity>,
}

impl PersistedSettings {
    pub fn normalized(mut self) -> Self {
        self.settings = self.settings.normalized();
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct KeyboardShortcutSettings {
    #[serde(default = "KeyboardShortcut::go_default")]
    pub go: KeyboardShortcut,
    #[serde(default = "KeyboardShortcut::cue_default")]
    pub cue: KeyboardShortcut,
}

impl Default for KeyboardShortcutSettings {
    fn default() -> Self {
        Self {
            go: KeyboardShortcut::go_default(),
            cue: KeyboardShortcut::cue_default(),
        }
    }
}

impl KeyboardShortcutSettings {
    fn normalized(self) -> Self {
        Self {
            go: self.go.normalized_or(KeyboardShortcut::go_default()),
            cue: self.cue.normalized_or(KeyboardShortcut::cue_default()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct KeyboardShortcut {
    pub key: String,
    pub modifiers: KeyboardShortcutModifiers,
}

impl Default for KeyboardShortcut {
    fn default() -> Self {
        Self::go_default()
    }
}

impl KeyboardShortcut {
    fn go_default() -> Self {
        Self {
            key: "Space".to_string(),
            modifiers: KeyboardShortcutModifiers::default(),
        }
    }

    fn cue_default() -> Self {
        Self {
            key: "C".to_string(),
            modifiers: KeyboardShortcutModifiers::default(),
        }
    }

    fn normalized_or(mut self, fallback: Self) -> Self {
        match normalize_key(&self.key) {
            Some(key) => {
                self.key = key;
                self
            }
            None => fallback,
        }
    }
}

fn normalize_key(key: &str) -> Option<String> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    if key.chars().count() == 1 {
        return Some(key.to_uppercase());
    }

    let normalized = match key.to_ascii_lowercase().as_str() {
        "space" => "Space",
        "enter" => "Enter",
        "escape" => "Escape",
        "tab" => "Tab",
        "backspace" => "Backspace",
        "delete" => "Delete",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "arrowup" => "ArrowUp",
        "arrowdown" => "ArrowDown",
        "arrowleft" => "ArrowLeft",
        "arrowright" => "ArrowRight",
        _ => return Some(key.to_string()),
    };
    Some(normalized.to_string())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct KeyboardShortcutModifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum TimeDisplayFormat {
    TwelveHour,
    #[default]
    TwentyFourHour,
}
