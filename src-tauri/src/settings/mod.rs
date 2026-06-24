mod actor;
mod commands;
mod events;
mod handle;
mod state;
mod types;

pub use actor::{SettingsActorTask, build_settings_actor};
pub use commands::{SettingsCommand, SettingsCommandResult};
pub use events::SettingsEvent;
pub use handle::SettingsHandle;
pub use types::{
    AppSettings, KeyboardShortcut, KeyboardShortcutModifiers, KeyboardShortcutSettings,
    TimeDisplayFormat,
};
