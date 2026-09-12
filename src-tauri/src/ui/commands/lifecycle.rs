use crate::connection_state::Lv1SystemIdentity;
use crate::lifecycle::AppLifecycle;
use crate::show::{ConnectCommandResult, ShowCommandResult};
use crate::ui::UiLogReceiverState;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

/// @cc [owner:mixxorz,label:architecture] frontend-ready-supplies-log-subscription
/// The frontend-ready adapter MUST obtain a fresh UI-log receiver from managed logging state and
/// delegate projector startup and idempotence to Lifecycle; it MUST NOT emit snapshots or start a
/// projector itself.
#[tauri::command]
pub async fn frontend_ready<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, AppLifecycle>,
) -> Result<(), String> {
    let logs = app.state::<UiLogReceiverState>().subscribe();
    let mut snapshots = lifecycle.frontend_ready(logs).await?;
    tokio::spawn(async move {
        loop {
            if let Some(snapshot) = snapshots.latest()
                && let Err(error) = app.emit("app-status-changed", snapshot)
            {
                tracing::debug!(
                    event = "projector_emit_failed",
                    error = %error,
                    "Failed to emit app-status-changed from Tauri projection bridge"
                );
            }
            if snapshots.changed().await.is_err() {
                break;
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn connect_lv1_system(
    lifecycle: State<'_, AppLifecycle>,
    identity: Lv1SystemIdentity,
) -> Result<ConnectCommandResult, String> {
    lifecycle.connect_lv1_system(identity).await
}

#[tauri::command]
pub async fn startup_auto_connect_lv1(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ConnectCommandResult, String> {
    lifecycle.startup_auto_connect_lv1().await
}

#[tauri::command]
pub async fn probe_lv1_tcp_connect_latency(
    identity: Lv1SystemIdentity,
    timeout_ms: Option<u64>,
) -> Result<crate::lv1::TcpConnectProbeResult, String> {
    crate::lv1::probe_tcp_connect_latency(&identity.address, identity.port, timeout_ms).await
}

#[tauri::command]
pub async fn disconnect_lv1(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<ShowCommandResult, String> {
    lifecycle.disconnect_current_runtime().await
}
