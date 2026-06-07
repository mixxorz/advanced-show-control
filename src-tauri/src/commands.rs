use lv1_scene_fade_utility::fade::engine::spawn_engine;
use lv1_scene_fade_utility::lv1::discovery::resolve_target;
use lv1_scene_fade_utility::lv1::messages::Lv1Event;
use lv1_scene_fade_utility::lv1::state::spawn_actor;
use lv1_scene_fade_utility::runtime::commands::AppCommandBus;
use lv1_scene_fade_utility::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::task::spawn_blocking;
use tokio::sync::Mutex;

use crate::app_state::{AppViewState, RuntimeHandles, ShellState};
use crate::show_file::{backup_folder, default_show_folder, read_show_file, write_show_file};

#[derive(Clone, Default)]
pub struct ActiveCommandBus(pub Arc<Mutex<Option<AppCommandBus>>>);

impl ActiveCommandBus {
    pub async fn set(&self, command_bus: Option<AppCommandBus>) {
        *self.0.lock().await = command_bus;
    }

    pub async fn current(&self) -> Option<AppCommandBus> {
        self.0.lock().await.clone()
    }
}

#[tauri::command]
pub async fn get_app_status(state: State<'_, ShellState>) -> Result<AppViewState, String> {
    Ok(state.snapshot().await)
}

#[tauri::command]
pub async fn new_show_file(
    app: AppHandle,
    state: State<'_, ShellState>,
) -> Result<AppViewState, String> {
    let snapshot = state.new_show_file().await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn open_show_file_dialog(
    app: AppHandle,
    state: State<'_, ShellState>,
) -> Result<AppViewState, String> {
    let path = spawn_blocking(|| -> Result<Option<std::path::PathBuf>, String> {
        let folder = default_show_folder();
        let folder = ensure_show_file_folder(folder)?;
        Ok(rfd::FileDialog::new()
            .set_directory(folder)
            .add_filter("LV1 Show", &["lv1show"])
            .pick_file())
    })
    .await
    .map_err(|err| format!("Failed to open file dialog: {err}"))??
    .ok_or_else(|| "Open show file cancelled".to_string())?;

    let mut file = read_show_file(&path)?;
    let snapshot = state.load_show_file_from_dto(path, &mut file).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_scene_duration_ms(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
    duration_ms: u64,
) -> Result<AppViewState, String> {
    let snapshot = state.set_scene_duration_ms(scene_id, duration_ms).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn select_scene_config(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
) -> Result<AppViewState, String> {
    let snapshot = state.select_scene_config(scene_id).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn store_scene_config(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
) -> Result<AppViewState, String> {
    let snapshot = state.store_scene_config(scene_id).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_channel_scoped(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
    group: i32,
    channel: i32,
    scoped: bool,
) -> Result<AppViewState, String> {
    let snapshot = state
        .set_channel_scoped(scene_id, group, channel, scoped)
        .await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_all_channels_scoped(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
    scoped: bool,
) -> Result<AppViewState, String> {
    let snapshot = state.set_all_channels_scoped(scene_id, scoped).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn save_show_file(
    app: AppHandle,
    state: State<'_, ShellState>,
) -> Result<AppViewState, String> {
    if let Some(path) = state.current_show_file_path().await {
        let snapshot = save_show_file_to_path(&state, path).await?;
        emit_snapshot(&app, &snapshot);
        return Ok(snapshot);
    }

    save_show_file_as_dialog(app, state).await
}

#[tauri::command]
pub async fn save_show_file_as_dialog(
    app: AppHandle,
    state: State<'_, ShellState>,
) -> Result<AppViewState, String> {
    let _ = state.export_show_file_for_save(String::new()).await?;

    let path = spawn_blocking(|| -> Result<Option<std::path::PathBuf>, String> {
        let folder = default_show_folder();
        let folder = ensure_show_file_folder(folder)?;
        Ok(rfd::FileDialog::new()
            .set_directory(folder)
            .set_file_name("Untitled.lv1show")
            .add_filter("LV1 Show", &["lv1show"])
            .save_file())
    })
    .await
    .map_err(|err| format!("Failed to open save dialog: {err}"))??
    .ok_or_else(|| "Save show file cancelled".to_string())?;

    let snapshot = save_show_file_to_path(&state, path).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_lockout(
    app: AppHandle,
    state: State<'_, ShellState>,
    enabled: bool,
) -> Result<AppViewState, String> {
    let snapshot = state.set_lockout(enabled).await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn disconnect_lv1(
    app: AppHandle,
    state: State<'_, ShellState>,
    active_command_bus: State<'_, ActiveCommandBus>,
) -> Result<AppViewState, String> {
    let (generation, snapshot) = state.disconnect().await;
    state
        .clear_runtime_handles_for_generation(generation, &active_command_bus)
        .await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn abort_all_fades(active_command_bus: State<'_, ActiveCommandBus>) -> Result<(), String> {
    let command_bus = active_command_bus.current().await;
    let command_bus = command_bus.ok_or_else(|| "Fade runtime is unavailable".to_string())?;
    command_bus.abort_all_fades().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn finish_fade_now(active_command_bus: State<'_, ActiveCommandBus>) -> Result<(), String> {
    let command_bus = active_command_bus.current().await;
    let command_bus = command_bus.ok_or_else(|| "Fade runtime is unavailable".to_string())?;
    command_bus.finish_fade_now().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn connect_lv1(
    app: AppHandle,
    state: State<'_, ShellState>,
    active_command_bus: State<'_, ActiveCommandBus>,
    host: Option<String>,
    port: Option<u16>,
    timeout_ms: Option<u64>,
) -> Result<AppViewState, String> {
    let timeout = timeout_ms.unwrap_or(6000);
    let (host, port) = resolve_target(host, port, timeout).map_err(|err| err.to_string())?;
    let event_bus = AppEventBus::default();
    let (generation, connecting_snapshot) = state.begin_connecting().await;
    emit_snapshot(&app, &connecting_snapshot);
    let events = event_bus.subscribe();

    let shell_state = (*state).clone();

    let lv1 = spawn_actor(host.clone(), port, event_bus.clone());
    let command_bus = AppCommandBus::new(event_bus.clone());
    command_bus.set_lv1(Some(lv1.clone())).await;
    let fade_command_bus = command_bus.clone();
    let fade = spawn_engine(command_bus, event_bus.clone());
    fade_command_bus.set_fade(Some(fade.clone())).await;
    let projector_task = spawn_shell_state_projector(app.clone(), shell_state, generation, events);

    let runtime_handles = RuntimeHandles {
        active_generation: 0,
        lv1: Some(lv1.clone()),
        fade: Some(fade),
        command_bus: Some(fade_command_bus),
        projector: Some(projector_task),
    };

    if let Err(mut stale_handles) = state
        .install_runtime_handles_for_generation(generation, runtime_handles, &active_command_bus)
        .await
    {
        stale_handles.abort_all();
        let snapshot = state.snapshot().await;
        emit_snapshot(&app, &snapshot);
        return Ok(snapshot);
    }

    let initial_snapshot = lv1.get_state().await;
    let snapshot = match state
        .begin_connection_for_generation(generation, initial_snapshot)
        .await
    {
        Some(snapshot) => snapshot,
        None => state.snapshot().await,
    };
    emit_snapshot(&app, &snapshot);

    Ok(snapshot)
}

fn emit_snapshot(app: &AppHandle, snapshot: &AppViewState) {
    if let Err(err) = app.emit("app-status-changed", snapshot) {
        eprintln!("failed to emit app-status-changed: {err}");
    }
}

fn spawn_shell_state_projector(
    app: AppHandle,
    state: ShellState,
    generation: u64,
    mut events: tokio::sync::broadcast::Receiver<AppEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(app_event) => match app_event {
                    AppEvent::Lv1(event) => {
                        if let Some(snapshot) = state
                            .apply_lv1_event_for_generation(generation, &event)
                            .await
                        {
                            if let Err(err) = app.emit("lv1-event", &Lv1EventPayload::from(&event))
                            {
                                eprintln!("failed to emit lv1-event: {err}");
                            }
                            if let Err(err) = app.emit("app-status-changed", &snapshot) {
                                eprintln!("failed to emit app-status-changed: {err}");
                            }
                        }
                    }
                    AppEvent::Fade(event) => {
                        let snapshot = state.apply_fade_event(&event).await;
                        if let Err(err) = app.emit("app-status-changed", &snapshot) {
                            eprintln!("failed to emit app-status-changed: {err}");
                        }
                    }
                    AppEvent::CommandFailed { command, message } => {
                        eprintln!("command failed: {command}: {message}");
                    }
                    AppEvent::Automation(_) => {}
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                    log_lagged_subscriber("shell-state-projector", count);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

async fn save_show_file_to_path(
    state: &State<'_, ShellState>,
    path: PathBuf,
) -> Result<AppViewState, String> {
    let saved_at = current_timestamp_millis();
    let file = state.export_show_file_for_save(saved_at.clone()).await?;
    write_show_file(&path, &file, &backup_folder())?;
    Ok(state.mark_show_file_saved(path, saved_at).await)
}

fn ensure_show_file_folder(path: std::path::PathBuf) -> Result<std::path::PathBuf, String> {
    std::fs::create_dir_all(&path)
        .map_err(|err| format!("Failed to create show file folder: {err}"))?;
    Ok(path)
}

#[derive(Debug, Clone, Serialize)]
struct Lv1EventPayload {
    kind: String,
    message: String,
}

fn current_timestamp_millis() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "lv1-scene-fade-utility-commands-{}-{}-{}",
            name,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn ensure_show_file_folder_creates_missing_directory() {
        let folder = temp_dir("show-folder").join("LV1 Scene Fade Utility");

        let created = ensure_show_file_folder(folder.clone()).unwrap();

        assert_eq!(created, folder);
        assert!(folder.exists());

        let _ = fs::remove_dir_all(created.parent().unwrap());
    }

    #[test]
    fn scene_store_commands_are_exposed() {
        let _ = store_scene_config;
        let _ = set_channel_scoped;
        let _ = set_all_channels_scoped;
    }

    #[tokio::test]
    async fn active_command_bus_tracks_current_bus() {
        let holder = ActiveCommandBus::default();
        assert!(holder.current().await.is_none());

        let bus = AppCommandBus::new(AppEventBus::default());
        holder.set(Some(bus.clone())).await;

        assert!(holder.current().await.is_some());

        holder.set(None).await;
        assert!(holder.current().await.is_none());
    }
}

impl From<&Lv1Event> for Lv1EventPayload {
    fn from(event: &Lv1Event) -> Self {
        match event {
            Lv1Event::Connected => Self {
                kind: "Connected".to_string(),
                message: "LV1 connected".to_string(),
            },
            Lv1Event::Disconnected => Self {
                kind: "Disconnected".to_string(),
                message: "LV1 disconnected".to_string(),
            },
            Lv1Event::SceneChanged(scene) => Self {
                kind: "SceneChanged".to_string(),
                message: format!("scene changed to {}: {}", scene.index, scene.name),
            },
            Lv1Event::SceneListChanged(scenes) => Self {
                kind: "SceneListChanged".to_string(),
                message: format!("scene list updated: {} scenes", scenes.len()),
            },
            Lv1Event::FaderChanged {
                group,
                channel,
                gain_db,
            } => Self {
                kind: "FaderChanged".to_string(),
                message: format!("fader changed: group {group}, channel {channel}, gain {gain_db}"),
            },
            Lv1Event::MuteChanged {
                group,
                channel,
                muted,
            } => Self {
                kind: "MuteChanged".to_string(),
                message: format!("mute changed: group {group}, channel {channel}, muted {muted}"),
            },
            Lv1Event::ChannelTopologyChanged(channels) => Self {
                kind: "ChannelTopologyChanged".to_string(),
                message: format!("channel topology updated: {} channels", channels.len()),
            },
        }
    }
}
