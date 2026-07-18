use super::map_app_command_error;
use crate::runtime::errors::AppCommandError;
use crate::show::{
    LoadShowFileResult, NewShowFileResult, ShowCommand, ShowCommandResult, ShowStateHandle,
};
use crate::show_file::default_show_folder;
use std::path::PathBuf;
use tauri::State;
use tokio::sync::oneshot;
use tokio::task::spawn_blocking;

#[tauri::command]
pub async fn refresh_lv1_discovery(
    show: State<'_, ShowStateHandle>,
    timeout_ms: Option<u64>,
) -> Result<ShowCommandResult, String> {
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
pub async fn new_show_file(show: State<'_, ShowStateHandle>) -> Result<NewShowFileResult, String> {
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
    show: State<'_, ShowStateHandle>,
) -> Result<LoadShowFileResult, String> {
    let path = spawn_blocking(|| -> Result<Option<PathBuf>, String> {
        let folder = default_show_folder();
        Ok(rfd::FileDialog::new()
            .set_directory(folder)
            .add_filter("Advanced Show Control Session", &["ascs"])
            .pick_file())
    })
    .await
    .map_err(|err| format!("Failed to open file dialog: {err}"))??
    .ok_or_else(|| "Open session cancelled".to_string())?;
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
pub async fn save_show_file(show: State<'_, ShowStateHandle>) -> Result<ShowCommandResult, String> {
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
                .set_file_name("Untitled.ascs")
                .add_filter("Advanced Show Control Session", &["ascs"])
                .save_file())
        })
        .await
        .map_err(|err| format!("Failed to open save dialog: {err}"))??
        .ok_or_else(|| "Save session cancelled".to_string())?,
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
    show: State<'_, ShowStateHandle>,
) -> Result<ShowCommandResult, String> {
    let path = spawn_blocking(|| -> Result<Option<PathBuf>, String> {
        let folder = default_show_folder();
        Ok(rfd::FileDialog::new()
            .set_directory(folder)
            .set_file_name("Untitled.ascs")
            .add_filter("Advanced Show Control Session", &["ascs"])
            .save_file())
    })
    .await
    .map_err(|err| format!("Failed to open save dialog: {err}"))??
    .ok_or_else(|| "Save session cancelled".to_string())?;
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
pub async fn set_lockout(
    show: State<'_, ShowStateHandle>,
    enabled: bool,
) -> Result<ShowCommandResult, String> {
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
