use lv1_scene_fade_utility::fade::engine::spawn_engine;
use lv1_scene_fade_utility::lv1::discovery::resolve_target;
use lv1_scene_fade_utility::lv1::state::Lv1Event;
use lv1_scene_fade_utility::lv1::state::spawn_actor;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_state::{AppViewState, ShellState};

#[tauri::command]
pub async fn get_app_status(state: State<'_, ShellState>) -> Result<AppViewState, String> {
    Ok(state.snapshot().await)
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

#[derive(Debug, Clone, Serialize)]
struct Lv1EventPayload {
    kind: String,
    message: String,
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
