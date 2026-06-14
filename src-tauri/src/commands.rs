use advanced_show_control::fade::actor::spawn_engine;
use advanced_show_control::lv1::actor::spawn_actor;
use advanced_show_control::lv1::discovery::resolve_target;
use advanced_show_control::lv1::events::Lv1Event;
use advanced_show_control::runtime::commands::AppCommandBus;
use advanced_show_control::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
use advanced_show_control::scene_recall::spawn_scene_recall_fader;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tokio::sync::Mutex;
use tokio::task::spawn_blocking;

use crate::app_state::{
    AppConnectionState, AppViewState, LogSeverity, LogSource, RuntimeHandles, ShellState,
};
use crate::show_file::{backup_folder, default_show_folder, read_show_file, write_show_file};

const DEFAULT_DISCOVERY_TIMEOUT_MS: u64 = 1000;
const MIN_DISCOVERY_TIMEOUT_MS: u64 = 100;
const MAX_DISCOVERY_TIMEOUT_MS: u64 = 6000;

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
        advanced_show_control::lv1::discovery::discover(
            advanced_show_control::lv1::discovery::DiscoverOptions {
                timeout: std::time::Duration::from_millis(timeout),
                ..Default::default()
            },
        )
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
pub async fn set_scene_scope_faders_enabled(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
    enabled: bool,
) -> Result<AppViewState, String> {
    let snapshot = state
        .set_scene_scope_faders_enabled(scene_id, enabled)
        .await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_scene_scope_pan_enabled(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
    enabled: bool,
) -> Result<AppViewState, String> {
    let snapshot = state.set_scene_scope_pan_enabled(scene_id, enabled).await?;
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
    if let Some(command_bus) = active_command_bus.current().await {
        command_bus.set_generation(generation).await;
    }
    state
        .clear_runtime_handles_for_generation(generation, &active_command_bus)
        .await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn reconnect_timed_out(
    app: AppHandle,
    state: State<'_, ShellState>,
    active_command_bus: State<'_, ActiveCommandBus>,
    attempt: u64,
) -> Result<AppViewState, String> {
    reconnect_timed_out_snapshot(
        app,
        (*state).clone(),
        (*active_command_bus).clone(),
        attempt,
    )
    .await
}

async fn reconnect_timed_out_snapshot<R: Runtime>(
    app: AppHandle<R>,
    state: ShellState,
    active_command_bus: ActiveCommandBus,
    attempt: u64,
) -> Result<AppViewState, String> {
    if let Some(generation) = state.reconnect_timeout_generation(attempt).await {
        state
            .clear_runtime_handles_for_generation(generation, &active_command_bus)
            .await;
    }
    let snapshot = state.reconnect_timed_out(attempt).await;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn abort_all_fades(
    active_command_bus: State<'_, ActiveCommandBus>,
) -> Result<(), String> {
    let command_bus = active_command_bus.current().await;
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
    active_command_bus: State<'_, ActiveCommandBus>,
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
        (*active_command_bus).clone(),
        identity,
        ConnectFailureMode::ClearConnectedIdentity,
    )
    .await
}

#[tauri::command]
pub async fn connect_lv1_system(
    app: AppHandle,
    state: State<'_, ShellState>,
    active_command_bus: State<'_, ActiveCommandBus>,
    identity: crate::connection_state::Lv1SystemIdentity,
) -> Result<AppViewState, String> {
    let snapshot = connect_to_target(
        app.clone(),
        (*state).clone(),
        (*active_command_bus).clone(),
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
    active_command_bus: State<'_, ActiveCommandBus>,
) -> Result<AppViewState, String> {
    attempt_reconnect_lv1_snapshot(app, (*state).clone(), (*active_command_bus).clone()).await
}

async fn attempt_reconnect_lv1_snapshot<R: Runtime>(
    app: AppHandle<R>,
    state: ShellState,
    active_command_bus: ActiveCommandBus,
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
        active_command_bus,
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
    active_command_bus: State<'_, ActiveCommandBus>,
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
        return connect_lv1_system(app, state, active_command_bus, identity).await;
    }
    Ok(snapshot)
}

async fn connect_to_target<R: Runtime>(
    app: AppHandle<R>,
    state: ShellState,
    active_command_bus: ActiveCommandBus,
    identity: crate::connection_state::Lv1SystemIdentity,
    failure_mode: ConnectFailureMode,
) -> Result<AppViewState, String> {
    let event_bus = AppEventBus::default();
    let Some((generation, connecting_snapshot)) = state.try_begin_connecting().await else {
        let snapshot = state.snapshot().await;
        emit_snapshot(&app, &snapshot);
        return Ok(snapshot);
    };
    state.abort_current_runtime(&active_command_bus).await;
    emit_snapshot(&app, &connecting_snapshot);
    if let Some(pending_snapshot) = state
        .set_pending_lv1_identity_for_generation(generation, Some(identity.clone()))
        .await
    {
        emit_snapshot(&app, &pending_snapshot);
    }
    let events = event_bus.subscribe();

    let shell_state = state.clone();

    let lv1 = spawn_actor(identity.address.clone(), identity.port, event_bus.clone());
    let command_bus = AppCommandBus::new(event_bus.clone());
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
    if initial_snapshot.connection != advanced_show_control::lv1::types::ConnectionStatus::Connected
    {
        runtime_handles.abort_all().await;
        let failed_snapshot = match failure_mode {
            ConnectFailureMode::ClearConnectedIdentity => {
                state
                    .fail_connect_for_generation(generation, "LV1 did not connect")
                    .await
            }
            ConnectFailureMode::PreserveConnectedIdentity => {
                state
                    .fail_reconnect_for_generation(generation, "LV1 did not connect")
                    .await
            }
        };
        if let Some(snapshot) = failed_snapshot {
            emit_snapshot(&app, &snapshot);
        } else {
            let snapshot = state.snapshot().await;
            emit_snapshot(&app, &snapshot);
        }
        return Err("LV1 did not connect".to_string());
    }

    let snapshot = match state
        .begin_connection_for_generation(generation, initial_snapshot)
        .await
    {
        Some(snapshot) => snapshot,
        None => {
            runtime_handles.abort_all().await;
            if let Some(snapshot) = state
                .clear_pending_lv1_identity_for_generation(generation)
                .await
            {
                emit_snapshot(&app, &snapshot);
            }
            let snapshot = state.snapshot().await;
            emit_snapshot(&app, &snapshot);
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
        &active_command_bus,
    )
    .await?;

    let Some(snapshot) = state
        .establish_connected_lv1_identity_for_generation(generation, identity)
        .await
    else {
        state
            .clear_runtime_handles_with_active_generation(generation, &active_command_bus)
            .await;
        let snapshot = state.snapshot().await;
        emit_snapshot(&app, &snapshot);
        return Ok(snapshot);
    };

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
    let remembered_uuid = preferences.last_connected_lv1.as_ref()?.uuid.as_ref()?;
    systems
        .iter()
        .find(|system| system.identity.uuid.as_ref() == Some(remembered_uuid))
        .map(|system| system.identity.clone())
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
    active_command_bus: &ActiveCommandBus,
) -> Result<AppViewState, String> {
    // Emit the initial snapshot before any buffered bus events can be projected.
    emit_snapshot(app, &snapshot);

    runtime_handles.projector = Some(spawn_shell_state_projector(
        app.clone(),
        shell_state,
        active_command_bus.clone(),
        generation,
        events,
    ));

    if let Err(mut stale_handles) = state
        .install_runtime_handles_for_generation(generation, runtime_handles, active_command_bus)
        .await
    {
        stale_handles.abort_all().await;
        let snapshot = state.snapshot().await;
        emit_snapshot(app, &snapshot);
        return Ok(snapshot);
    }

    Ok(snapshot)
}

fn emit_snapshot<R: Runtime>(app: &AppHandle<R>, snapshot: &AppViewState) {
    if let Err(err) = app.emit("app-status-changed", snapshot) {
        eprintln!("failed to emit app-status-changed: {err}");
    }
}

const SHELL_PROJECTION_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

fn spawn_shell_state_projector<R: Runtime>(
    app: AppHandle<R>,
    state: ShellState,
    active_command_bus: ActiveCommandBus,
    generation: u64,
    mut events: tokio::sync::broadcast::Receiver<AppEvent>,
) -> tokio::task::JoinHandle<()> {
    let diagnostics_path = app
        .try_state::<crate::diagnostics::DiagnosticLogPath>()
        .map(|path| path.0.clone())
        .unwrap_or_else(|| crate::diagnostics::diagnostic_log_path(&app));
    tokio::spawn(async move {
        let _ = crate::diagnostics::append_diagnostic(
            &diagnostics_path,
            "tauri-shell",
            &format!("projector started generation={generation}"),
        );
        let mut projection_interval = tokio::time::interval(SHELL_PROJECTION_INTERVAL);
        projection_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        projection_interval.tick().await;
        let mut dirty = false;
        loop {
            tokio::select! {
                _ = projection_interval.tick() => {
                    if dirty {
                        if let Some(snapshot) = state.snapshot_for_generation(generation).await {
                            emit_snapshot(&app, &snapshot);
                        }
                        dirty = false;
                    }
                }
                received = events.recv() => {
                    match received {
                        Ok(app_event) => {
                            if apply_projector_event(&state, generation, &diagnostics_path, &active_command_bus, &app_event).await {
                                dirty = true;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                            let log_message = format!(
                                "shell-state-projector event subscriber lagged and missed {count} events"
                            );
                            state.push_log(LogSource::App, LogSeverity::Warning, log_message).await;
                            dirty = true;
                            log_lagged_subscriber("shell-state-projector", count);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    })
}

async fn apply_projector_event(
    state: &ShellState,
    generation: u64,
    diagnostics_path: &std::path::Path,
    active_command_bus: &ActiveCommandBus,
    event: &AppEvent,
) -> bool {
    match event {
        AppEvent::Lv1(event) => {
            if let Lv1Event::SceneListChanged(scenes) = event {
                let _ = append_scene_list_diagnostic_for_generation(
                    state,
                    generation,
                    diagnostics_path,
                    scenes,
                )
                .await;
            }

            if !state
                .apply_lv1_event_without_snapshot_for_generation(generation, event)
                .await
            {
                return false;
            }

            if matches!(event, Lv1Event::Disconnected { .. }) {
                state
                    .clear_runtime_handles_for_generation(generation, active_command_bus)
                    .await;
            }
            true
        }
        AppEvent::Fade(event) => {
            state
                .apply_fade_event_without_snapshot_for_generation(generation, event)
                .await
        }
        AppEvent::CommandFailed { command, message } => {
            let log_message = format!("command failed: {command}: {message}");
            state
                .push_log_for_generation(
                    generation,
                    LogSource::App,
                    LogSeverity::Error,
                    log_message,
                )
                .await
        }
        AppEvent::Diagnostic { source, message } => {
            if !state
                .append_diagnostic_for_generation(generation, diagnostics_path, source, message)
                .await
            {
                return false;
            }

            state
                .project_event_without_snapshot_for_generation(generation, event)
                .await
        }
        AppEvent::SceneRecall(_) => {
            state
                .project_event_without_snapshot_for_generation(generation, event)
                .await
        }
    }
}

#[cfg(test)]
async fn handle_diagnostic_event<R: Runtime>(
    app: &AppHandle<R>,
    state: &ShellState,
    generation: u64,
    diagnostics_path: &std::path::Path,
    source: &str,
    message: &str,
) -> Option<AppViewState> {
    if !state
        .append_diagnostic_for_generation(generation, diagnostics_path, source, message)
        .await
    {
        return None;
    }

    if !state
        .push_log_for_generation(
            generation,
            LogSource::App,
            LogSeverity::Warning,
            format!("{source}: {message}"),
        )
        .await
    {
        return None;
    }

    let _ = app;
    state.snapshot_for_generation(generation).await
}

async fn append_scene_list_diagnostic_for_generation(
    state: &ShellState,
    generation: u64,
    diagnostics_path: &std::path::Path,
    scenes: &[advanced_show_control::lv1::types::SceneListEntry],
) -> bool {
    let message = state
        .show
        .scene_reconciliation_diagnostic(scenes.to_vec())
        .await;
    state
        .append_diagnostic_for_generation(generation, diagnostics_path, "show-state", &message)
        .await
}

async fn save_show_file_to_path(
    state: &State<'_, ShellState>,
    path: PathBuf,
) -> Result<AppViewState, String> {
    let saved_at = crate::time::current_timestamp_millis();
    let file = state.export_show_file_for_save(saved_at.clone()).await?;
    write_show_file(&path, &file, &backup_folder())?;
    Ok(state.mark_show_file_saved(path, saved_at).await)
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
    use advanced_show_control::fade::events::FadeEvent;
    use advanced_show_control::lv1::types::{ConnectionStatus, Lv1StateSnapshot};
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
        let active_command_bus = ActiveCommandBus::default();
        let reconnecting = enter_reconnect_state(&state).await;

        let snapshot = reconnect_timed_out_snapshot(
            handle,
            state,
            active_command_bus,
            reconnecting.reconnect.attempt,
        )
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
        let projector = spawn_shell_state_projector(
            handle,
            state,
            ActiveCommandBus::default(),
            generation,
            event_bus.subscribe(),
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
        let active_command_bus = ActiveCommandBus::default();
        let reconnecting = enter_reconnect_state(&state).await;
        let command_bus = AppCommandBus::new(AppEventBus::default());
        let installed = state
            .install_runtime_handles_for_generation(
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
                &active_command_bus,
            )
            .await;
        assert!(installed.is_ok());
        assert!(active_command_bus.current().await.is_some());

        let snapshot = reconnect_timed_out_snapshot(
            handle,
            state.clone(),
            active_command_bus.clone(),
            reconnecting.reconnect.attempt,
        )
        .await
        .expect("timeout command should return snapshot");

        assert!(!snapshot.reconnect.active);
        assert!(active_command_bus.current().await.is_none());
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
        state
            .apply_lv1_event_for_generation(1, &Lv1Event::Connected)
            .await
            .expect("connected event should apply");
        let second_reconnect = state
            .apply_lv1_event_for_generation(
                1,
                &Lv1Event::Disconnected {
                    reason: "test".to_string(),
                },
            )
            .await
            .expect("second disconnect should apply");
        assert!(second_reconnect.reconnect.active);
        assert!(second_reconnect.reconnect.attempt > first_reconnect.reconnect.attempt);

        let snapshot = reconnect_timed_out_snapshot(
            handle,
            state,
            ActiveCommandBus::default(),
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
        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::Disconnected {
                    reason: "test".to_string(),
                },
            )
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
    fn remembered_uuid_absent_does_not_match_same_address() {
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

        assert!(remembered_auto_connect_target(&preferences, &systems).is_none());
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
            .begin_connection_for_generation(
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
            .establish_connected_lv1_identity_for_generation(generation, identity.clone())
            .await
            .expect("connected snapshot should allow identity establishment");

        assert!(should_save_connection_preferences(&connected, &identity));
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

    #[tokio::test(start_paused = true)]
    async fn initial_connection_snapshot_is_emitted_before_coalesced_projector_events() {
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

        let initial_snapshot = state
            .begin_connection_for_generation(
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

        let active_command_bus = ActiveCommandBus::default();
        let snapshot = install_connected_runtime(
            &handle,
            &state,
            state.clone(),
            generation,
            initial_snapshot,
            events,
            RuntimeHandles::default(),
            &active_command_bus,
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
    async fn diagnostic_event_updates_shell_state_log_and_coalesced_snapshot() {
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

        let initial_snapshot = state
            .begin_connection_for_generation(
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
        let active_command_bus = ActiveCommandBus::default();
        let _snapshot = install_connected_runtime(
            &handle,
            &state,
            state.clone(),
            generation,
            initial_snapshot,
            event_bus.subscribe(),
            RuntimeHandles::default(),
            &active_command_bus,
        )
        .await
        .expect("connected runtime should install successfully");

        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        assert!(!observed.lock().unwrap().is_empty());

        event_bus.publish(AppEvent::Diagnostic {
            source: "fade-engine".to_string(),
            message: "event subscriber lagged and missed 3 events".to_string(),
        });

        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        let observed = observed.lock().unwrap();
        assert_eq!(observed.len(), 2);
        assert_eq!(observed[1]["fadeState"], "idle");
        assert!(observed[1]["logs"].as_array().unwrap().iter().any(|entry| {
            entry["message"] == "fade-engine: event subscriber lagged and missed 3 events"
        }));
    }

    #[tokio::test]
    async fn stale_diagnostic_event_does_not_write_diagnostics_or_logs() {
        let app = mock_app();
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        let diagnostics_dir = temp_dir("stale-diagnostic-file");
        let diagnostics_path = diagnostics_dir.join("diagnostics.jsonl");

        let _ = state.disconnect().await;

        let snapshot = handle_diagnostic_event(
            &app.handle().clone(),
            &state,
            generation,
            &diagnostics_path,
            "fade-engine",
            "stale diagnostic",
        )
        .await;

        assert!(snapshot.is_none());
        assert!(!diagnostics_path.exists());
        assert!(
            state
                .snapshot()
                .await
                .logs
                .iter()
                .all(|entry| { entry.message != "fade-engine: stale diagnostic" })
        );

        let _ = fs::remove_dir_all(&diagnostics_dir);
    }

    #[tokio::test]
    async fn stale_scene_list_event_does_not_write_show_state_diagnostics() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        let diagnostics_dir = temp_dir("stale-scene-list-diagnostic");
        let diagnostics_path = diagnostics_dir.join("diagnostics.jsonl");
        let scenes = vec![advanced_show_control::lv1::types::SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        }];

        let _ = state.disconnect().await;

        assert!(
            !append_scene_list_diagnostic_for_generation(
                &state,
                generation,
                &diagnostics_path,
                &scenes,
            )
            .await
        );
        assert!(!diagnostics_path.exists());

        let _ = fs::remove_dir_all(&diagnostics_dir);
    }

    #[tokio::test]
    async fn stale_diagnostic_event_does_not_emit_snapshot_through_projector() {
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

        let initial_snapshot = state
            .begin_connection_for_generation(
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
        let active_command_bus = ActiveCommandBus::default();
        let _snapshot = install_connected_runtime(
            &handle,
            &state,
            state.clone(),
            generation,
            initial_snapshot,
            event_bus.subscribe(),
            RuntimeHandles::default(),
            &active_command_bus,
        )
        .await
        .expect("connected runtime should install successfully");

        for _ in 0..20 {
            if !observed.lock().unwrap().is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(!observed.lock().unwrap().is_empty());

        let _ = state.disconnect().await;

        event_bus.publish(AppEvent::Diagnostic {
            source: "fade-engine".to_string(),
            message: "stale projector diagnostic".to_string(),
        });

        for _ in 0..20 {
            if observed.lock().unwrap().len() == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(observed.lock().unwrap().len(), 1);

        let snapshot = state.snapshot().await;
        assert!(
            snapshot
                .logs
                .iter()
                .all(|entry| { entry.message != "fade-engine: stale projector diagnostic" })
        );
    }

    #[tokio::test]
    async fn stale_diagnostic_event_does_not_write_diagnostics_through_projector() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        let diagnostics_dir = temp_dir("stale-projector-diagnostic");
        let diagnostics_path = diagnostics_dir.join("diagnostics.jsonl");

        let _ = state.disconnect().await;

        let applied = apply_projector_event(
            &state,
            generation,
            &diagnostics_path,
            &ActiveCommandBus::default(),
            &AppEvent::Diagnostic {
                source: "fade-engine".to_string(),
                message: "stale projector diagnostic".to_string(),
            },
        )
        .await;

        assert!(!applied);
        assert!(!diagnostics_path.exists());

        let _ = fs::remove_dir_all(&diagnostics_dir);
    }

    #[tokio::test]
    async fn projector_applies_runtime_events_before_coalesced_snapshot() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;

        let applied = state
            .project_event_without_snapshot_for_generation(
                generation,
                &AppEvent::Diagnostic {
                    source: "shell-state-projector".to_string(),
                    message: "coalesced snapshot pending".to_string(),
                },
            )
            .await;

        assert!(applied);

        let snapshot = state
            .snapshot_for_generation(generation)
            .await
            .expect("current generation should still snapshot after projection");
        assert!(
            snapshot.logs.iter().any(|entry| {
                entry.message == "shell-state-projector: coalesced snapshot pending"
            })
        );
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
            .begin_connection_for_generation(
                generation,
                Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: None,
                    scene_list: Vec::new(),
                    channels: vec![advanced_show_control::lv1::types::ChannelInfo {
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
        let projector = spawn_shell_state_projector(
            handle,
            state,
            ActiveCommandBus::default(),
            generation,
            event_bus.subscribe(),
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
            .begin_connection_for_generation(
                generation,
                Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: None,
                    scene_list: Vec::new(),
                    channels: vec![advanced_show_control::lv1::types::ChannelInfo {
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
        let projector = spawn_shell_state_projector(
            handle,
            state,
            ActiveCommandBus::default(),
            generation,
            event_bus.subscribe(),
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
    async fn connected_runtime_installs_scene_recall_fader_handle() {
        let app = mock_app();
        let handle = app.handle().clone();
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;

        let initial_snapshot = state
            .begin_connection_for_generation(
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
        let active_command_bus = ActiveCommandBus::default();

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
            &active_command_bus,
        )
        .await
        .expect("connected runtime should install successfully");

        let mut handles = state.handles.lock().await;
        assert!(handles.scene_recall_fader.is_some());
        handles.abort_all().await;
    }

    #[tokio::test(start_paused = true)]
    async fn scene_recall_events_emit_coalesced_app_status_snapshot() {
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
        let event_bus = AppEventBus::default();
        let projector = spawn_shell_state_projector(
            handle,
            state,
            ActiveCommandBus::default(),
            generation,
            event_bus.subscribe(),
        );

        event_bus.publish(AppEvent::SceneRecall(
            advanced_show_control::scene_recall::events::SceneRecallEvent::Blocked {
                scene_label: "1: Intro".to_string(),
                reason: "locked out".to_string(),
            },
        ));

        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        assert!(!observed.lock().unwrap().is_empty());

        projector.abort();
    }
}
