use super::commands::{
    new_show_file, open_show_file_dialog, save_show_file, save_show_file_as_dialog,
};
#[cfg(target_os = "macos")]
use tauri::menu::PredefinedMenuItem;
use tauri::menu::{Menu, MenuEvent, MenuItem, Submenu};
use tauri::{App, AppHandle, Manager};

pub const MENU_NEW_SESSION: &str = "session:new";
pub const MENU_OPEN_SESSION: &str = "session:open";
pub const MENU_SAVE_SESSION: &str = "session:save";
pub const MENU_SAVE_SESSION_AS: &str = "session:save-as";
pub const MENU_NEW_SESSION_ACCELERATOR: &str = "CmdOrCtrl+N";
pub const MENU_OPEN_SESSION_ACCELERATOR: &str = "CmdOrCtrl+O";
pub const MENU_SAVE_SESSION_ACCELERATOR: &str = "CmdOrCtrl+S";
pub const MENU_SAVE_SESSION_AS_ACCELERATOR: &str = "CmdOrCtrl+Shift+S";
#[cfg(target_os = "macos")]
const APP_DISPLAY_NAME: &str = "Advanced Show Control";
#[cfg(target_os = "macos")]
const MENU_ABOUT_APP: &str = "About Advanced Show Control";
#[cfg(target_os = "macos")]
const MENU_HIDE_APP: &str = "Hide Advanced Show Control";
#[cfg(target_os = "macos")]
const MENU_QUIT_APP: &str = "Quit Advanced Show Control";

pub fn install_session_menu(app: &mut App<tauri::Wry>) -> tauri::Result<()> {
    let handle = app.handle();
    #[cfg(target_os = "macos")]
    let app_menu = Submenu::with_items(
        handle,
        APP_DISPLAY_NAME,
        true,
        &[
            &PredefinedMenuItem::about(handle, Some(MENU_ABOUT_APP), None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::services(handle, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::hide(handle, Some(MENU_HIDE_APP))?,
            &PredefinedMenuItem::hide_others(handle, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::quit(handle, Some(MENU_QUIT_APP))?,
        ],
    )?;
    let file_menu = Submenu::with_items(
        handle,
        "File",
        true,
        &[
            &MenuItem::with_id(
                handle,
                MENU_NEW_SESSION,
                "New Session",
                true,
                Some(MENU_NEW_SESSION_ACCELERATOR),
            )?,
            &MenuItem::with_id(
                handle,
                MENU_OPEN_SESSION,
                "Open Session...",
                true,
                Some(MENU_OPEN_SESSION_ACCELERATOR),
            )?,
            &MenuItem::with_id(
                handle,
                MENU_SAVE_SESSION,
                "Save Session",
                true,
                Some(MENU_SAVE_SESSION_ACCELERATOR),
            )?,
            &MenuItem::with_id(
                handle,
                MENU_SAVE_SESSION_AS,
                "Save As...",
                true,
                Some(MENU_SAVE_SESSION_AS_ACCELERATOR),
            )?,
        ],
    )?;
    let menu = Menu::with_items(
        handle,
        &[
            #[cfg(target_os = "macos")]
            &app_menu,
            &file_menu,
        ],
    )?;
    app.set_menu(menu)?;
    Ok(())
}

pub fn handle_session_menu_event(app: &AppHandle<tauri::Wry>, event: MenuEvent) {
    let id = event.id().as_ref();
    let app = app.clone();
    match id {
        MENU_NEW_SESSION => tauri::async_runtime::spawn(async move {
            if let Err(err) = new_show_file(app.state()).await {
                tracing::warn!(event = "session_menu_command_failed", error = %err, "New Session menu command failed: {err}");
            }
        }),
        MENU_OPEN_SESSION => tauri::async_runtime::spawn(async move {
            if let Err(err) = open_show_file_dialog(app.state()).await {
                tracing::warn!(event = "session_menu_command_failed", error = %err, "Open Session menu command failed: {err}");
            }
        }),
        MENU_SAVE_SESSION => tauri::async_runtime::spawn(async move {
            if let Err(err) = save_show_file(app.state()).await {
                tracing::warn!(event = "session_menu_command_failed", error = %err, "Save Session menu command failed: {err}");
            }
        }),
        MENU_SAVE_SESSION_AS => tauri::async_runtime::spawn(async move {
            if let Err(err) = save_show_file_as_dialog(app.state()).await {
                tracing::warn!(event = "session_menu_command_failed", error = %err, "Save As menu command failed: {err}");
            }
        }),
        _ => return,
    };
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

    #[test]
    fn file_menu_accelerators_are_standard() {
        assert_eq!(MENU_NEW_SESSION_ACCELERATOR, "CmdOrCtrl+N");
        assert_eq!(MENU_OPEN_SESSION_ACCELERATOR, "CmdOrCtrl+O");
        assert_eq!(MENU_SAVE_SESSION_ACCELERATOR, "CmdOrCtrl+S");
        assert_eq!(MENU_SAVE_SESSION_AS_ACCELERATOR, "CmdOrCtrl+Shift+S");
    }
}
