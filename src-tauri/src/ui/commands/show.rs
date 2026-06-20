use super::map_app_command_error;
use crate::lifecycle::AppLifecycle;
use crate::runtime::errors::AppCommandError;
use crate::show::{
    CueSceneResult, LoadShowFileResult, NewShowFileResult, SelectedSceneResult, ShowCommand,
    ShowCommandResult,
};
use crate::show_file::default_show_folder;
use std::path::PathBuf;
use tauri::State;
use tokio::sync::oneshot;
use tokio::task::spawn_blocking;

#[tauri::command]
pub async fn refresh_lv1_discovery(
    lifecycle: State<'_, AppLifecycle>,
    timeout_ms: Option<u64>,
) -> Result<ShowCommandResult, String> {
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::RefreshLv1Discovery {
        timeout_ms,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn new_show_file(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<NewShowFileResult, String> {
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::NewShowFileFromCurrentLv1 { reply: Some(reply) })
        .await
        .map_err(|_| AppCommandError::ShowUnavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn open_show_file_dialog(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<LoadShowFileResult, String> {
    let path = spawn_blocking(|| -> Result<Option<PathBuf>, String> {
        let folder = default_show_folder();
        Ok(rfd::FileDialog::new()
            .set_directory(folder)
            .add_filter("LV1 Show", &["lv1show"])
            .pick_file())
    })
    .await
    .map_err(|err| format!("Failed to open file dialog: {err}"))??
    .ok_or_else(|| "Open show file cancelled".to_string())?;
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::LoadShowFileFromPath {
        path,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn save_show_file(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ShowCommandResult, String> {
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::CurrentShowFilePath { reply })
        .await
        .map_err(|_| AppCommandError::ShowUnavailable)
        .map_err(map_app_command_error)?;
    let path = match rx
        .await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
    {
        Some(path) => path,
        None => spawn_blocking(|| -> Result<Option<PathBuf>, String> {
            let folder = default_show_folder();
            Ok(rfd::FileDialog::new()
                .set_directory(folder)
                .set_file_name("Untitled.lv1show")
                .add_filter("LV1 Show", &["lv1show"])
                .save_file())
        })
        .await
        .map_err(|err| format!("Failed to open save dialog: {err}"))??
        .ok_or_else(|| "Save show file cancelled".to_string())?,
    };
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::SaveShowFileAs {
        path,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn save_show_file_as_dialog(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ShowCommandResult, String> {
    let path = spawn_blocking(|| -> Result<Option<PathBuf>, String> {
        let folder = default_show_folder();
        Ok(rfd::FileDialog::new()
            .set_directory(folder)
            .set_file_name("Untitled.lv1show")
            .add_filter("LV1 Show", &["lv1show"])
            .save_file())
    })
    .await
    .map_err(|err| format!("Failed to open save dialog: {err}"))??
    .ok_or_else(|| "Save show file cancelled".to_string())?;
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::SaveShowFileAs {
        path,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn set_scene_duration_ms(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    duration_ms: u64,
) -> Result<ShowCommandResult, String> {
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::SetSceneDuration {
        scene_id,
        duration_ms,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn select_scene_config(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
) -> Result<SelectedSceneResult, String> {
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::SelectSceneConfig {
        scene_id,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn cue_scene(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
) -> Result<CueSceneResult, String> {
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::CueScene {
        scene_id,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn store_scene_config(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
) -> Result<ShowCommandResult, String> {
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::StoreSceneConfigFromCurrentLv1 {
        scene_id,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn set_channel_scoped(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    group: i32,
    channel: i32,
    scoped: bool,
) -> Result<ShowCommandResult, String> {
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::SetChannelScoped {
        scene_id,
        group,
        channel,
        scoped,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn set_all_channels_scoped(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    scoped: bool,
) -> Result<ShowCommandResult, String> {
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::SetAllChannelsScoped {
        scene_id,
        scoped,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn set_scene_scope_faders_enabled(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    enabled: bool,
) -> Result<ShowCommandResult, String> {
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::SetSceneScopeFadersEnabled {
        scene_id,
        enabled,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn set_scene_scope_pan_enabled(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    enabled: bool,
) -> Result<ShowCommandResult, String> {
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::SetSceneScopePanEnabled {
        scene_id,
        enabled,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}

#[tauri::command]
pub async fn set_lockout(
    lifecycle: State<'_, AppLifecycle>,
    enabled: bool,
) -> Result<ShowCommandResult, String> {
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::SetLockout {
        enabled,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)
}
