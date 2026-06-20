//! Tauri command adapter exports.

use crate::lifecycle::AppLifecycle;
use crate::runtime::commands::AppCommandError;
use crate::show::commands::{
    ConnectCommandResult, CueSceneResult, LoadShowFileResult, NewShowFileResult, RecallSceneResult,
    SelectedSceneResult, ShowCommandResult,
};
use crate::show_file::{backup_folder, default_show_folder, read_show_file, write_show_file};
use crate::ui::UiLogReceiverState;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, Runtime, State};
use tokio::task::spawn_blocking;

fn map_app_command_error(error: AppCommandError) -> String {
    match error {
        AppCommandError::CommandFailed(message) => message,
        other => other.to_string(),
    }
}

#[tauri::command]
pub async fn frontend_ready<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, AppLifecycle>,
) -> Result<(), String> {
    let logs = app.state::<UiLogReceiverState>().subscribe();
    lifecycle.frontend_ready(app, logs).await
}

#[tauri::command]
pub async fn refresh_lv1_discovery(
    lifecycle: State<'_, AppLifecycle>,
    timeout_ms: Option<u64>,
) -> Result<ShowCommandResult, String> {
    let command_bus = lifecycle.current_command_bus().await;
    let systems = crate::lv1::discovery::discover(crate::lv1::discovery::DiscoverOptions {
        timeout: std::time::Duration::from_millis(timeout_ms.unwrap_or(1000).clamp(100, 6000)),
        ..Default::default()
    })
    .map_err(|err| format!("Failed to discover LV1 systems: {err}"))?
    .iter()
    .filter_map(crate::connection_state::identity_from_discovery)
    .map(|identity| crate::connection_state::DiscoveredLv1System {
        identity,
        latency_ms: None,
        status: crate::connection_state::DiscoveredLv1Status::Available,
    })
    .collect();
    command_bus
        .set_discovered_lv1_systems(systems)
        .await
        .map_err(map_app_command_error)
}

#[tauri::command]
pub async fn new_show_file(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<NewShowFileResult, String> {
    let command_bus = lifecycle.current_command_bus().await;
    command_bus
        .new_show_file(None)
        .await
        .map_err(map_app_command_error)
}

#[tauri::command]
pub async fn open_show_file_dialog(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<LoadShowFileResult, String> {
    let path = choose_open_show_file_path().await?;
    let file = read_show_file(&path)?;
    let command_bus = lifecycle.current_command_bus().await;
    let lv1 = command_bus
        .get_lv1_state()
        .await
        .map_err(map_app_command_error)?;
    command_bus
        .load_show_file_from_dto(path, file, lv1)
        .await
        .map_err(map_app_command_error)
}

#[tauri::command]
pub async fn save_show_file(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ShowCommandResult, String> {
    save_show_file_as_dialog(lifecycle).await
}

#[tauri::command]
pub async fn save_show_file_as_dialog(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ShowCommandResult, String> {
    let path = choose_save_show_file_path().await?;
    let command_bus = lifecycle.current_command_bus().await;
    let file = command_bus
        .export_show_file_for_save(String::new())
        .await
        .map_err(map_app_command_error)?;
    write_show_file(&path, &file, &backup_folder())?;
    Ok(ShowCommandResult { changed: true })
}

async fn choose_open_show_file_path() -> Result<PathBuf, String> {
    spawn_blocking(|| -> Result<Option<PathBuf>, String> {
        let folder = default_show_folder();
        Ok(rfd::FileDialog::new()
            .set_directory(folder)
            .add_filter("LV1 Show", &["lv1show"])
            .pick_file())
    })
    .await
    .map_err(|err| format!("Failed to open file dialog: {err}"))??
    .ok_or_else(|| "Open show file cancelled".to_string())
}

async fn choose_save_show_file_path() -> Result<PathBuf, String> {
    spawn_blocking(|| -> Result<Option<PathBuf>, String> {
        let folder = default_show_folder();
        Ok(rfd::FileDialog::new()
            .set_directory(folder)
            .set_file_name("Untitled.lv1show")
            .add_filter("LV1 Show", &["lv1show"])
            .save_file())
    })
    .await
    .map_err(|err| format!("Failed to open save dialog: {err}"))??
    .ok_or_else(|| "Save show file cancelled".to_string())
}

#[tauri::command]
pub async fn set_scene_duration_ms(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    duration_ms: u64,
) -> Result<ShowCommandResult, String> {
    lifecycle
        .current_command_bus()
        .await
        .set_scene_duration_ms(scene_id, duration_ms)
        .await
        .map_err(map_app_command_error)
}

#[tauri::command]
pub async fn select_scene_config(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
) -> Result<SelectedSceneResult, String> {
    lifecycle
        .current_command_bus()
        .await
        .select_scene_config(scene_id)
        .await
        .map_err(map_app_command_error)
}

#[tauri::command]
pub async fn cue_scene(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
) -> Result<CueSceneResult, String> {
    lifecycle
        .current_command_bus()
        .await
        .cue_scene(scene_id)
        .await
        .map_err(map_app_command_error)
}

#[tauri::command]
pub async fn recall_scene(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
) -> Result<RecallSceneResult, String> {
    lifecycle
        .current_command_bus()
        .await
        .recall_scene_by_id(scene_id)
        .await
        .map_err(map_app_command_error)
}

#[tauri::command]
pub async fn connect_lv1(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ConnectCommandResult, String> {
    let _ = lifecycle.begin_connecting().await;
    Ok(ConnectCommandResult { changed: true })
}

#[tauri::command]
pub async fn connect_lv1_system(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ConnectCommandResult, String> {
    let _ = lifecycle.begin_connecting().await;
    Ok(ConnectCommandResult { changed: true })
}

#[tauri::command]
pub async fn attempt_reconnect_lv1(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ConnectCommandResult, String> {
    let _ = lifecycle.begin_connecting().await;
    Ok(ConnectCommandResult { changed: true })
}

#[tauri::command]
pub async fn startup_auto_connect_lv1(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ConnectCommandResult, String> {
    let _ = lifecycle.begin_connecting().await;
    Ok(ConnectCommandResult { changed: true })
}

#[tauri::command]
pub async fn disconnect_lv1(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ShowCommandResult, String> {
    lifecycle.disconnect_current_runtime().await
}

#[tauri::command]
pub async fn reconnect_timed_out(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ShowCommandResult, String> {
    lifecycle.disconnect_current_runtime().await
}

#[tauri::command]
pub async fn abort_all_fades(lifecycle: State<'_, AppLifecycle>) -> Result<(), String> {
    lifecycle
        .current_command_bus()
        .await
        .abort_all_fades()
        .await
        .map_err(map_app_command_error)?;
    Ok(())
}

#[tauri::command]
pub async fn store_scene_config(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
) -> Result<ShowCommandResult, String> {
    lifecycle
        .current_command_bus()
        .await
        .store_scene_config(scene_id, vec![])
        .await
        .map_err(map_app_command_error)
}

#[tauri::command]
pub async fn set_channel_scoped(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    group: i32,
    channel: i32,
    scoped: bool,
) -> Result<ShowCommandResult, String> {
    lifecycle
        .current_command_bus()
        .await
        .set_channel_scoped(scene_id, group, channel, scoped)
        .await
        .map_err(map_app_command_error)
}

#[tauri::command]
pub async fn set_all_channels_scoped(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    scoped: bool,
) -> Result<ShowCommandResult, String> {
    lifecycle
        .current_command_bus()
        .await
        .set_all_channels_scoped(scene_id, scoped)
        .await
        .map_err(map_app_command_error)
}

#[tauri::command]
pub async fn set_scene_scope_faders_enabled(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    enabled: bool,
) -> Result<ShowCommandResult, String> {
    lifecycle
        .current_command_bus()
        .await
        .set_scene_scope_faders_enabled(scene_id, enabled)
        .await
        .map_err(map_app_command_error)
}

#[tauri::command]
pub async fn set_scene_scope_pan_enabled(
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    enabled: bool,
) -> Result<ShowCommandResult, String> {
    lifecycle
        .current_command_bus()
        .await
        .set_scene_scope_pan_enabled(scene_id, enabled)
        .await
        .map_err(map_app_command_error)
}

#[tauri::command]
pub async fn set_lockout(
    lifecycle: State<'_, AppLifecycle>,
    enabled: bool,
) -> Result<ShowCommandResult, String> {
    lifecycle
        .current_command_bus()
        .await
        .set_lockout(enabled)
        .await
        .map_err(map_app_command_error)
}
