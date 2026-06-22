use crate::lifecycle::AppLifecycle;
use crate::runtime::errors::AppCommandError;
use crate::show::{LoadShowFileResult, NewShowFileResult, ShowCommand, ShowCommandResult};
use crate::show_file::default_show_folder;
use std::path::PathBuf;
use tauri::menu::{Menu, MenuEvent, MenuItem, Submenu};
use tauri::{App, AppHandle, Manager};
use tokio::sync::oneshot;
use tokio::task::spawn_blocking;

pub const MENU_NEW_SESSION: &str = "session:new";
pub const MENU_OPEN_SESSION: &str = "session:open";
pub const MENU_SAVE_SESSION: &str = "session:save";
pub const MENU_SAVE_SESSION_AS: &str = "session:save-as";

pub fn install_session_menu(app: &mut App<tauri::Wry>) -> tauri::Result<()> {
    let handle = app.handle();
    let file_menu = Submenu::with_items(
        handle,
        "File",
        true,
        &[
            &MenuItem::with_id(handle, MENU_NEW_SESSION, "New Session", true, None::<&str>)?,
            &MenuItem::with_id(
                handle,
                MENU_OPEN_SESSION,
                "Open Session...",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                handle,
                MENU_SAVE_SESSION,
                "Save Session",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                handle,
                MENU_SAVE_SESSION_AS,
                "Save As...",
                true,
                None::<&str>,
            )?,
        ],
    )?;
    let menu = Menu::with_items(handle, &[&file_menu])?;
    app.set_menu(menu)?;
    Ok(())
}

pub fn handle_session_menu_event(app: &AppHandle<tauri::Wry>, event: MenuEvent) {
    let id = event.id().as_ref();
    let app = app.clone();
    match id {
        MENU_NEW_SESSION => tauri::async_runtime::spawn(async move {
            if let Err(err) = new_session_from_menu(app).await {
                tracing::warn!(error = %err, "New Session menu command failed");
            }
        }),
        MENU_OPEN_SESSION => tauri::async_runtime::spawn(async move {
            if let Err(err) = open_session_from_menu(app).await {
                tracing::warn!(error = %err, "Open Session menu command failed");
            }
        }),
        MENU_SAVE_SESSION => tauri::async_runtime::spawn(async move {
            if let Err(err) = save_session_from_menu(app).await {
                tracing::warn!(error = %err, "Save Session menu command failed");
            }
        }),
        MENU_SAVE_SESSION_AS => tauri::async_runtime::spawn(async move {
            if let Err(err) = save_session_as_from_menu(app).await {
                tracing::warn!(error = %err, "Save As menu command failed");
            }
        }),
        _ => return,
    };
}

async fn new_session_from_menu(app: AppHandle<tauri::Wry>) -> Result<NewShowFileResult, String> {
    let lifecycle = app.state::<AppLifecycle>();
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::NewShowFileFromCurrentLv1 { reply: Some(reply) })
        .await
        .map_err(|_| AppCommandError::ShowUnavailable)
        .map_err(super::commands::map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(super::commands::map_app_command_error)?
}

async fn open_session_from_menu(app: AppHandle<tauri::Wry>) -> Result<LoadShowFileResult, String> {
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
    let lifecycle = app.state::<AppLifecycle>();
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::LoadShowFileFromPath {
        path,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(super::commands::map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(super::commands::map_app_command_error)?
}

async fn save_session_from_menu(app: AppHandle<tauri::Wry>) -> Result<ShowCommandResult, String> {
    let lifecycle = app.state::<AppLifecycle>();
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::CurrentShowFilePath { reply })
        .await
        .map_err(|_| AppCommandError::ShowUnavailable)
        .map_err(super::commands::map_app_command_error)?;
    let path = match rx
        .await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(super::commands::map_app_command_error)?
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
    .map_err(super::commands::map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(super::commands::map_app_command_error)?
}

async fn save_session_as_from_menu(
    app: AppHandle<tauri::Wry>,
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
    let lifecycle = app.state::<AppLifecycle>();
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::SaveShowFileAs {
        path,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(super::commands::map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(super::commands::map_app_command_error)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_are_stable() {
        assert_eq!(MENU_NEW_SESSION, "session:new");
        assert_eq!(MENU_OPEN_SESSION, "session:open");
        assert_eq!(MENU_SAVE_SESSION, "session:save");
        assert_eq!(MENU_SAVE_SESSION_AS, "session:save-as");
    }
}
