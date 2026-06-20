use crate::connection_state::Lv1SystemIdentity;
use crate::lifecycle::AppLifecycle;
use crate::show::{ConnectCommandResult, ShowCommandResult};
use crate::ui::UiLogReceiverState;
use tauri::{AppHandle, Manager, Runtime, State};

#[tauri::command]
pub async fn frontend_ready<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, AppLifecycle>,
) -> Result<(), String> {
    let logs = app.state::<UiLogReceiverState>().subscribe();
    lifecycle.frontend_ready(app, logs).await
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
