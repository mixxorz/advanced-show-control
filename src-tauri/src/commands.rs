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
    AppConnectionState, AppViewState, ProjectionOutcome, RuntimeHandles, ShellState,
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
pub async fn cue_scene(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
) -> Result<AppViewState, String> {
    let snapshot = cue_scene_snapshot((*state).clone(), scene_id).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

async fn cue_scene_snapshot(state: ShellState, scene_id: String) -> Result<AppViewState, String> {
    tracing::debug!(
        event = "scene_cue_requested",
        scene_id = %scene_id,
        "Scene cue requested"
    );

    let scene = state
        .show
        .get_scene_config(scene_id.clone())
        .await
        .ok_or_else(|| {
            tracing::warn!(
                event = "scene_cue_blocked",
                scene_id = %scene_id,
                reason = "scene config not found",
                "Scene cue blocked: scene config not found"
            );
            "Scene config not found".to_string()
        })?;

    let changed = state.show.cue_scene(scene_id.clone()).await?;
    if changed {
        state.mark_show_file_dirty().await;
    }

    tracing::info!(
        event = "scene_cued",
        scene_id = %scene.scene_id,
        scene_index = scene.scene_index,
        scene_name = %scene.scene_name,
        "Scene cued: {}",
        scene.scene_name
    );

    Ok(state.snapshot().await)
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
pub async fn recall_scene(
    app: AppHandle,
    state: State<'_, ShellState>,
    active_command_bus: State<'_, ActiveCommandBus>,
    scene_id: String,
) -> Result<AppViewState, String> {
    let snapshot =
        recall_scene_snapshot((*state).clone(), (*active_command_bus).clone(), scene_id).await?;
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

    let show = state.show.get_snapshot().await;
    if show.lockout {
        return block_scene_recall(
            &scene_id,
            "lockout is enabled",
            "Recall blocked: lockout is enabled",
        );
    }

    let scene = show
        .scene_configs
        .iter()
        .find(|scene| scene.scene_id == scene_id)
        .cloned()
        .ok_or_else(|| {
            tracing::warn!(
                event = "scene_recall_blocked",
                scene_id = %scene_id,
                reason = "scene config not found",
                "Scene recall blocked: scene config not found"
            );
            "Scene config not found".to_string()
        })?;

    let lv1 = state.lv1_snapshot().await.ok_or_else(|| {
        tracing::warn!(
            event = "scene_recall_blocked",
            scene_id = %scene_id,
            reason = "LV1 state is unavailable",
            "Scene recall blocked: LV1 state is unavailable"
        );
        "Recall blocked: LV1 state is unavailable".to_string()
    })?;

    if lv1.connection != advanced_show_control::lv1::types::ConnectionStatus::Connected {
        return block_scene_recall(
            &scene_id,
            "LV1 is disconnected",
            "Recall blocked: LV1 is disconnected",
        );
    }

    let Some(lv1_scene) = lv1.scene_list.iter().find(|candidate| {
        candidate.index == scene.scene_index && candidate.name == scene.scene_name
    }) else {
        return block_scene_recall(
            &scene_id,
            "scene identity mismatch",
            "Recall blocked: scene identity mismatch",
        );
    };

    let command_bus = active_command_bus.current().await.ok_or_else(|| {
        tracing::warn!(
            event = "scene_recall_blocked",
            scene_id = %scene_id,
            reason = "LV1 command target is unavailable",
            "Scene recall blocked: LV1 command target is unavailable"
        );
        "Recall blocked: LV1 command target is unavailable".to_string()
    })?;

    if let Err(error) = command_bus.recall_scene(lv1_scene.index).await {
        tracing::warn!(
            event = "scene_recall_command_failed",
            scene_id = %scene.scene_id,
            scene_index = scene.scene_index,
            scene_name = %scene.scene_name,
            error = %error,
            "Scene recall command failed: {error}"
        );
        return Err(error.to_string());
    }

    tracing::info!(
        event = "scene_recall_command_sent",
        scene_id = %scene.scene_id,
        scene_index = scene.scene_index,
        scene_name = %scene.scene_name,
        "Scene recall command sent: {}",
        scene.scene_name
    );

    Ok(state.snapshot().await)
}

fn block_scene_recall<T>(
    scene_id: &str,
    reason: &'static str,
    message: &'static str,
) -> Result<T, String> {
    tracing::warn!(
        event = "scene_recall_blocked",
        scene_id = %scene_id,
        reason,
        "{message}"
    );
    Err(message.to_string())
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
    tracing::info!(
        event = "lockout_changed",
        enabled = enabled,
        "Lockout {}",
        if enabled { "enabled" } else { "disabled" }
    );
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn disconnect_lv1(
    app: AppHandle,
    state: State<'_, ShellState>,
    active_command_bus: State<'_, ActiveCommandBus>,
) -> Result<AppViewState, String> {
    tracing::debug!(
        event = "lv1_disconnect_requested",
        "LV1 disconnect requested"
    );
    let (generation, snapshot) = state.disconnect().await;
    if let Some(command_bus) = active_command_bus.current().await {
        command_bus.set_generation(generation).await;
    }
    state
        .clear_runtime_handles(generation, &active_command_bus)
        .await;
    tracing::info!(event = "lv1_disconnected", "Disconnected from LV1");
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
            .clear_runtime_handles(generation, &active_command_bus)
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
    state.abort_current_runtime(&active_command_bus).await;
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
        &active_command_bus,
    )
    .await?;

    let connected_host = identity.address.clone();
    let connected_port = identity.port;
    let Some(snapshot) = state
        .establish_connected_lv1_identity(generation, identity)
        .await
    else {
        state
            .clear_runtime_handles_with_active_generation(generation, &active_command_bus)
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
    active_command_bus: &ActiveCommandBus,
) -> Result<AppViewState, String> {
    let (projector_start_tx, projector_start_rx) = tokio::sync::oneshot::channel();

    runtime_handles.projector = Some(spawn_shell_state_projector(
        app.clone(),
        shell_state,
        active_command_bus.clone(),
        generation,
        events,
        projector_start_rx,
    ));

    if let Err(mut stale_handles) = state
        .install_runtime_handles(generation, runtime_handles, active_command_bus)
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

const SHELL_PROJECTION_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

fn spawn_shell_state_projector<R: Runtime>(
    app: AppHandle<R>,
    state: ShellState,
    active_command_bus: ActiveCommandBus,
    generation: u64,
    mut events: tokio::sync::broadcast::Receiver<AppEvent>,
    projector_start_rx: tokio::sync::oneshot::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if projector_start_rx.await.is_err() {
            return;
        }

        tracing::debug!(
            event = "shell_state_projector_started",
            generation = generation,
            "shell-state projector started"
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
                            if apply_projector_event(&state, generation, &active_command_bus, &app_event).await.was_applied() {
                                dirty = true;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
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
    active_command_bus: &ActiveCommandBus,
    event: &AppEvent,
) -> ProjectionOutcome {
    match event {
        AppEvent::Lv1(event) => {
            if let Lv1Event::SceneListChanged(scenes) = event {
                let _ = state
                    .show
                    .scene_reconciliation_diagnostic(scenes.clone())
                    .await;
            }

            let outcome = state.apply_lv1_event_to_projection(generation, event).await;
            if !outcome.was_applied() {
                return outcome;
            }

            if matches!(event, Lv1Event::Disconnected { .. }) {
                state
                    .clear_runtime_handles(generation, active_command_bus)
                    .await;
            }
            ProjectionOutcome::Applied
        }
        AppEvent::Fade(event) => {
            state
                .apply_fade_event_to_projection(generation, event)
                .await
        }
        AppEvent::SceneRecall(_) => ProjectionOutcome::Ignored,
    }
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

    fn spawn_started_shell_state_projector<R: Runtime>(
        handle: AppHandle<R>,
        state: ShellState,
        active_command_bus: ActiveCommandBus,
        generation: u64,
        events: tokio::sync::broadcast::Receiver<AppEvent>,
    ) -> tokio::task::JoinHandle<()> {
        let (projector_start_tx, projector_start_rx) = tokio::sync::oneshot::channel();
        let projector = spawn_shell_state_projector(
            handle,
            state,
            active_command_bus,
            generation,
            events,
            projector_start_rx,
        );
        let _ = projector_start_tx.send(());
        projector
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
        state
            .show
            .replace_snapshot(advanced_show_control::show::types::ShowSnapshot {
                lockout: true,
                scene_configs: vec![advanced_show_control::show::types::SceneConfig {
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

        let snapshot = cue_scene_snapshot(state.clone(), "1::Verse".to_string())
            .await
            .unwrap();

        assert_eq!(snapshot.cued_scene_id, Some("1::Verse".to_string()));
        assert!(snapshot.lockout);
        assert!(snapshot.show_file_dirty);
    }

    #[tokio::test]
    async fn cue_scene_rejects_unknown_scene_id() {
        let state = ShellState::default();

        let err = cue_scene_snapshot(state.clone(), "99::Missing".to_string())
            .await
            .unwrap_err();

        assert_eq!(err, "Scene config not found");
        assert_eq!(state.snapshot().await.cued_scene_id, None);
    }

    fn scene_config(
        scene_index: i32,
        scene_name: &str,
    ) -> advanced_show_control::show::types::SceneConfig {
        advanced_show_control::show::types::SceneConfig {
            scene_id: advanced_show_control::show::types::scene_id(scene_index, scene_name),
            scene_index,
            scene_name: scene_name.to_string(),
            duration_ms: 0,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: Default::default(),
        }
    }

    async fn state_with_scene(lockout: bool) -> ShellState {
        let state = ShellState::default();
        state
            .show
            .replace_snapshot(advanced_show_control::show::types::ShowSnapshot {
                lockout,
                scene_configs: vec![scene_config(1, "Verse")],
                cued_scene_id: Some("1::Verse".to_string()),
            })
            .await;
        state
    }

    #[tokio::test]
    async fn recall_scene_blocks_when_lockout_is_enabled() {
        let state = state_with_scene(true).await;
        let active_command_bus = ActiveCommandBus::default();

        let err = recall_scene_snapshot(state, active_command_bus, "1::Verse".to_string())
            .await
            .unwrap_err();

        assert_eq!(err, "Recall blocked: lockout is enabled");
    }

    #[tokio::test]
    async fn recall_scene_blocks_without_lv1_state() {
        let state = state_with_scene(false).await;
        let active_command_bus = ActiveCommandBus::default();

        let err = recall_scene_snapshot(state, active_command_bus, "1::Verse".to_string())
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
        let projector = spawn_started_shell_state_projector(
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
            .install_runtime_handles(
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
        let projector = spawn_started_shell_state_projector(
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
            .begin_connection(
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
        let projector = spawn_started_shell_state_projector(
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
    async fn stale_runtime_install_does_not_emit_current_snapshot() {
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
            &ActiveCommandBus::default(),
        )
        .await
        .expect("stale install should return current snapshot to command caller");

        tokio::task::yield_now().await;

        assert!(observed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn connected_runtime_installs_scene_recall_fader_handle() {
        let app = mock_app();
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
}
