use tauri::State;
use tokio::sync::oneshot;

use super::map_app_command_error;
use crate::runtime::errors::AppCommandError;
use crate::settings::{AppSettings, SettingsCommand, SettingsCommandResult, SettingsHandle};

#[tauri::command]
pub async fn replace_app_settings(
    settings_handle: State<'_, SettingsHandle>,
    settings: AppSettings,
) -> Result<SettingsCommandResult, String> {
    let (reply, rx) = oneshot::channel();
    settings_handle
        .send(SettingsCommand::ReplaceSettings { settings, reply })
        .await
        .map_err(|_| AppCommandError::CommandFailed("Settings unavailable".to_string()))
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}
