//! Tauri command adapter exports.

use crate::connection_state::Lv1SystemIdentity;
use crate::fade::FadeCommand;
use crate::lifecycle::AppLifecycle;
use crate::runtime::errors::AppCommandError;
use crate::scene_recall::SceneRecallCommand;
use crate::show::{
    ConnectCommandResult, CueSceneResult, LoadShowFileResult, NewShowFileResult, RecallSceneResult,
    SelectedSceneResult, ShowCommand, ShowCommandResult,
};
use crate::show_file::default_show_folder;
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
    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::SetDiscoveredLv1Systems {
        systems,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)
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
    let path = choose_open_show_file_path().await?;
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
        None => choose_save_show_file_path().await?,
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

async fn save_show_file_to_path(
    lifecycle: &AppLifecycle,
    path: PathBuf,
) -> Result<ShowCommandResult, String> {
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
    use crate::show::{SceneConfig, SceneScopeToggles, ShowDocument};

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
        let (show, show_task, show_peers) = crate::show::build_show_actor(event_bus.clone());
        show_task.spawn();
        let (reply, rx) = oneshot::channel();
        show.send(ShowCommand::ReplaceSnapshotForTest {
            snapshot: ShowDocument {
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
            reply: Some(reply),
        })
        .await
        .unwrap();
        let _ = rx.await;
        let lifecycle = AppLifecycle::new(event_bus, show.clone(), show_peers);
        let path = temp_show_file_path("save-existing");
        let (reply, rx) = oneshot::channel();
        show.send(ShowCommand::SaveShowFileAs {
            path: path.clone(),
            reply: Some(reply),
        })
        .await
        .unwrap();
        let _ = rx
            .await
            .expect("save should reply")
            .expect("save should set current show file path");

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
        .send(SceneRecallCommand::RecallScene { scene_id, reply })
        .await
        .map_err(|_| AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
        .map_err(map_app_command_error)
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
