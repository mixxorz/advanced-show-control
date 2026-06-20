//! Tauri command adapter exports.

use crate::connection_state::Lv1SystemIdentity;
use crate::lifecycle::AppLifecycle;
use crate::lv1::{Lv1ActorError, Lv1Command};
use crate::runtime::commands::AppCommandError;
use crate::show::{
    ConnectCommandResult, CueSceneResult, LoadShowFileResult, NewShowFileResult, RecallSceneResult,
    SelectedSceneResult, ShowCommandResult,
};
use crate::show_file::{backup_folder, default_show_folder, read_show_file, write_show_file};
use crate::ui::UiLogReceiverState;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, Runtime, State};
use tokio::sync::oneshot;
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
    let systems = crate::lv1::discover(crate::lv1::DiscoverOptions {
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
    let lv1 = lifecycle
        .current_lv1()
        .await
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::GetState { reply })
        .await
        .map_err(|error| match error {
            Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
            other => AppCommandError::CommandFailed(other.to_string()),
        })
        .map_err(map_app_command_error)?;
    let lv1 = rx
        .await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?;
    command_bus
        .load_show_file_from_path(path, file, lv1)
        .await
        .map_err(map_app_command_error)
}

#[tauri::command]
pub async fn save_show_file(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ShowCommandResult, String> {
    save_show_file_with_lifecycle(&lifecycle).await
}

#[tauri::command]
pub async fn save_show_file_as_dialog(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ShowCommandResult, String> {
    let path = choose_save_show_file_path().await?;
    save_show_file_to_path(&lifecycle, path).await
}

async fn save_show_file_with_lifecycle(
    lifecycle: &AppLifecycle,
) -> Result<ShowCommandResult, String> {
    let command_bus = lifecycle.current_command_bus().await;
    if let Some(path) = command_bus
        .current_show_file_path()
        .await
        .map_err(map_app_command_error)?
    {
        return save_show_file_to_path(lifecycle, path).await;
    }

    let path = choose_save_show_file_path().await?;
    save_show_file_to_path(lifecycle, path).await
}

async fn save_show_file_to_path(
    lifecycle: &AppLifecycle,
    path: PathBuf,
) -> Result<ShowCommandResult, String> {
    let command_bus = lifecycle.current_command_bus().await;
    let saved_at = crate::time::current_timestamp_millis();
    let file = command_bus
        .export_show_file_snapshot(saved_at.clone())
        .await
        .map_err(map_app_command_error)?;
    write_show_file(&path, &file, &backup_folder())?;
    command_bus
        .mark_show_file_saved(path, saved_at)
        .await
        .map_err(map_app_command_error)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::events::AppEventBus;
    use crate::show::{SceneConfig, SceneScopeToggles, ShowDocument, ShowStateHandle};

    fn temp_show_file_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "advanced-show-control-ui-commands-{}-{}-{}.lv1show",
            name,
            std::process::id(),
            crate::time::current_timestamp_millis()
        ));
        path
    }

    #[tokio::test]
    async fn save_show_file_uses_existing_show_file_path() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus);
        crate::show::replace_show_document_for_test(
            &show,
            ShowDocument {
                lockout: false,
                scene_configs: vec![SceneConfig {
                    scene_id: "1::Intro".to_string(),
                    scene_index: 1,
                    scene_name: "Intro".to_string(),
                    duration_ms: 1000,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: SceneScopeToggles::default(),
                }],
                cued_scene_id: None,
            },
        )
        .await;
        let lifecycle = AppLifecycle::new(AppEventBus::default(), show.clone());
        let path = temp_show_file_path("save-existing");
        let initial_file = crate::show::export_show_file(
            crate::show::get_show_document(&show).await,
            "saved".to_string(),
        );
        write_show_file(&path, &initial_file, &backup_folder())
            .expect("seed show file should write");
        lifecycle
            .current_command_bus()
            .await
            .mark_show_file_saved(path.clone(), "saved".to_string())
            .await
            .expect("mark saved should set current show file path");

        let result = save_show_file_with_lifecycle(&lifecycle)
            .await
            .expect("save should use existing path");

        assert!(result.changed);
        assert!(path.exists());
        let _ = std::fs::remove_file(path);
    }
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
    tracing::debug!(
        event = "scene_recall_requested",
        scene_id = %scene_id,
        "Scene recall requested"
    );
    let command_bus = lifecycle.current_command_bus().await;
    let lv1 = lifecycle
        .current_lv1()
        .await
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(|error| {
            let message = map_app_command_error(match error {
                AppCommandError::Lv1Unavailable => AppCommandError::CommandFailed(
                    "Recall blocked: LV1 state is unavailable".to_string(),
                ),
                other => other,
            });
            tracing::warn!(
                event = "scene_recall_blocked",
                scene_id = %scene_id,
                reason = %message,
                "Scene recall blocked: {message}"
            );
            message
        })?;
    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::GetState { reply })
        .await
        .map_err(|error| match error {
            Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
            other => AppCommandError::CommandFailed(other.to_string()),
        })
        .map_err(|error| {
            let message = map_app_command_error(match error {
                AppCommandError::Lv1Unavailable => AppCommandError::CommandFailed(
                    "Recall blocked: LV1 state is unavailable".to_string(),
                ),
                other => other,
            });
            tracing::warn!(
                event = "scene_recall_blocked",
                scene_id = %scene_id,
                reason = %message,
                "Scene recall blocked: {message}"
            );
            message
        })?;
    let lv1_snapshot = rx
        .await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(|error| {
            let message = map_app_command_error(match error {
                AppCommandError::Lv1Unavailable => AppCommandError::CommandFailed(
                    "Recall blocked: LV1 state is unavailable".to_string(),
                ),
                other => other,
            });
            tracing::warn!(
                event = "scene_recall_blocked",
                scene_id = %scene_id,
                reason = %message,
                "Scene recall blocked: {message}"
            );
            message
        })?;
    let show_document = command_bus
        .get_show_document()
        .await
        .map_err(map_app_command_error)?;
    let result =
        crate::show::validate_recall_scene_request(&show_document, &lv1_snapshot, &scene_id)
            .map_err(|message| {
                tracing::warn!(
                    event = "scene_recall_blocked",
                    scene_id = %scene_id,
                    reason = %message,
                    "Scene recall blocked: {message}"
                );
                message
            })?;

    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::RecallScene {
        scene_index: result.lv1_scene_index,
        reply,
    })
    .await
    .map_err(|error| match error {
        Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
        other => AppCommandError::CommandFailed(other.to_string()),
    })
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
        .map_err(|error| match error {
            Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
            other => AppCommandError::CommandFailed(other.to_string()),
        })
        .map_err(map_app_command_error)?;
    tracing::debug!(
        event = "scene_recall_command_sent",
        scene_id = %result.scene.scene_id,
        scene_index = result.scene.scene_index,
        scene_name = %result.scene.scene_name,
        "Scene recall command sent: {}",
        result.scene.scene_name
    );
    Ok(result)
}

#[tauri::command]
pub async fn connect_lv1_system(
    app: AppHandle<impl Runtime>,
    lifecycle: State<'_, AppLifecycle>,
    identity: Lv1SystemIdentity,
) -> Result<ConnectCommandResult, String> {
    lifecycle.connect_lv1_system(app, identity).await
}

#[tauri::command]
pub async fn attempt_reconnect_lv1(
    app: AppHandle<impl Runtime>,
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ConnectCommandResult, String> {
    lifecycle.attempt_reconnect_lv1(app).await
}

#[tauri::command]
pub async fn startup_auto_connect_lv1(
    app: AppHandle<impl Runtime>,
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ConnectCommandResult, String> {
    lifecycle.startup_auto_connect_lv1(app).await
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
    let command_bus = lifecycle.current_command_bus().await;
    let lv1 = lifecycle
        .current_lv1()
        .await
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(|error| {
            map_app_command_error(match error {
                AppCommandError::Lv1Unavailable => AppCommandError::CommandFailed(
                    "Store scene blocked: LV1 state is unavailable".to_string(),
                ),
                other => other,
            })
        })?;
    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::GetState { reply })
        .await
        .map_err(|error| match error {
            Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
            other => AppCommandError::CommandFailed(other.to_string()),
        })
        .map_err(|error| {
            map_app_command_error(match error {
                AppCommandError::Lv1Unavailable => AppCommandError::CommandFailed(
                    "Store scene blocked: LV1 state is unavailable".to_string(),
                ),
                other => other,
            })
        })?;
    let lv1 = rx
        .await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(|error| {
            map_app_command_error(match error {
                AppCommandError::Lv1Unavailable => AppCommandError::CommandFailed(
                    "Store scene blocked: LV1 state is unavailable".to_string(),
                ),
                other => other,
            })
        })?;
    command_bus
        .store_scene_config(scene_id, lv1.channels)
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
