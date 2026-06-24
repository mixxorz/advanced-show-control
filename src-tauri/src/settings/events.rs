use super::AppSettings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEvent {
    StateChanged { settings: AppSettings },
}
