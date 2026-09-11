use super::map_app_command_error;
use crate::lifecycle::AppLifecycle;
use crate::runtime::errors::AppCommandError;
use crate::scenes::ScenesCommand;
use tauri::State;
use tokio::sync::oneshot;

/// @cc [owner:mixxorz,label:architecture;safety] abort-routes-through-scenes-owner
/// Abort All MUST be sent through the app-lifetime Scenes owner and await its nested result; the UI
/// adapter MUST NOT address a generation-scoped Fade peer directly or treat mailbox acceptance as
/// successful cancellation.
#[tauri::command]
pub async fn abort_all_fades(lifecycle: State<'_, AppLifecycle>) -> Result<(), String> {
    let scenes = lifecycle.scenes_handle();
    let (reply, rx) = oneshot::channel();
    scenes
        .send(ScenesCommand::AbortAll { reply })
        .await
        .map_err(|_| AppCommandError::ScenesUnavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
        .map_err(map_app_command_error)?;
    Ok(())
}
