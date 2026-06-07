use lv1_scene_fade_utility::fade::engine::spawn_engine;
use lv1_scene_fade_utility::lv1::discovery::resolve_target;
use lv1_scene_fade_utility::lv1::messages::Lv1Event;
use lv1_scene_fade_utility::lv1::state::spawn_actor;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::task::spawn_blocking;

use crate::app_state::{AppViewState, ShellState};
use crate::show_file::{backup_folder, default_show_folder, read_show_file, write_show_file};

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
pub async fn set_scene_fade_enabled(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
    enabled: bool,
) -> Result<AppViewState, String> {
    let snapshot = state.set_scene_fade_enabled(scene_id, enabled).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_listen_mode(
    app: AppHandle,
    state: State<'_, ShellState>,
    active: bool,
) -> Result<AppViewState, String> {
    let snapshot = state.set_listen_mode(active).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_fade_target_enabled(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
    group: i32,
    channel: i32,
    enabled: bool,
) -> Result<AppViewState, String> {
    let snapshot = state
        .set_fade_target_enabled(scene_id, group, channel, enabled)
        .await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn remove_fade_target(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
    group: i32,
    channel: i32,
) -> Result<AppViewState, String> {
    let snapshot = state.remove_fade_target(&scene_id, group, channel).await?;
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
) -> Result<AppViewState, String> {
    {
        let mut handles = state.handles.lock().await;
        handles.lv1 = None;
        handles.fade = None;
    }

    let snapshot = state.disconnect().await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn abort_all_fades(state: State<'_, ShellState>) -> Result<(), String> {
    let fade = { state.handles.lock().await.fade.clone() };
    if let Some(fade) = fade {
        fade.abort_all().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn finish_fade_now(state: State<'_, ShellState>) -> Result<(), String> {
    let fade = { state.handles.lock().await.fade.clone() };
    if let Some(fade) = fade {
        fade.finish_now().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn connect_lv1(
    app: AppHandle,
    state: State<'_, ShellState>,
    host: Option<String>,
    port: Option<u16>,
    timeout_ms: Option<u64>,
) -> Result<AppViewState, String> {
    let timeout = timeout_ms.unwrap_or(6000);
    let (host, port) = resolve_target(host, port, timeout).map_err(|err| err.to_string())?;

    let lv1 = spawn_actor(host.clone(), port);
    let fade = spawn_engine(lv1.clone());

    {
        let mut handles = state.handles.lock().await;
        handles.lv1 = Some(lv1.clone());
        handles.fade = Some(fade);
    }

    let (generation, connecting_snapshot) = state.begin_connecting().await;
    emit_snapshot(&app, &connecting_snapshot);

    let mut events = lv1.subscribe().await;
    let initial_snapshot = lv1.get_state().await;
    let snapshot = state.begin_connection(initial_snapshot).await;
    emit_snapshot(&app, &snapshot);

    let app_for_task = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            let state_for_task = app_for_task.state::<ShellState>();
            if let Some(snapshot) = state_for_task
                .apply_lv1_event_for_generation(generation, &event)
                .await
            {
                if let Err(err) = app_for_task.emit("lv1-event", &Lv1EventPayload::from(&event)) {
                    eprintln!("failed to emit lv1-event: {err}");
                }
                if let Err(err) = app_for_task.emit("app-status-changed", &snapshot) {
                    eprintln!("failed to emit app-status-changed: {err}");
                }
            }
        }
    });

    Ok(snapshot)
}

fn emit_snapshot(app: &AppHandle, snapshot: &AppViewState) {
    if let Err(err) = app.emit("app-status-changed", snapshot) {
        eprintln!("failed to emit app-status-changed: {err}");
    }
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
