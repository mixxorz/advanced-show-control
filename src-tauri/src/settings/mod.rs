mod actor;
mod commands;
mod events;
mod state;
mod types;

pub use actor::{SettingsActorTask, build_settings_actor};
pub use commands::{SettingsCommand, SettingsCommandResult};
pub use events::SettingsEvent;
pub type SettingsHandle = tokio::sync::mpsc::Sender<SettingsCommand>;
pub use types::{
    AppSettings, KeyboardShortcut, KeyboardShortcutModifiers, KeyboardShortcutSettings,
    TimeDisplayFormat,
};
