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
        assert!(!settings.auto_cue_next_scene_on_go);
        assert_eq!(settings.time_display, TimeDisplayFormat::TwentyFourHour);
        assert_eq!(settings.fader_override_sensitivity, 9);
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
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct AppSettings {
    pub auto_load_last_show_file: bool,
    pub auto_save_sessions: bool,
    pub keyboard_shortcuts: KeyboardShortcutSettings,
    pub auto_cue_next_scene_on_go: bool,
    pub time_display: TimeDisplayFormat,
    pub fader_override_sensitivity: u8,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            auto_load_last_show_file: false,
            auto_save_sessions: false,
            keyboard_shortcuts: KeyboardShortcutSettings::default(),
            auto_cue_next_scene_on_go: false,
            time_display: TimeDisplayFormat::TwentyFourHour,
            fader_override_sensitivity: 9,
        }
    }
}

impl AppSettings {
    pub fn normalized(mut self) -> Self {
        self.fader_override_sensitivity = self.fader_override_sensitivity.clamp(1, 10);
        self.keyboard_shortcuts = self.keyboard_shortcuts.normalized();
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct KeyboardShortcutSettings {
    pub go: KeyboardShortcut,
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
        self.key = self.key.trim().to_string();
        if self.key.is_empty() { fallback } else { self }
    }
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
