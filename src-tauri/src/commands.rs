use lv1_scene_fade_utility::fade::engine::spawn_engine;
use lv1_scene_fade_utility::lv1::discovery::resolve_target;
use lv1_scene_fade_utility::lv1::state::spawn_actor;
use tauri::{AppHandle, Emitter, State};

use crate::app_state::{AppSnapshot, ShellState};

#[tauri::command]
pub async fn get_app_status(state: State<'_, ShellState>) -> Result<AppSnapshot, String> {
    Ok(state.snapshot().await)
}

#[tauri::command]
pub async fn set_lockout(
    app: AppHandle,
    state: State<'_, ShellState>,
    enabled: bool,
) -> Result<AppSnapshot, String> {
    let snapshot = state.set_lockout(enabled).await;
    emit_snapshot(&app, &snapshot)?;
    Ok(snapshot)
}

#[tauri::command]
pub async fn disconnect_lv1(app: AppHandle, state: State<'_, ShellState>) -> Result<AppSnapshot, String> {
    {
        let mut handles = state.handles.lock().await;
        handles.lv1 = None;
        handles.fade = None;
    }

    let snapshot = state.clear_lv1_snapshot().await;
    emit_snapshot(&app, &snapshot)?;
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
) -> Result<AppSnapshot, String> {
    let timeout = timeout_ms.unwrap_or(6000);
    let (host, port) = resolve_target(host, port, timeout).map_err(|err| err.to_string())?;

    let lv1 = spawn_actor(host.clone(), port);
    let fade = spawn_engine(lv1.clone());
    let initial_snapshot = lv1.get_state().await;

    {
        let mut handles = state.handles.lock().await;
        handles.lv1 = Some(lv1.clone());
        handles.fade = Some(fade);
    }

    let snapshot = state.replace_lv1_snapshot(initial_snapshot).await;
    emit_snapshot(&app, &snapshot)?;

    let mut events = lv1.subscribe().await;
    let app_for_task = app.clone();
    let state_for_task = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            let snapshot = state_for_task.apply_lv1_event(&event).await;
            let _ = app_for_task.emit("lv1-event", format!("{:?}", event));
            let _ = app_for_task.emit("app-status-changed", &snapshot);
        }
    });

    Ok(snapshot)
}

fn emit_snapshot(app: &AppHandle, snapshot: &AppSnapshot) -> Result<(), String> {
    app.emit("app-status-changed", snapshot)
        .map_err(|err| err.to_string())
}
