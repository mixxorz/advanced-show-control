use crate::fade::actor::spawn_engine;
use crate::lifecycle::{ActiveCommandBus, AppLifecycle};
use crate::lv1::actor::spawn_actor;
use crate::lv1::discovery::resolve_target;
use crate::projector::{ProjectorInputs, spawn_projector};
use crate::runtime::commands::AppCommandBus;
use crate::runtime::events::{AppEvent, AppEventBus};
use crate::scene_recall::spawn_scene_recall_fader;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tokio::task::spawn_blocking;

use crate::app_state::{AppConnectionState, AppViewState, RuntimeHandles, ShellState};
use crate::show_file::{backup_folder, default_show_folder, read_show_file, write_show_file};
use crate::ui::UiLogReceiverState;

const DEFAULT_DISCOVERY_TIMEOUT_MS: u64 = 1000;
const MIN_DISCOVERY_TIMEOUT_MS: u64 = 100;
const MAX_DISCOVERY_TIMEOUT_MS: u64 = 6000;

#[tauri::command]
pub async fn get_app_status(state: State<'_, ShellState>) -> Result<AppViewState, String> {
    Ok(state.snapshot().await)
}

#[tauri::command]
pub async fn refresh_lv1_discovery(
    app: AppHandle,
    state: State<'_, ShellState>,
    timeout_ms: Option<u64>,
) -> Result<AppViewState, String> {
    refresh_lv1_discovery_snapshot(app, (*state).clone(), timeout_ms).await
}

async fn refresh_lv1_discovery_snapshot<R: Runtime>(
    app: AppHandle<R>,
    state: ShellState,
    timeout_ms: Option<u64>,
) -> Result<AppViewState, String> {
    let started = std::time::Instant::now();
    let timeout = timeout_ms
        .unwrap_or(DEFAULT_DISCOVERY_TIMEOUT_MS)
        .clamp(MIN_DISCOVERY_TIMEOUT_MS, MAX_DISCOVERY_TIMEOUT_MS);
    let entries = spawn_blocking(move || {
        crate::lv1::discovery::discover(crate::lv1::discovery::DiscoverOptions {
            timeout: std::time::Duration::from_millis(timeout),
            ..Default::default()
        })
    })
    .await
    .map_err(|err| format!("Failed to run LV1 discovery task: {err}"))?
    .map_err(|err| format!("Failed to discover LV1 systems: {err}"))?;

    let latency_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    let systems = entries
        .iter()
        .filter_map(crate::connection_state::identity_from_discovery)
        .map(|identity| crate::connection_state::DiscoveredLv1System {
            identity,
            latency_ms: Some(latency_ms),
            status: crate::connection_state::DiscoveredLv1Status::Available,
        })
        .collect();
    let snapshot = state.set_discovered_lv1_systems(systems).await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

async fn resolve_connect_target(
    host: Option<String>,
    port: Option<u16>,
    timeout: u64,
) -> Result<(String, u16), String> {
    spawn_blocking(move || resolve_target(host, port, timeout).map_err(|err| err.to_string()))
        .await
        .map_err(|err| format!("Failed to resolve LV1 target: {err}"))?
}

async fn current_command_bus(
    active_command_bus: ActiveCommandBus,
    command_name: &'static str,
) -> Result<AppCommandBus, String> {
    active_command_bus.current().await.ok_or_else(|| {
        tracing::warn!(
            event = "command_blocked",
            command = command_name,
            reason = "app command bus is unavailable",
            "Command blocked: app command bus is unavailable"
        );
        "App command bus is unavailable".to_string()
    })
}

fn map_app_command_error(error: crate::runtime::commands::AppCommandError) -> String {
    match error {
        crate::runtime::commands::AppCommandError::CommandFailed(message) => message,
        other => other.to_string(),
    }
}

#[tauri::command]
pub async fn new_show_file(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
) -> Result<AppViewState, String> {
    let command_bus = current_command_bus(lifecycle.command_bus_holder(), "new_show_file").await?;
    let lv1 = state.lv1_snapshot().await;
    let result = command_bus
        .new_show_file(lv1)
        .await
        .map_err(map_app_command_error)?;
    let snapshot = state
        .apply_new_show_file_metadata(result.selected_scene_id)
        .await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn open_show_file_dialog(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
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
    let command_bus =
        current_command_bus(lifecycle.command_bus_holder(), "open_show_file_dialog").await?;
    let lv1 = state.lv1_snapshot_required_for_show_file().await?;
    let result = command_bus
        .load_show_file_from_dto(&mut file, lv1)
        .await
        .map_err(map_app_command_error)?;
    for scene in result.report.removed_scenes.iter() {
        tracing::warn!(
            event = "show_file_scene_pruned",
            scene = %scene,
            "Skipped loading \"{scene}\" because it was not found in the current scene list."
        );
    }
    let snapshot = state
        .apply_loaded_show_file_metadata(
            path,
            result.selected_scene_id,
            result.saved_at,
            result.report.removed_anything(),
        )
        .await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_scene_duration_ms(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    duration_ms: u64,
) -> Result<AppViewState, String> {
    let command_bus =
        current_command_bus(lifecycle.command_bus_holder(), "set_scene_duration_ms").await?;
    let result = command_bus
        .set_scene_duration_ms(scene_id, duration_ms)
        .await
        .map_err(map_app_command_error)?;
    if result.changed {
        state.mark_show_file_dirty().await;
    }
    let snapshot = state.snapshot().await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn select_scene_config(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
) -> Result<AppViewState, String> {
    let command_bus =
        current_command_bus(lifecycle.command_bus_holder(), "select_scene_config").await?;
    command_bus
        .select_scene_config(scene_id.clone())
        .await
        .map_err(map_app_command_error)?;
    let snapshot = state.select_scene_config(scene_id).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn cue_scene(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
) -> Result<AppViewState, String> {
    let snapshot =
        cue_scene_snapshot((*state).clone(), lifecycle.command_bus_holder(), scene_id).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

async fn cue_scene_snapshot(
    state: ShellState,
    active_command_bus: ActiveCommandBus,
    scene_id: String,
) -> Result<AppViewState, String> {
    tracing::debug!(
        event = "scene_cue_requested",
        scene_id = %scene_id,
        "Scene cue requested"
    );

    let command_bus = current_command_bus(active_command_bus, "cue_scene").await?;
    let result = command_bus
        .cue_scene(scene_id.clone())
        .await
        .map_err(|error| {
            if matches!(
                error,
                crate::runtime::commands::AppCommandError::CommandFailed(_)
            ) {
                tracing::warn!(
                    event = "scene_cue_blocked",
                    scene_id = %scene_id,
                    reason = "scene config not found",
                    "Scene cue blocked: scene config not found"
                );
            }
            map_app_command_error(error)
        })?;

    tracing::info!(
        event = "scene_cued",
        scene_id = %result.scene.scene_id,
        scene_index = result.scene.scene_index,
        scene_name = %result.scene.scene_name,
        "Scene cued: {}",
        result.scene.scene_name
    );

    Ok(state.snapshot().await)
}

#[tauri::command]
pub async fn store_scene_config(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
) -> Result<AppViewState, String> {
    let command_bus =
        current_command_bus(lifecycle.command_bus_holder(), "store_scene_config").await?;
    let lv1 = state
        .lv1_snapshot()
        .await
        .ok_or_else(|| "Open a show file after LV1 scenes are loaded".to_string())?;
    let result = command_bus
        .store_scene_config(scene_id, lv1.channels)
        .await
        .map_err(map_app_command_error)?;
    if result.changed {
        state.mark_show_file_dirty().await;
    }
    let snapshot = state.snapshot().await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn recall_scene(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
) -> Result<AppViewState, String> {
    let snapshot =
        recall_scene_snapshot((*state).clone(), lifecycle.command_bus_holder(), scene_id).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

async fn recall_scene_snapshot(
    state: ShellState,
    active_command_bus: ActiveCommandBus,
    scene_id: String,
) -> Result<AppViewState, String> {
    tracing::debug!(
        event = "scene_recall_requested",
        scene_id = %scene_id,
        "Scene recall requested"
    );

    let command_bus = current_command_bus(active_command_bus, "recall_scene").await?;
    let result = command_bus
        .recall_scene_by_id(scene_id.clone())
        .await
        .map_err(|error| {
            let message = map_app_command_error(error);
            tracing::warn!(
                event = "scene_recall_blocked",
                scene_id = %scene_id,
                reason = %message,
                "Scene recall blocked: {message}"
            );
            message
        })?;

    tracing::debug!(
        event = "scene_recall_command_sent",
        scene_id = %result.scene.scene_id,
        scene_index = result.scene.scene_index,
        scene_name = %result.scene.scene_name,
        "Scene recall command sent: {}",
        result.scene.scene_name
    );

    Ok(state.snapshot().await)
}

#[tauri::command]
pub async fn set_channel_scoped(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    group: i32,
    channel: i32,
    scoped: bool,
) -> Result<AppViewState, String> {
    let command_bus =
        current_command_bus(lifecycle.command_bus_holder(), "set_channel_scoped").await?;
    let result = command_bus
        .set_channel_scoped(scene_id, group, channel, scoped)
        .await
        .map_err(map_app_command_error)?;
    if result.changed {
        state.mark_show_file_dirty().await;
    }
    let snapshot = state.snapshot().await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_all_channels_scoped(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    scoped: bool,
) -> Result<AppViewState, String> {
    let command_bus =
        current_command_bus(lifecycle.command_bus_holder(), "set_all_channels_scoped").await?;
    let result = command_bus
        .set_all_channels_scoped(scene_id, scoped)
        .await
        .map_err(map_app_command_error)?;
    if result.changed {
        state.mark_show_file_dirty().await;
    }
    let snapshot = state.snapshot().await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_scene_scope_faders_enabled(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    enabled: bool,
) -> Result<AppViewState, String> {
    let command_bus = current_command_bus(
        lifecycle.command_bus_holder(),
        "set_scene_scope_faders_enabled",
    )
    .await?;
    let result = command_bus
        .set_scene_scope_faders_enabled(scene_id, enabled)
        .await
        .map_err(map_app_command_error)?;
    if result.changed {
        state.mark_show_file_dirty().await;
    }
    let snapshot = state.snapshot().await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_scene_scope_pan_enabled(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    scene_id: String,
    enabled: bool,
) -> Result<AppViewState, String> {
    let command_bus = current_command_bus(
        lifecycle.command_bus_holder(),
        "set_scene_scope_pan_enabled",
    )
    .await?;
    let result = command_bus
        .set_scene_scope_pan_enabled(scene_id, enabled)
        .await
        .map_err(map_app_command_error)?;
    if result.changed {
        state.mark_show_file_dirty().await;
    }
    let snapshot = state.snapshot().await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn save_show_file(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
) -> Result<AppViewState, String> {
    if let Some(path) = state.current_show_file_path().await {
        let snapshot = save_show_file_to_path(&state, lifecycle.command_bus_holder(), path).await?;
        emit_snapshot(&app, &snapshot);
        return Ok(snapshot);
    }

    save_show_file_as_dialog(app, state, lifecycle).await
}

#[tauri::command]
pub async fn save_show_file_as_dialog(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
) -> Result<AppViewState, String> {
    let command_bus =
        current_command_bus(lifecycle.command_bus_holder(), "save_show_file_as_dialog").await?;
    command_bus
        .export_show_file_for_save(String::new())
        .await
        .map_err(map_app_command_error)?;

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

    let snapshot = save_show_file_to_path(&state, lifecycle.command_bus_holder(), path).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_lockout(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    enabled: bool,
) -> Result<AppViewState, String> {
    let snapshot =
        set_lockout_snapshot((*state).clone(), lifecycle.command_bus_holder(), enabled).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

async fn set_lockout_snapshot(
    state: ShellState,
    active_command_bus: ActiveCommandBus,
    enabled: bool,
) -> Result<AppViewState, String> {
    let command_bus = current_command_bus(active_command_bus, "set_lockout").await?;
    let result = command_bus
        .set_lockout(enabled)
        .await
        .map_err(map_app_command_error)?;
    if result.changed {
        state.mark_show_file_dirty().await;
    }
    Ok(state.snapshot().await)
}

#[tauri::command]
pub async fn disconnect_lv1(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
) -> Result<AppViewState, String> {
    tracing::debug!(
        event = "lv1_disconnect_requested",
        "LV1 disconnect requested"
    );
    let (generation, snapshot) = state.disconnect().await;
    let active_command_bus = lifecycle.command_bus_holder();
    if let Some(command_bus) = active_command_bus.current().await {
        command_bus.set_generation(generation).await;
    }
    lifecycle.clear_runtime_handles(&state, generation).await;
    tracing::info!(event = "lv1_disconnected", "Disconnected from LV1");
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn reconnect_timed_out(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    attempt: u64,
) -> Result<AppViewState, String> {
    reconnect_timed_out_snapshot(app, (*state).clone(), &lifecycle, attempt).await
}

async fn reconnect_timed_out_snapshot<R: Runtime>(
    app: AppHandle<R>,
    state: ShellState,
    lifecycle: &AppLifecycle,
    attempt: u64,
) -> Result<AppViewState, String> {
    if let Some(generation) = state.reconnect_timeout_generation(attempt).await {
        lifecycle.clear_runtime_handles(&state, generation).await;
    }
    let snapshot = state.reconnect_timed_out(attempt).await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn abort_all_fades(lifecycle: State<'_, AppLifecycle>) -> Result<(), String> {
    let command_bus = lifecycle.current_command_bus().await;
    let command_bus = command_bus.ok_or_else(|| "Fade runtime is unavailable".to_string())?;
    command_bus
        .abort_all_fades()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn connect_lv1(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    host: Option<String>,
    port: Option<u16>,
    timeout_ms: Option<u64>,
) -> Result<AppViewState, String> {
    let timeout = timeout_ms.unwrap_or(6000);
    let (host, port) = resolve_connect_target(host, port, timeout).await?;
    let identity = crate::connection_state::Lv1SystemIdentity {
        uuid: None,
        host: None,
        address: host,
        port,
    };

    connect_to_target(
        app,
        (*state).clone(),
        &lifecycle,
        identity,
        ConnectFailureMode::ClearConnectedIdentity,
    )
    .await
}

#[tauri::command]
pub async fn connect_lv1_system(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
    identity: crate::connection_state::Lv1SystemIdentity,
) -> Result<AppViewState, String> {
    let snapshot = connect_to_target(
        app.clone(),
        (*state).clone(),
        &lifecycle,
        identity.clone(),
        ConnectFailureMode::ClearConnectedIdentity,
    )
    .await?;

    if should_save_connection_preferences(&snapshot, &identity) {
        let preferences = crate::connection_preferences::ConnectionPreferences {
            last_connected_lv1: Some(crate::connection_preferences::LastConnectedLv1 {
                uuid: identity.uuid.clone(),
                host: identity.host.clone(),
                address: identity.address.clone(),
                port: identity.port,
            }),
        };
        let preferences_path = app
            .path()
            .app_config_dir()
            .map_err(|err| format!("Failed to resolve app config dir: {err}"))?
            .join("preferences.json");
        crate::connection_preferences::write_connection_preferences(
            &preferences_path,
            &preferences,
        )?;
    }

    Ok(snapshot)
}

#[tauri::command]
pub async fn attempt_reconnect_lv1(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
) -> Result<AppViewState, String> {
    attempt_reconnect_lv1_snapshot(app, (*state).clone(), &lifecycle).await
}

async fn attempt_reconnect_lv1_snapshot<R: Runtime>(
    app: AppHandle<R>,
    state: ShellState,
    lifecycle: &AppLifecycle,
) -> Result<AppViewState, String> {
    let Some(connected_identity) = state.connected_lv1_identity().await else {
        let snapshot = state.snapshot().await;
        emit_snapshot(&app, &snapshot);
        return Ok(snapshot);
    };

    if connected_identity.uuid.is_none() {
        let snapshot = state.snapshot().await;
        emit_snapshot(&app, &snapshot);
        return Ok(snapshot);
    }

    let snapshot = startup_discovery_or_current_snapshot(
        &state,
        refresh_lv1_discovery_snapshot(
            app.clone(),
            state.clone(),
            Some(DEFAULT_DISCOVERY_TIMEOUT_MS),
        )
        .await,
    )
    .await;

    let Some(identity) = reconnect_target_for_connected_identity(
        &connected_identity,
        &snapshot.discovered_lv1_systems,
    ) else {
        return Ok(snapshot);
    };

    connect_to_target(
        app,
        state,
        lifecycle,
        identity,
        ConnectFailureMode::PreserveConnectedIdentity,
    )
    .await
}

#[derive(Clone, Copy)]
enum ConnectFailureMode {
    ClearConnectedIdentity,
    PreserveConnectedIdentity,
}

#[tauri::command]
pub async fn startup_auto_connect_lv1(
    app: AppHandle,
    state: State<'_, ShellState>,
    lifecycle: State<'_, AppLifecycle>,
) -> Result<AppViewState, String> {
    let preferences_path = app
        .path()
        .app_config_dir()
        .map_err(|err| format!("Failed to resolve app config dir: {err}"))?
        .join("preferences.json");
    let preferences =
        crate::connection_preferences::read_connection_preferences(&preferences_path)?;
    let snapshot = startup_discovery_or_current_snapshot(
        &state,
        refresh_lv1_discovery_snapshot(
            app.clone(),
            (*state).clone(),
            Some(DEFAULT_DISCOVERY_TIMEOUT_MS),
        )
        .await,
    )
    .await;
    if let Some(identity) =
        remembered_auto_connect_target(&preferences, &snapshot.discovered_lv1_systems)
    {
        return connect_lv1_system(app, state, lifecycle, identity).await;
    }
    Ok(snapshot)
}

async fn connect_to_target<R: Runtime>(
    app: AppHandle<R>,
    state: ShellState,
    lifecycle: &AppLifecycle,
    identity: crate::connection_state::Lv1SystemIdentity,
    failure_mode: ConnectFailureMode,
) -> Result<AppViewState, String> {
    let event_bus = AppEventBus::default();
    tracing::debug!(
        event = "lv1_connect_requested",
        host = %identity.address,
        port = identity.port,
        "LV1 connect requested"
    );
    let Some((generation, connecting_snapshot)) = state.try_begin_connecting().await else {
        let snapshot = state.snapshot().await;
        emit_snapshot(&app, &snapshot);
        return Ok(snapshot);
    };
    tracing::info!(
        event = "lv1_connecting",
        host = %identity.address,
        port = identity.port,
        "Connecting to LV1"
    );
    lifecycle.abort_current_runtime(&state).await;
    emit_snapshot(&app, &connecting_snapshot);
    if let Some(pending_snapshot) = state
        .set_pending_lv1_identity(generation, Some(identity.clone()))
        .await
    {
        emit_snapshot(&app, &pending_snapshot);
    }
    let events = event_bus.subscribe();

    let shell_state = state.clone();

    let lv1 = spawn_actor(identity.address.clone(), identity.port, event_bus.clone());
    let command_bus = AppCommandBus::new();
    command_bus.set_generation(generation).await;
    command_bus.set_lv1(Some(lv1.clone())).await;
    command_bus.set_show(Some(shell_state.show.clone())).await;
    let fade_command_bus = command_bus.clone();
    let fade = spawn_engine(command_bus, event_bus.clone());
    fade_command_bus.set_fade(Some(fade.clone())).await;

    let mut runtime_handles = RuntimeHandles {
        active_generation: 0,
        lv1: Some(lv1.clone()),
        fade: Some(fade),
        command_bus: Some(fade_command_bus.clone()),
        projector: None,
        scene_recall_fader: Some(spawn_scene_recall_fader(
            generation,
            fade_command_bus.clone(),
            event_bus.clone(),
        )),
    };

    let initial_snapshot = lv1.get_state().await;
    if initial_snapshot.connection != crate::lv1::types::ConnectionStatus::Connected {
        runtime_handles.abort_all().await;
        let failed_snapshot = match failure_mode {
            ConnectFailureMode::ClearConnectedIdentity => state.fail_connect(generation).await,
            ConnectFailureMode::PreserveConnectedIdentity => state.fail_reconnect(generation).await,
        };
        if let Some(snapshot) = failed_snapshot {
            emit_snapshot(&app, &snapshot);
        }
        match failure_mode {
            ConnectFailureMode::ClearConnectedIdentity => {
                tracing::warn!(
                    event = "lv1_connect_failed",
                    host = %identity.address,
                    port = identity.port,
                    error = "LV1 did not connect",
                    "LV1 did not connect"
                );
            }
            ConnectFailureMode::PreserveConnectedIdentity => {
                tracing::warn!(
                    event = "lv1_reconnect_failed",
                    host = %identity.address,
                    port = identity.port,
                    error = "LV1 did not connect",
                    "LV1 did not connect"
                );
            }
        }
        return Err("LV1 did not connect".to_string());
    }

    let snapshot = match state.begin_connection(generation, initial_snapshot).await {
        Some(snapshot) => snapshot,
        None => {
            runtime_handles.abort_all().await;
            let snapshot = state.snapshot().await;
            return Ok(snapshot);
        }
    };

    let _installed_snapshot = install_connected_runtime(
        &app,
        &state,
        shell_state,
        generation,
        snapshot,
        events,
        runtime_handles,
        lifecycle,
    )
    .await?;

    let connected_host = identity.address.clone();
    let connected_port = identity.port;
    let Some(snapshot) = state
        .establish_connected_lv1_identity(generation, identity)
        .await
    else {
        lifecycle
            .clear_runtime_handles_with_active_generation(&state, generation)
            .await;
        let snapshot = state.snapshot().await;
        return Ok(snapshot);
    };

    tracing::info!(
        event = "lv1_connected",
        host = %connected_host,
        port = connected_port,
        "LV1 connected"
    );
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

fn should_save_connection_preferences(
    snapshot: &AppViewState,
    identity: &crate::connection_state::Lv1SystemIdentity,
) -> bool {
    snapshot.connection == AppConnectionState::Connected
        && snapshot.connected_lv1_identity.as_ref() == Some(identity)
}

fn remembered_auto_connect_target(
    preferences: &crate::connection_preferences::ConnectionPreferences,
    systems: &[crate::connection_state::DiscoveredLv1System],
) -> Option<crate::connection_state::Lv1SystemIdentity> {
    let remembered = preferences.last_connected_lv1.as_ref()?;
    let available_systems = systems
        .iter()
        .filter(|system| system.status == crate::connection_state::DiscoveredLv1Status::Available);

    if let Some(system) = remembered.uuid.as_ref().and_then(|remembered_uuid| {
        available_systems
            .clone()
            .find(|system| system.identity.uuid.as_ref() == Some(remembered_uuid))
    }) {
        return Some(system.identity.clone());
    }

    let remembered_host = remembered.host.as_deref()?.trim();
    if remembered_host.is_empty() {
        return None;
    }

    let mut host_matches = available_systems
        .filter(|system| system.identity.host.as_deref().map(str::trim) == Some(remembered_host));
    let first = host_matches.next()?;
    if host_matches.next().is_some() {
        return None;
    }

    Some(first.identity.clone())
}

fn reconnect_target_for_connected_identity(
    connected_identity: &crate::connection_state::Lv1SystemIdentity,
    systems: &[crate::connection_state::DiscoveredLv1System],
) -> Option<crate::connection_state::Lv1SystemIdentity> {
    let connected_uuid = connected_identity.uuid.as_ref()?;
    systems
        .iter()
        .find(|system| system.identity.uuid.as_ref() == Some(connected_uuid))
        .map(|system| system.identity.clone())
}

async fn startup_discovery_or_current_snapshot(
    state: &ShellState,
    discovery_result: Result<AppViewState, String>,
) -> AppViewState {
    match discovery_result {
        Ok(snapshot) => snapshot,
        Err(_) => state.snapshot().await,
    }
}

#[allow(clippy::too_many_arguments)]
async fn install_connected_runtime<R: Runtime>(
    app: &AppHandle<R>,
    state: &ShellState,
    shell_state: ShellState,
    generation: u64,
    snapshot: AppViewState,
    events: tokio::sync::broadcast::Receiver<AppEvent>,
    mut runtime_handles: RuntimeHandles,
    lifecycle: &AppLifecycle,
) -> Result<AppViewState, String> {
    let (projector_start_tx, projector_start_rx) = tokio::sync::oneshot::channel();

    runtime_handles.projector = Some(spawn_projector(ProjectorInputs {
        app: app.clone(),
        shell_state,
        active_command_bus: lifecycle.command_bus_holder(),
        generation,
        events,
        logs: ui_log_receiver(app)?,
        start_rx: projector_start_rx,
    }));

    if let Err(mut stale_handles) = lifecycle
        .install_runtime_handles(state, generation, runtime_handles)
        .await
    {
        stale_handles.abort_all().await;
        let snapshot = state.snapshot().await;
        return Ok(snapshot);
    }

    // Emit the initial snapshot before any buffered bus events can be projected.
    emit_snapshot(app, &snapshot);
    let _ = projector_start_tx.send(());

    Ok(snapshot)
}

fn emit_snapshot<R: Runtime>(app: &AppHandle<R>, snapshot: &AppViewState) {
    if let Err(err) = app.emit("app-status-changed", snapshot) {
        eprintln!("failed to emit app-status-changed: {err}");
    }
}

fn ui_log_receiver<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<tokio::sync::broadcast::Receiver<crate::logging::UiLogEvent>, String> {
    Ok(app.state::<UiLogReceiverState>().subscribe())
}

async fn save_show_file_to_path(
    state: &State<'_, ShellState>,
    active_command_bus: ActiveCommandBus,
    path: PathBuf,
) -> Result<AppViewState, String> {
    let saved_at = crate::time::current_timestamp_millis();
    let command_bus = current_command_bus(active_command_bus, "save_show_file").await?;
    let file = command_bus
        .export_show_file_for_save(saved_at.clone())
        .await
        .map_err(map_app_command_error)?;
    write_show_file(&path, &file, &backup_folder())?;
    Ok(state.mark_show_file_saved(path, saved_at).await)
}

#[cfg(test)]
async fn new_show_file_snapshot_for_test(
    state: ShellState,
    active_command_bus: ActiveCommandBus,
) -> Result<AppViewState, String> {
    let command_bus = current_command_bus(active_command_bus, "new_show_file").await?;
    let lv1 = state.lv1_snapshot().await;
    let result = command_bus
        .new_show_file(lv1)
        .await
        .map_err(map_app_command_error)?;
    Ok(state
        .apply_new_show_file_metadata(result.selected_scene_id)
        .await)
}

fn ensure_show_file_folder(path: std::path::PathBuf) -> Result<std::path::PathBuf, String> {
    std::fs::create_dir_all(&path)
        .map_err(|err| format!("Failed to create show file folder: {err}"))?;
    Ok(path)
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::app_state::ProjectionOutcome;
    use crate::fade::events::FadeEvent;
    use crate::lv1::events::Lv1Event;
    use crate::lv1::types::{ConnectionStatus, Lv1StateSnapshot};
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tauri::{Listener, test::mock_app};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "advanced-show-control-commands-{}-{}-{}",
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

    fn manage_ui_log_receiver<R: Runtime>(
        app: &tauri::App<R>,
    ) -> tokio::sync::broadcast::Sender<crate::logging::UiLogEvent> {
        let (tx, _rx) = tokio::sync::broadcast::channel(8);
        app.manage(tx.clone());
        tx
    }

    fn spawn_started_projector<R: Runtime>(
        handle: AppHandle<R>,
        state: ShellState,
        active_command_bus: ActiveCommandBus,
        generation: u64,
        events: tokio::sync::broadcast::Receiver<AppEvent>,
        logs: tokio::sync::broadcast::Receiver<crate::logging::UiLogEvent>,
    ) -> tokio::task::JoinHandle<()> {
        let (projector_start_tx, projector_start_rx) = tokio::sync::oneshot::channel();
        let projector = crate::projector::spawn_projector(crate::projector::ProjectorInputs {
            app: handle,
            shell_state: state,
            active_command_bus,
            generation,
            events,
            logs,
            start_rx: projector_start_rx,
        });
        let _ = projector_start_tx.send(());
        projector
    }

    #[tokio::test]
    async fn runtime_projector_accepts_log_input() {
        let app = mock_app();
        let handle = app.handle().clone();
        let event_bus = AppEventBus::default();
        let state = ShellState::new(event_bus.clone());
        let active_command_bus = ActiveCommandBus::default();
        let (log_tx, log_rx) = tokio::sync::broadcast::channel(8);
        let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let received_events = received.clone();
        handle.listen_any("app-status-changed", move |event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            received_events.lock().unwrap().push(payload);
        });

        let projector = crate::projector::spawn_projector(crate::projector::ProjectorInputs {
            app: handle,
            shell_state: state,
            active_command_bus,
            generation: 0,
            events: event_bus.subscribe(),
            logs: log_rx,
            start_rx: {
                let (tx, rx) = tokio::sync::oneshot::channel();
                let _ = tx.send(());
                rx
            },
        });

        log_tx
            .send(crate::logging::UiLogEvent {
                severity: crate::app_state::LogSeverity::Info,
                message: "runtime projector log".to_string(),
            })
            .unwrap();
        tokio::time::sleep(
            crate::projector::PROJECTOR_INTERVAL + std::time::Duration::from_millis(60),
        )
        .await;

        projector.abort();
        assert!(received.lock().unwrap().iter().any(|snapshot| {
            snapshot["logs"].as_array().is_some_and(|logs| {
                logs.iter()
                    .any(|entry| entry["message"] == "runtime projector log")
            })
        }));
    }

    fn remembered_preferences(
        uuid: Option<&str>,
        host: Option<&str>,
    ) -> crate::connection_preferences::ConnectionPreferences {
        crate::connection_preferences::ConnectionPreferences {
            last_connected_lv1: Some(crate::connection_preferences::LastConnectedLv1 {
                uuid: uuid.map(str::to_string),
                host: host.map(str::to_string),
                address: "192.168.1.35".to_string(),
                port: 50000,
            }),
        }
    }

    fn discovered_system(
        uuid: Option<&str>,
        host: Option<&str>,
        address: &str,
        status: crate::connection_state::DiscoveredLv1Status,
    ) -> crate::connection_state::DiscoveredLv1System {
        crate::connection_state::DiscoveredLv1System {
            identity: crate::connection_state::Lv1SystemIdentity {
                uuid: uuid.map(str::to_string),
                host: host.map(str::to_string),
                address: address.to_string(),
                port: 50000,
            },
            latency_ms: Some(10),
            status,
        }
    }

    #[test]
    fn ensure_show_file_folder_creates_missing_directory() {
        let folder = temp_dir("show-folder").join("Advanced Show Control");

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
        let _ = set_scene_scope_faders_enabled;
        let _ = set_scene_scope_pan_enabled;
    }

    #[test]
    fn connection_chooser_commands_are_exposed() {
        let _ = refresh_lv1_discovery;
        let _ = connect_lv1_system;
        let _ = attempt_reconnect_lv1;
        let _ = startup_auto_connect_lv1;
        let _ = reconnect_timed_out;
    }

    #[tokio::test]
    async fn cue_scene_updates_show_state_even_when_lockout_is_enabled() {
        let state = ShellState::default();
        let active_command_bus = ActiveCommandBus::default();
        let command_bus = AppCommandBus::new();
        command_bus.set_show(Some(state.show.clone())).await;
        active_command_bus.set(Some(command_bus)).await;
        state
            .show
            .replace_snapshot(crate::show::types::ShowSnapshot {
                lockout: true,
                scene_configs: vec![crate::show::types::SceneConfig {
                    scene_id: "1::Verse".to_string(),
                    scene_index: 1,
                    scene_name: "Verse".to_string(),
                    duration_ms: 0,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: Default::default(),
                }],
                cued_scene_id: None,
            })
            .await;

        let snapshot =
            cue_scene_snapshot(state.clone(), active_command_bus, "1::Verse".to_string())
                .await
                .unwrap();

        assert_eq!(snapshot.cued_scene_id, Some("1::Verse".to_string()));
        assert!(snapshot.lockout);
        assert!(!snapshot.show_file_dirty);
    }

    #[tokio::test]
    async fn set_lockout_marks_show_file_dirty_when_state_changes() {
        let state = ShellState::default();
        state
            .show
            .replace_snapshot(crate::show::types::ShowSnapshot {
                lockout: false,
                scene_configs: vec![crate::show::types::SceneConfig {
                    scene_id: "1::Intro".to_string(),
                    scene_index: 1,
                    scene_name: "Intro".to_string(),
                    duration_ms: 0,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: Default::default(),
                }],
                cued_scene_id: None,
            })
            .await;
        let lifecycle = lifecycle_with_show(&state).await;

        let snapshot = set_lockout_snapshot(state.clone(), lifecycle.command_bus_holder(), true)
            .await
            .unwrap();

        assert!(snapshot.lockout);
        assert!(snapshot.show_file_dirty);
    }

    #[tokio::test]
    async fn set_lockout_noop_keeps_show_file_clean() {
        let state = ShellState::default();
        let lifecycle = lifecycle_with_show(&state).await;

        let snapshot = set_lockout_snapshot(state.clone(), lifecycle.command_bus_holder(), false)
            .await
            .unwrap();

        assert!(!snapshot.lockout);
        assert!(!snapshot.show_file_dirty);
    }

    #[tokio::test]
    async fn set_scene_duration_ms_routes_through_command_bus() {
        let state = ShellState::default();
        state
            .show
            .replace_snapshot(crate::show::types::ShowSnapshot {
                lockout: false,
                scene_configs: vec![crate::show::types::SceneConfig {
                    scene_id: "1::Intro".to_string(),
                    scene_index: 1,
                    scene_name: "Intro".to_string(),
                    duration_ms: 0,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: Default::default(),
                }],
                cued_scene_id: None,
            })
            .await;
        let lifecycle = AppLifecycle::default();
        let command_bus = AppCommandBus::new();
        command_bus.set_show(Some(state.show.clone())).await;

        lifecycle.set_command_bus(Some(command_bus)).await;

        let resolved = current_command_bus(lifecycle.command_bus_holder(), "set_scene_duration_ms")
            .await
            .unwrap();

        resolved
            .set_scene_duration_ms("1::Intro".to_string(), 2500)
            .await
            .unwrap();

        assert_eq!(state.snapshot().await.selected_scene_id, None);
    }

    #[tokio::test]
    async fn select_scene_config_updates_shell_projection_after_bus_validation() {
        let state = ShellState::default();
        state
            .show
            .replace_snapshot(crate::show::types::ShowSnapshot {
                lockout: false,
                scene_configs: vec![crate::show::types::SceneConfig {
                    scene_id: "1::Verse".to_string(),
                    scene_index: 1,
                    scene_name: "Verse".to_string(),
                    duration_ms: 0,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: Default::default(),
                }],
                cued_scene_id: None,
            })
            .await;
        let lifecycle = AppLifecycle::default();
        let command_bus = AppCommandBus::new();
        command_bus.set_show(Some(state.show.clone())).await;

        lifecycle.set_command_bus(Some(command_bus)).await;

        let resolved = current_command_bus(lifecycle.command_bus_holder(), "select_scene_config")
            .await
            .unwrap();

        resolved
            .select_scene_config("1::Verse".to_string())
            .await
            .unwrap();

        let snapshot = state
            .select_scene_config("1::Verse".to_string())
            .await
            .unwrap();

        assert_eq!(snapshot.selected_scene_id, Some("1::Verse".to_string()));
    }

    #[tokio::test]
    async fn cue_scene_rejects_when_command_bus_is_unavailable() {
        let state = ShellState::default();

        let err = cue_scene_snapshot(
            state.clone(),
            ActiveCommandBus::default(),
            "1::Verse".to_string(),
        )
        .await
        .unwrap_err();

        assert_eq!(err, "App command bus is unavailable");
    }

    #[tokio::test]
    async fn new_show_file_requires_active_command_bus() {
        let state = ShellState::default();

        let err = new_show_file_snapshot_for_test(state, ActiveCommandBus::default())
            .await
            .unwrap_err();

        assert_eq!(err, "App command bus is unavailable");
    }

    #[tokio::test]
    async fn cue_scene_rejects_unknown_scene_id() {
        let state = ShellState::default();
        let command_bus = AppCommandBus::new();
        command_bus.set_show(Some(state.show.clone())).await;
        let active_command_bus = ActiveCommandBus::default();
        active_command_bus.set(Some(command_bus)).await;

        let err = cue_scene_snapshot(state.clone(), active_command_bus, "99::Missing".to_string())
            .await
            .unwrap_err();

        assert_eq!(err, "Scene config not found");
        assert_eq!(state.snapshot().await.cued_scene_id, None);
    }

    async fn lifecycle_with_show(state: &ShellState) -> AppLifecycle {
        let lifecycle = AppLifecycle::default();
        let bus = AppCommandBus::new();
        bus.set_show(Some(state.show.clone())).await;
        lifecycle.set_command_bus(Some(bus)).await;
        lifecycle
    }

    async fn lifecycle_with_recall_show(lockout: bool, with_lv1: bool) -> AppLifecycle {
        let lifecycle = AppLifecycle::default();
        let bus = AppCommandBus::new();
        let event_bus = AppEventBus::default();
        let show = crate::show::handle::ShowStateHandle::new_empty(event_bus.clone());
        show.replace_snapshot(crate::show::types::ShowSnapshot {
            lockout,
            scene_configs: vec![crate::show::types::SceneConfig {
                scene_id: "1::Verse".to_string(),
                scene_index: 1,
                scene_name: "Verse".to_string(),
                duration_ms: 0,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: Default::default(),
            }],
            cued_scene_id: Some("1::Verse".to_string()),
        })
        .await;
        bus.set_show(Some(show)).await;
        if with_lv1 {
            let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(2);
            bus.set_lv1(Some(crate::lv1::handle::Lv1ActorHandle::new(lv1_tx)))
                .await;
            tokio::spawn(async move {
                while let Some(command) = lv1_rx.recv().await {
                    match command {
                        crate::lv1::commands::Lv1Command::GetState { reply } => {
                            let _ = reply.send(crate::lv1::types::Lv1StateSnapshot {
                                connection: crate::lv1::types::ConnectionStatus::Connected,
                                scene: None,
                                scene_list: vec![crate::lv1::types::SceneListEntry {
                                    index: 1,
                                    name: "Verse".to_string(),
                                }],
                                channels: Vec::new(),
                            });
                        }
                        crate::lv1::commands::Lv1Command::RecallScene { reply, .. } => {
                            let _ = reply.send(Ok(()));
                        }
                        _ => panic!("unexpected LV1 command"),
                    }
                }
            });
        }
        lifecycle.set_command_bus(Some(bus)).await;
        lifecycle
    }

    #[tokio::test]
    async fn cue_scene_updates_show_state_through_command_bus_and_returns_snapshot() {
        let state = recall_state_with_unstored_scene(true).await;
        let lifecycle = lifecycle_with_show(&state).await;

        let snapshot = cue_scene_snapshot(
            state.clone(),
            lifecycle.command_bus_holder(),
            "1::Verse".to_string(),
        )
        .await
        .unwrap();

        assert_eq!(snapshot.cued_scene_id, Some("1::Verse".to_string()));
        assert!(snapshot.lockout);
    }

    async fn recall_state_with_unstored_scene(lockout: bool) -> ShellState {
        let state = ShellState::default();
        state
            .show
            .replace_snapshot(crate::show::types::ShowSnapshot {
                lockout,
                scene_configs: vec![crate::show::types::SceneConfig {
                    scene_id: "1::Verse".to_string(),
                    scene_index: 1,
                    scene_name: "Verse".to_string(),
                    duration_ms: 0,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: Default::default(),
                }],
                cued_scene_id: Some("1::Verse".to_string()),
            })
            .await;
        state
    }

    #[tokio::test]
    async fn recall_scene_routes_lockout_block_through_command_bus() {
        let state = recall_state_with_unstored_scene(true).await;
        let lifecycle = lifecycle_with_recall_show(true, true).await;

        let err = recall_scene_snapshot(
            state,
            lifecycle.command_bus_holder(),
            "1::Verse".to_string(),
        )
        .await
        .unwrap_err();

        assert_eq!(err, "Recall blocked: lockout is enabled");
    }

    #[tokio::test]
    async fn recall_scene_routes_missing_lv1_state_through_command_bus() {
        let state = recall_state_with_unstored_scene(false).await;
        let lifecycle = lifecycle_with_recall_show(false, false).await;

        let err = recall_scene_snapshot(
            state,
            lifecycle.command_bus_holder(),
            "1::Verse".to_string(),
        )
        .await
        .unwrap_err();

        assert_eq!(err, "Recall blocked: LV1 state is unavailable");
    }

    #[tokio::test]
    async fn resolve_connect_target_returns_explicit_target() {
        let target = resolve_connect_target(Some("127.0.0.1".to_string()), Some(1234), 1000)
            .await
            .expect("explicit target should resolve");

        assert_eq!(target, ("127.0.0.1".to_string(), 1234));
    }

    #[tokio::test]
    async fn reconnect_timed_out_clears_reconnect_state() {
        let app = mock_app();
        let handle = app.handle().clone();
        let state = ShellState::default();
        let lifecycle = AppLifecycle::default();
        let reconnecting = enter_reconnect_state(&state).await;

        let snapshot =
            reconnect_timed_out_snapshot(handle, state, &lifecycle, reconnecting.reconnect.attempt)
                .await
                .expect("timeout command should return snapshot");

        assert!(!snapshot.reconnect.active);
    }

    #[tokio::test(start_paused = true)]
    async fn projector_does_not_emit_raw_lv1_event() {
        let app = mock_app();
        let handle = app.handle().clone();
        let raw_events = Arc::new(Mutex::new(0usize));
        let raw_events_for_listener = raw_events.clone();

        handle.listen_any("lv1-event", move |_| {
            *raw_events_for_listener.lock().unwrap() += 1;
        });

        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        let event_bus = AppEventBus::default();
        let (_log_tx, log_rx) = tokio::sync::broadcast::channel(8);
        let projector = spawn_started_projector(
            handle,
            state,
            ActiveCommandBus::default(),
            generation,
            event_bus.subscribe(),
            log_rx,
        );

        event_bus.publish(AppEvent::Lv1(Lv1Event::Connected));
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        assert_eq!(*raw_events.lock().unwrap(), 0);
        projector.abort();
    }

    #[tokio::test]
    async fn reconnect_timed_out_aborts_runtime_and_clears_command_bus_for_matching_attempt() {
        let app = mock_app();
        let handle = app.handle().clone();
        let state = ShellState::default();
        let lifecycle = AppLifecycle::default();
        let reconnecting = enter_reconnect_state(&state).await;
        let command_bus = AppCommandBus::new();
        let installed = lifecycle
            .install_runtime_handles(
                &state,
                1,
                RuntimeHandles {
                    active_generation: 0,
                    lv1: None,
                    fade: None,
                    command_bus: Some(command_bus),
                    projector: Some(tokio::spawn(async {
                        std::future::pending::<()>().await;
                    })),
                    scene_recall_fader: Some(tokio::spawn(async {
                        std::future::pending::<()>().await;
                    })),
                },
            )
            .await;
        assert!(installed.is_ok());
        assert!(lifecycle.current_command_bus().await.is_some());

        let snapshot = reconnect_timed_out_snapshot(
            handle,
            state.clone(),
            &lifecycle,
            reconnecting.reconnect.attempt,
        )
        .await
        .expect("timeout command should return snapshot");

        assert!(!snapshot.reconnect.active);
        assert!(lifecycle.current_command_bus().await.is_none());
        let handles = state.handles.lock().await;
        assert_eq!(handles.active_generation, 0);
        assert!(handles.command_bus.is_none());
        assert!(handles.projector.is_none());
        assert!(handles.scene_recall_fader.is_none());
    }

    #[tokio::test]
    async fn stale_reconnect_timed_out_does_not_clear_newer_reconnect_state() {
        let app = mock_app();
        let handle = app.handle().clone();
        let state = ShellState::default();
        let first_reconnect = enter_reconnect_state(&state).await;
        assert_eq!(
            state
                .apply_lv1_event_to_projection(1, &Lv1Event::Connected)
                .await,
            ProjectionOutcome::Applied
        );
        assert_eq!(
            state
                .apply_lv1_event_to_projection(
                    1,
                    &Lv1Event::Disconnected {
                        reason: "test".to_string(),
                    },
                )
                .await,
            ProjectionOutcome::Applied
        );
        let second_reconnect = state
            .snapshot_for_generation(1)
            .await
            .expect("second disconnect should apply");
        assert!(second_reconnect.reconnect.active);
        assert!(second_reconnect.reconnect.attempt > first_reconnect.reconnect.attempt);

        let snapshot = reconnect_timed_out_snapshot(
            handle,
            state,
            &AppLifecycle::default(),
            first_reconnect.reconnect.attempt,
        )
        .await
        .expect("timeout command should return snapshot");

        assert!(snapshot.reconnect.active);
        assert_eq!(
            snapshot.reconnect.attempt,
            second_reconnect.reconnect.attempt
        );
    }

    async fn enter_reconnect_state(state: &ShellState) -> AppViewState {
        state
            .set_connected_lv1_identity(Some(crate::connection_state::Lv1SystemIdentity {
                uuid: Some("uuid-1".to_string()),
                host: Some("LV1-FOH".to_string()),
                address: "192.168.1.35".to_string(),
                port: 50000,
            }))
            .await;
        let (generation, _) = state.begin_connecting().await;
        assert_eq!(
            state
                .apply_lv1_event_to_projection(
                    generation,
                    &Lv1Event::Disconnected {
                        reason: "test".to_string(),
                    },
                )
                .await,
            ProjectionOutcome::Applied
        );
        state
            .snapshot_for_generation(generation)
            .await
            .expect("disconnect should apply")
    }

    #[test]
    fn remembered_uuid_matches_discovered_identity_without_host_fallback() {
        let preferences = crate::connection_preferences::ConnectionPreferences {
            last_connected_lv1: Some(crate::connection_preferences::LastConnectedLv1 {
                uuid: Some("uuid-1".to_string()),
                host: Some("Old Host".to_string()),
                address: "192.168.1.35".to_string(),
                port: 50000,
            }),
        };
        let systems = vec![crate::connection_state::DiscoveredLv1System {
            identity: crate::connection_state::Lv1SystemIdentity {
                uuid: Some("uuid-1".to_string()),
                host: Some("New Host".to_string()),
                address: "10.0.0.20".to_string(),
                port: 50000,
            },
            latency_ms: Some(10),
            status: crate::connection_state::DiscoveredLv1Status::Available,
        }];

        let matched = remembered_auto_connect_target(&preferences, &systems).unwrap();

        assert_eq!(matched.address, "10.0.0.20");
    }

    #[test]
    fn remembered_hostname_fallback_matches_single_available_system() {
        let preferences = remembered_preferences(Some("uuid-1"), Some(" LV1-FOH "));
        let systems = vec![discovered_system(
            Some("uuid-2"),
            Some("LV1-FOH"),
            "10.0.0.20",
            crate::connection_state::DiscoveredLv1Status::Available,
        )];

        let matched = remembered_auto_connect_target(&preferences, &systems).unwrap();

        assert_eq!(matched.address, "10.0.0.20");
    }

    #[test]
    fn remembered_uuid_match_takes_precedence_over_hostname_match() {
        let preferences = remembered_preferences(Some("uuid-1"), Some("LV1-FOH"));
        let systems = vec![
            discovered_system(
                Some("uuid-2"),
                Some("LV1-FOH"),
                "10.0.0.20",
                crate::connection_state::DiscoveredLv1Status::Available,
            ),
            discovered_system(
                Some("uuid-1"),
                Some("Renamed LV1"),
                "10.0.0.21",
                crate::connection_state::DiscoveredLv1Status::Available,
            ),
        ];

        let matched = remembered_auto_connect_target(&preferences, &systems).unwrap();

        assert_eq!(matched.address, "10.0.0.21");
    }

    #[test]
    fn remembered_hostname_fallback_rejects_duplicate_available_hosts() {
        let preferences = remembered_preferences(None, Some("LV1-FOH"));
        let systems = vec![
            discovered_system(
                None,
                Some("LV1-FOH"),
                "10.0.0.20",
                crate::connection_state::DiscoveredLv1Status::Available,
            ),
            discovered_system(
                None,
                Some("LV1-FOH"),
                "10.0.0.21",
                crate::connection_state::DiscoveredLv1Status::Available,
            ),
        ];

        assert!(remembered_auto_connect_target(&preferences, &systems).is_none());
    }

    #[test]
    fn remembered_hostname_fallback_ignores_unavailable_systems() {
        let preferences = remembered_preferences(None, Some("LV1-FOH"));
        let systems = vec![discovered_system(
            None,
            Some("LV1-FOH"),
            "10.0.0.20",
            crate::connection_state::DiscoveredLv1Status::Unavailable,
        )];

        assert!(remembered_auto_connect_target(&preferences, &systems).is_none());
    }

    #[test]
    fn remembered_host_matches_when_uuid_does_not() {
        let preferences = crate::connection_preferences::ConnectionPreferences {
            last_connected_lv1: Some(crate::connection_preferences::LastConnectedLv1 {
                uuid: Some("uuid-1".to_string()),
                host: Some("LV1".to_string()),
                address: "192.168.1.35".to_string(),
                port: 50000,
            }),
        };
        let systems = vec![crate::connection_state::DiscoveredLv1System {
            identity: crate::connection_state::Lv1SystemIdentity {
                uuid: Some("uuid-2".to_string()),
                host: Some("LV1".to_string()),
                address: "192.168.1.35".to_string(),
                port: 50000,
            },
            latency_ms: Some(10),
            status: crate::connection_state::DiscoveredLv1Status::Available,
        }];

        let matched = remembered_auto_connect_target(&preferences, &systems).unwrap();

        assert_eq!(matched.address, "192.168.1.35");
    }

    #[test]
    fn reconnect_target_requires_connected_identity_uuid_without_host_fallback() {
        let connected = crate::connection_state::Lv1SystemIdentity {
            uuid: None,
            host: Some("LV1".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };
        let systems = vec![crate::connection_state::DiscoveredLv1System {
            identity: crate::connection_state::Lv1SystemIdentity {
                uuid: Some("uuid-2".to_string()),
                host: Some("LV1".to_string()),
                address: "192.168.1.35".to_string(),
                port: 50000,
            },
            latency_ms: Some(10),
            status: crate::connection_state::DiscoveredLv1Status::Available,
        }];

        assert!(reconnect_target_for_connected_identity(&connected, &systems).is_none());
    }

    #[test]
    fn reconnect_target_uses_discovered_identity_for_matching_uuid() {
        let connected = crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("Old Host".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };
        let systems = vec![crate::connection_state::DiscoveredLv1System {
            identity: crate::connection_state::Lv1SystemIdentity {
                uuid: Some("uuid-1".to_string()),
                host: Some("New Host".to_string()),
                address: "10.0.0.20".to_string(),
                port: 50000,
            },
            latency_ms: Some(10),
            status: crate::connection_state::DiscoveredLv1Status::Available,
        }];

        let matched = reconnect_target_for_connected_identity(&connected, &systems).unwrap();

        assert_eq!(matched.address, "10.0.0.20");
    }

    #[tokio::test]
    async fn startup_discovery_failure_returns_current_snapshot() {
        let state = ShellState::default();

        let snapshot = startup_discovery_or_current_snapshot(
            &state,
            Err("Failed to discover LV1 systems: network unavailable".to_string()),
        )
        .await;

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert!(snapshot.discovered_lv1_systems.is_empty());
    }

    #[tokio::test]
    async fn preferences_are_saved_only_for_connected_matching_identity() {
        let state = ShellState::default();
        let identity = crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };

        let disconnected = state
            .set_connected_lv1_identity(Some(identity.clone()))
            .await;

        assert!(!should_save_connection_preferences(
            &disconnected,
            &identity
        ));

        let (generation, _) = state.begin_connecting().await;
        state
            .begin_connection(
                generation,
                Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: None,
                    scene_list: Vec::new(),
                    channels: Vec::new(),
                },
            )
            .await
            .expect("current generation should accept connected snapshot");
        let connected = state
            .establish_connected_lv1_identity(generation, identity.clone())
            .await
            .expect("connected snapshot should allow identity establishment");

        assert!(should_save_connection_preferences(&connected, &identity));
    }

    #[tokio::test]
    async fn active_command_bus_tracks_current_bus() {
        let holder = ActiveCommandBus::default();
        assert!(holder.current().await.is_none());

        let bus = AppCommandBus::new();
        holder.set(Some(bus.clone())).await;

        assert!(holder.current().await.is_some());

        holder.set(None).await;
        assert!(holder.current().await.is_none());
    }

    #[tokio::test]
    async fn emit_snapshot_directly_emits_app_status_changed() {
        let app = mock_app();
        let handle = app.handle().clone();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_for_listener = observed.clone();

        handle.listen_any("app-status-changed", move |event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            observed_for_listener.lock().unwrap().push(payload);
        });

        let state = ShellState::default();
        let snapshot = state.snapshot().await;
        emit_snapshot(&handle, &snapshot);
        tokio::task::yield_now().await;

        let observed = observed.lock().unwrap();
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0]["stateVersion"], snapshot.state_version);
    }

    #[tokio::test(start_paused = true)]
    async fn initial_connection_snapshot_is_emitted_before_coalesced_projector_events() {
        let app = mock_app();
        let _log_tx = manage_ui_log_receiver(&app);
        let handle = app.handle().clone();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_for_listener = observed.clone();

        handle.listen_any("app-status-changed", move |event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            observed_for_listener.lock().unwrap().push(payload);
        });

        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;

        let initial_snapshot = state
            .begin_connection(
                generation,
                Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: None,
                    scene_list: Vec::new(),
                    channels: Vec::new(),
                },
            )
            .await
            .expect("current generation should accept the initial snapshot");

        let event_bus = AppEventBus::default();
        let events = event_bus.subscribe();
        event_bus.publish(AppEvent::Fade(FadeEvent::FadeStarted));

        let lifecycle = AppLifecycle::default();
        let snapshot = install_connected_runtime(
            &handle,
            &state,
            state.clone(),
            generation,
            initial_snapshot,
            events,
            RuntimeHandles::default(),
            &lifecycle,
        )
        .await
        .expect("connected runtime should install successfully");

        assert_eq!(format!("{:?}", snapshot.connection), "Connected");

        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        let observed = observed.lock().unwrap();
        assert_eq!(observed.len(), 2);
        assert_eq!(observed[0]["fadeState"], "idle");
        assert_eq!(observed[1]["fadeState"], "running");
    }

    #[tokio::test(start_paused = true)]
    async fn projector_coalesces_runtime_updates_to_ten_hz() {
        let app = mock_app();
        let handle = app.handle().clone();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_for_listener = observed.clone();

        handle.listen_any("app-status-changed", move |event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            observed_for_listener.lock().unwrap().push(payload);
        });

        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        let _ = state
            .begin_connection(
                generation,
                Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: None,
                    scene_list: Vec::new(),
                    channels: vec![crate::lv1::types::ChannelInfo {
                        group: 1,
                        channel: 1,
                        name: "Channel 1".to_string(),
                        gain_db: 0.0,
                        muted: false,
                        pan: None,
                        balance: None,
                        width: None,
                        pan_mode: None,
                    }],
                },
            )
            .await
            .expect("current generation should accept the initial snapshot");

        let event_bus = AppEventBus::default();
        let (_log_tx, log_rx) = tokio::sync::broadcast::channel(8);
        let projector = spawn_started_projector(
            handle,
            state,
            ActiveCommandBus::default(),
            generation,
            event_bus.subscribe(),
            log_rx,
        );

        for gain_db in [1.0, 2.0, 3.0] {
            event_bus.publish(AppEvent::Lv1(Lv1Event::FaderChanged {
                group: 1,
                channel: 1,
                gain_db,
            }));
        }

        tokio::task::yield_now().await;
        assert!(observed.lock().unwrap().is_empty());

        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        {
            let observed_guard = observed.lock().unwrap();
            assert_eq!(observed_guard.len(), 1);
        }

        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        assert_eq!(observed.lock().unwrap().len(), 1);
        projector.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn projector_drains_events_while_waiting_for_projection_tick() {
        let app = mock_app();
        let handle = app.handle().clone();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_for_listener = observed.clone();

        handle.listen_any("app-status-changed", move |event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            observed_for_listener.lock().unwrap().push(payload);
        });

        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        let _ = state
            .begin_connection(
                generation,
                Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: None,
                    scene_list: Vec::new(),
                    channels: vec![crate::lv1::types::ChannelInfo {
                        group: 1,
                        channel: 1,
                        name: "Channel 1".to_string(),
                        gain_db: 0.0,
                        muted: false,
                        pan: None,
                        balance: None,
                        width: None,
                        pan_mode: None,
                    }],
                },
            )
            .await
            .expect("current generation should accept the initial snapshot");

        let event_bus = AppEventBus::new(4);
        let (_log_tx, log_rx) = tokio::sync::broadcast::channel(8);
        let projector = spawn_started_projector(
            handle,
            state,
            ActiveCommandBus::default(),
            generation,
            event_bus.subscribe(),
            log_rx,
        );

        for gain_db in 0..32 {
            event_bus.publish(AppEvent::Lv1(Lv1Event::FaderChanged {
                group: 1,
                channel: 1,
                gain_db: gain_db as f64,
            }));
            tokio::task::yield_now().await;
        }

        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        {
            let observed_guard = observed.lock().unwrap();
            assert_eq!(observed_guard.len(), 1);
            assert!(
                observed_guard[0]["logs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|entry| {
                        entry["message"]
                            != "shell-state-projector event subscriber lagged and missed 0 events"
                            && !entry["message"]
                                .as_str()
                                .unwrap_or_default()
                                .contains("shell-state-projector event subscriber lagged")
                    })
            );
        }
        projector.abort();
    }

    #[tokio::test]
    async fn stale_runtime_install_does_not_emit_current_snapshot() {
        let app = mock_app();
        let _log_tx = manage_ui_log_receiver(&app);
        let handle = app.handle().clone();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_for_listener = observed.clone();

        handle.listen_any("app-status-changed", move |event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload())
                .expect("app-status-changed payload should be valid JSON");
            observed_for_listener.lock().unwrap().push(payload);
        });

        let state = ShellState::default();
        let (stale_generation, _) = state.begin_connecting().await;
        let stale_snapshot = state
            .begin_connection(
                stale_generation,
                Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: None,
                    scene_list: Vec::new(),
                    channels: Vec::new(),
                },
            )
            .await
            .expect("stale setup should first connect");
        let (_current_generation, _) = state.begin_connecting().await;

        install_connected_runtime(
            &handle,
            &state,
            state.clone(),
            stale_generation,
            stale_snapshot,
            AppEventBus::default().subscribe(),
            RuntimeHandles::default(),
            &AppLifecycle::default(),
        )
        .await
        .expect("stale install should return current snapshot to command caller");

        tokio::task::yield_now().await;

        assert!(observed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn connected_runtime_installs_scene_recall_fader_handle() {
        let app = mock_app();
        let _log_tx = manage_ui_log_receiver(&app);
        let handle = app.handle().clone();
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;

        let initial_snapshot = state
            .begin_connection(
                generation,
                Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: None,
                    scene_list: Vec::new(),
                    channels: Vec::new(),
                },
            )
            .await
            .expect("current generation should accept the initial snapshot");

        let event_bus = AppEventBus::default();
        let scene_recall_fader = tokio::spawn(async {
            std::future::pending::<()>().await;
        });
        let lifecycle = AppLifecycle::default();

        install_connected_runtime(
            &handle,
            &state,
            state.clone(),
            generation,
            initial_snapshot,
            event_bus.subscribe(),
            RuntimeHandles {
                active_generation: 0,
                lv1: None,
                fade: None,
                command_bus: None,
                projector: None,
                scene_recall_fader: Some(scene_recall_fader),
            },
            &lifecycle,
        )
        .await
        .expect("connected runtime should install successfully");

        let mut handles = state.handles.lock().await;
        assert!(handles.scene_recall_fader.is_some());
        handles.abort_all().await;
    }

    #[tokio::test]
    async fn connected_runtime_can_install_projector_after_previous_runtime_aborts() {
        let app = mock_app();
        let _log_tx = manage_ui_log_receiver(&app);
        let handle = app.handle().clone();
        let state = ShellState::default();
        let lifecycle = AppLifecycle::default();

        for _ in 0..2 {
            let (generation, _) = state.begin_connecting().await;
            let initial_snapshot = state
                .begin_connection(
                    generation,
                    Lv1StateSnapshot {
                        connection: ConnectionStatus::Connected,
                        scene: None,
                        scene_list: Vec::new(),
                        channels: Vec::new(),
                    },
                )
                .await
                .expect("current generation should accept the initial snapshot");

            install_connected_runtime(
                &handle,
                &state,
                state.clone(),
                generation,
                initial_snapshot,
                AppEventBus::default().subscribe(),
                RuntimeHandles::default(),
                &lifecycle,
            )
            .await
            .expect("connected runtime should install successfully");

            state
                .clear_runtime_handles(generation, &lifecycle.command_bus_holder())
                .await;
        }
    }
}
