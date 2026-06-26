use super::map_app_command_error;
use crate::lifecycle::AppLifecycle;
use crate::runtime::errors::AppCommandError;
use crate::scenes::{
    CueSceneResult, RecallSceneResult, ScenesCommand, ScenesCommandResult, SelectedSceneResult,
};
use tauri::State;
use tokio::sync::oneshot;

#[tauri::command]
pub async fn recall_scene(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
) -> Result<RecallSceneResult, String> {
    let scene_recall = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let (reply, rx) = oneshot::channel();
    scene_recall
        .send(ScenesCommand::RecallScene {
            internal_scene_id,
            reply,
        })
        .await
        .map_err(|_| AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
        .map_err(map_app_command_error)
}

#[tauri::command]
pub async fn set_scene_duration_ms(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
    duration_ms: u64,
) -> Result<ScenesCommandResult, String> {
    let scenes = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let (reply, rx) = oneshot::channel();
    scenes
        .send(ScenesCommand::SetSceneDuration {
            internal_scene_id,
            duration_ms,
            reply: Some(reply),
        })
        .await
        .map_err(|_| AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn link_scene_config(
    lifecycle: State<'_, AppLifecycle>,
    source_internal_scene_id: uuid::Uuid,
    target_scene_index: i32,
    overwrite_existing: bool,
) -> Result<ScenesCommandResult, String> {
    let scenes = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let (reply, rx) = oneshot::channel();
    scenes
        .send(ScenesCommand::LinkSceneConfig {
            source_internal_scene_id,
            target_scene_index,
            overwrite_existing,
            reply: Some(reply),
        })
        .await
        .map_err(|_| AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn delete_scene_config(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
) -> Result<ScenesCommandResult, String> {
    let scenes = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let (reply, rx) = oneshot::channel();
    scenes
        .send(ScenesCommand::DeleteSceneConfig {
            internal_scene_id,
            reply: Some(reply),
        })
        .await
        .map_err(|_| AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn select_scene_config(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
) -> Result<SelectedSceneResult, String> {
    let scenes = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let (reply, rx) = oneshot::channel();
    scenes
        .send(ScenesCommand::SelectSceneConfig {
            internal_scene_id,
            reply: Some(reply),
        })
        .await
        .map_err(|_| AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn cue_scene(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
) -> Result<CueSceneResult, String> {
    let scenes = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let (reply, rx) = oneshot::channel();
    scenes
        .send(ScenesCommand::CueScene {
            internal_scene_id,
            reply: Some(reply),
        })
        .await
        .map_err(|_| AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn store_scene_config_from_current_lv1(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
) -> Result<ScenesCommandResult, String> {
    let scenes = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let (reply, rx) = oneshot::channel();
    scenes
        .send(ScenesCommand::StoreSceneConfigFromCurrentLv1 {
            internal_scene_id,
            reply: Some(reply),
        })
        .await
        .map_err(|_| AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn set_all_channels_scoped(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
    scoped: bool,
) -> Result<ScenesCommandResult, String> {
    let scenes = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let (reply, rx) = oneshot::channel();
    scenes
        .send(ScenesCommand::SetAllChannelsScoped {
            internal_scene_id,
            scoped,
            reply: Some(reply),
        })
        .await
        .map_err(|_| AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn set_scene_scope_faders_enabled(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
    enabled: bool,
) -> Result<ScenesCommandResult, String> {
    let scenes = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let (reply, rx) = oneshot::channel();
    scenes
        .send(ScenesCommand::SetSceneScopeFadersEnabled {
            internal_scene_id,
            enabled,
            reply: Some(reply),
        })
        .await
        .map_err(|_| AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn set_scene_scope_pan_enabled(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
    enabled: bool,
) -> Result<ScenesCommandResult, String> {
    let scenes = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let (reply, rx) = oneshot::channel();
    scenes
        .send(ScenesCommand::SetSceneScopePanEnabled {
            internal_scene_id,
            enabled,
            reply: Some(reply),
        })
        .await
        .map_err(|_| AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn set_channel_scoped(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
    group: i32,
    channel: i32,
    scoped: bool,
) -> Result<ScenesCommandResult, String> {
    let scenes = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let (reply, rx) = oneshot::channel();
    scenes
        .send(ScenesCommand::SetChannelScoped {
            internal_scene_id,
            group,
            channel,
            scoped,
            reply: Some(reply),
        })
        .await
        .map_err(|_| AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}
