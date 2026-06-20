use super::map_app_command_error;
use crate::lifecycle::AppLifecycle;
use crate::runtime::errors::AppCommandError;
use crate::scenes::ScenesCommand;
use crate::show::RecallSceneResult;
use tauri::State;
use tokio::sync::oneshot;

#[tauri::command]
pub async fn recall_scene(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
) -> Result<RecallSceneResult, String> {
    let scene_recall = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let (reply, rx) = oneshot::channel();
    scene_recall
        .send(ScenesCommand::RecallScene { scene_id, reply })
        .await
        .map_err(|_| AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
        .map_err(map_app_command_error)
}
