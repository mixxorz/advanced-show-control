use super::map_app_command_error;
use crate::fade::FadeCommand;
use crate::lifecycle::AppLifecycle;
use crate::runtime::errors::AppCommandError;
use tauri::State;
use tokio::sync::oneshot;

#[tauri::command]
pub async fn abort_all_fades(lifecycle: State<'_, AppLifecycle>) -> Result<(), String> {
    let fade = lifecycle
        .current_fade()
        .await
        .ok_or(AppCommandError::FadeUnavailable)
        .map_err(map_app_command_error)?;
    let (reply, rx) = oneshot::channel();
    fade.send(FadeCommand::AbortAll { reply: Some(reply) })
        .await
        .map_err(|_| AppCommandError::FadeUnavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
        .map_err(map_app_command_error)?;
    Ok(())
}
