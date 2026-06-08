use lv1_scene_fade_utility::fade::engine::spawn_engine;
use lv1_scene_fade_utility::lv1::discovery::resolve_target;
use lv1_scene_fade_utility::lv1::messages::Lv1Event;
use lv1_scene_fade_utility::lv1::state::spawn_actor;
use lv1_scene_fade_utility::runtime::commands::AppCommandBus;
use lv1_scene_fade_utility::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tokio::sync::Mutex;
use tokio::task::spawn_blocking;

use crate::app_state::{AppConnectionState, AppViewState, RuntimeHandles, ShellState};
use crate::scene_recall_fader::spawn_scene_recall_fader;
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
        lv1_scene_fade_utility::lv1::discovery::discover(
            lv1_scene_fade_utility::lv1::discovery::DiscoverOptions {
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
pub async fn finish_fade_now(
    active_command_bus: State<'_, ActiveCommandBus>,
) -> Result<(), String> {
    let command_bus = active_command_bus.current().await;
    let command_bus = command_bus.ok_or_else(|| "Fade runtime is unavailable".to_string())?;
    command_bus
        .finish_fade_now()
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
    let (host, port) = resolve_target(host, port, timeout).map_err(|err| err.to_string())?;
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
    let snapshot = refresh_lv1_discovery_snapshot(
        app.clone(),
        (*state).clone(),
        Some(DEFAULT_DISCOVERY_TIMEOUT_MS),
    )
    .await?;
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
) -> Result<AppViewState, String> {
    let event_bus = AppEventBus::default();
    let (generation, connecting_snapshot) = state.begin_connecting().await;
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
    command_bus.set_lv1(Some(lv1.clone())).await;
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
            shell_state.clone(),
            generation,
            fade_command_bus.clone(),
            event_bus.clone(),
        )),
    };

    let initial_snapshot = lv1.get_state().await;
    if initial_snapshot.connection
        != lv1_scene_fade_utility::lv1::model::ConnectionStatus::Connected
    {
        runtime_handles.abort_all().await;
        if let Some(snapshot) = state
            .fail_connect_for_generation(generation, "LV1 did not connect")
            .await
        {
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

fn spawn_shell_state_projector<R: Runtime>(
    app: AppHandle<R>,
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
                        if let Some(snapshot) = state
                            .apply_fade_event_for_generation(generation, &event)
                            .await
                        {
                            if let Err(err) = app.emit("app-status-changed", &snapshot) {
                                eprintln!("failed to emit app-status-changed: {err}");
                            }
                        }
                    }
                    AppEvent::CommandFailed { command, message } => {
                        eprintln!("command failed: {command}: {message}");
                    }
                    AppEvent::Automation(_) => {
                        if let Some(snapshot) = state.snapshot_for_generation(generation).await {
                            if let Err(err) = app.emit("app-status-changed", &snapshot) {
                                eprintln!("failed to emit app-status-changed: {err}");
                            }
                        }
                    }
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
    use lv1_scene_fade_utility::fade::types::FadeEvent;
    use lv1_scene_fade_utility::lv1::model::{ConnectionStatus, Lv1StateSnapshot};
    use lv1_scene_fade_utility::runtime::events::AutomationEvent;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tauri::{Listener, test::mock_app};

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

    #[test]
    fn connection_chooser_commands_are_exposed() {
        let _ = refresh_lv1_discovery;
        let _ = connect_lv1_system;
        let _ = startup_auto_connect_lv1;
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

    #[tokio::test]
    async fn initial_connection_snapshot_is_emitted_before_projector_events() {
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

        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if observed.lock().unwrap().len() >= 2 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("projector should emit the buffered event");

        let observed = observed.lock().unwrap();
        assert_eq!(observed.len(), 2);
        assert_eq!(observed[0]["fadeState"], "idle");
        assert_eq!(observed[1]["fadeState"], "running");
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

    #[tokio::test]
    async fn automation_event_emits_fresh_app_status_snapshot() {
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
        let projector =
            spawn_shell_state_projector(handle, state, generation, event_bus.subscribe());

        event_bus.publish(AppEvent::Automation(AutomationEvent::RuleTriggered {
            rule_id: "scene-recall-fader".to_string(),
        }));

        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if !observed.lock().unwrap().is_empty() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("projector should emit snapshot for automation refresh");

        projector.abort();
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
