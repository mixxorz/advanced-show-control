use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use advanced_show_control::lv1::types::{ConnectionStatus, Lv1StateSnapshot};
use advanced_show_control::runtime::commands::AppCommandBus;
use advanced_show_control::show::handle::ShowStateHandle;
use advanced_show_control::show::types::ShowSnapshot;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::commands::ActiveCommandBus;
use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};

use super::view::{
    AppConnectionState, AppFadeState, AppLogEntry, AppViewState, ChannelSummary, LogSeverity,
    LogSource, SceneSummary,
};

pub(super) const MAX_LOGS: usize = 200;

#[derive(Default)]
pub struct RuntimeHandles {
    pub active_generation: u64,
    pub lv1: Option<advanced_show_control::lv1::handle::Lv1ActorHandle>,
    pub fade: Option<advanced_show_control::fade::handle::FadeEngineHandle>,
    pub command_bus: Option<AppCommandBus>,
    pub projector: Option<JoinHandle<()>>,
    pub scene_recall_fader: Option<JoinHandle<()>>,
}

/// Lock ordering: always acquire `inner` before `show.state` to avoid deadlocks.
#[derive(Clone)]
pub struct ShellState {
    pub handles: Arc<Mutex<RuntimeHandles>>,
    pub show: ShowStateHandle,
    pub(super) inner: Arc<Mutex<ShellInner>>,
}

#[derive(Default)]
pub(super) struct ShellInner {
    pub(super) generation: u64,
    pub(super) lv1_snapshot: Option<Lv1StateSnapshot>,
    pub(super) discovered_lv1_systems: Vec<DiscoveredLv1System>,
    pub(super) connected_lv1_identity: Option<Lv1SystemIdentity>,
    pub(super) pending_lv1_identity: Option<Lv1SystemIdentity>,
    pub(super) reconnect_state: ReconnectState,
    pub(super) fade_state: AppFadeState,
    pub(super) selected_scene_id: Option<String>,
    pub(super) show_file_path: Option<PathBuf>,
    pub(super) show_file_dirty: bool,
    pub(super) show_file_last_saved_at: Option<String>,
    pub(super) logs: VecDeque<AppLogEntry>,
    pub(super) next_log_id: u64,
    pub(super) last_event_at: Option<String>,
    pub(super) snapshot_counter: AtomicU64,
}

impl Default for ShellState {
    fn default() -> Self {
        cover_state_variants();
        Self {
            handles: Arc::new(Mutex::new(RuntimeHandles::default())),
            show: ShowStateHandle::new_empty(),
            inner: Arc::new(Mutex::new(ShellInner::default())),
        }
    }
}

impl ShellState {
    pub async fn snapshot(&self) -> AppViewState {
        let inner_guard = self.inner.lock().await;
        let inner = snapshot_inner(&inner_guard);
        // +1 so versions start at 1: the UI starts at 0 and drops snapshots
        // that are not strictly newer.
        let state_version = inner_guard.snapshot_counter.fetch_add(1, Ordering::Relaxed) + 1;
        drop(inner_guard);
        let show = self.show.get_snapshot().await;
        snapshot_from_parts(inner, show, state_version)
    }

    pub async fn snapshot_for_generation(&self, generation: u64) -> Option<AppViewState> {
        let inner_guard = self.inner.lock().await;
        if inner_guard.generation != generation {
            return None;
        }

        let inner = snapshot_inner(&inner_guard);
        let state_version = inner_guard.snapshot_counter.fetch_add(1, Ordering::Relaxed) + 1;
        drop(inner_guard);
        let show = self.show.get_snapshot().await;
        Some(snapshot_from_parts(inner, show, state_version))
    }

    pub async fn set_scene_duration_ms(
        &self,
        scene_id: String,
        duration_ms: u64,
    ) -> Result<AppViewState, String> {
        let changed = self.show.set_scene_duration(scene_id, duration_ms).await?;
        if changed {
            self.inner.lock().await.show_file_dirty = true;
        }
        Ok(self.snapshot().await)
    }

    pub async fn select_scene_config(&self, scene_id: String) -> Result<AppViewState, String> {
        let mut inner = self.inner.lock().await;
        inner.selected_scene_id = Some(scene_id);
        drop(inner);
        Ok(self.snapshot().await)
    }

    pub async fn store_scene_config(&self, scene_id: String) -> Result<AppViewState, String> {
        let lv1 = self
            .inner
            .lock()
            .await
            .lv1_snapshot
            .clone()
            .ok_or_else(|| "Open a show file after LV1 scenes are loaded".to_string())?;
        // Validate that the scene_id matches a scene in the current show (reconciled from LV1)
        let show_snapshot = self.show.get_snapshot().await;
        if !show_snapshot
            .scene_configs
            .iter()
            .any(|scene| scene.scene_id == scene_id)
        {
            return Err("Scene config not found".to_string());
        }
        let changed = self.show.store_scene_config(scene_id, lv1.channels).await?;
        if changed {
            self.inner.lock().await.show_file_dirty = true;
        }
        Ok(self.snapshot().await)
    }

    pub async fn set_channel_scoped(
        &self,
        scene_id: String,
        group: i32,
        channel: i32,
        scoped: bool,
    ) -> Result<AppViewState, String> {
        let changed = self
            .show
            .set_channel_scoped(scene_id, group, channel, scoped)
            .await?;
        if changed {
            self.inner.lock().await.show_file_dirty = true;
        }
        Ok(self.snapshot().await)
    }

    pub async fn set_all_channels_scoped(
        &self,
        scene_id: String,
        scoped: bool,
    ) -> Result<AppViewState, String> {
        let changed = self.show.set_all_channels_scoped(scene_id, scoped).await?;
        if changed {
            self.inner.lock().await.show_file_dirty = true;
        }
        Ok(self.snapshot().await)
    }

    pub async fn set_scene_scope_faders_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<AppViewState, String> {
        let changed = self
            .show
            .set_scene_scope_faders_enabled(scene_id, enabled)
            .await?;
        if changed {
            self.inner.lock().await.show_file_dirty = true;
        }
        Ok(self.snapshot().await)
    }

    pub async fn set_scene_scope_pan_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<AppViewState, String> {
        let changed = self
            .show
            .set_scene_scope_pan_enabled(scene_id, enabled)
            .await?;
        if changed {
            self.inner.lock().await.show_file_dirty = true;
        }
        Ok(self.snapshot().await)
    }

    #[cfg(test)]
    pub async fn set_connected_lv1_identity(
        &self,
        identity: Option<Lv1SystemIdentity>,
    ) -> AppViewState {
        let mut inner = self.inner.lock().await;
        inner.connected_lv1_identity = identity;
        refresh_discovered_statuses(&mut inner);
        drop(inner);
        self.snapshot().await
    }

    pub async fn establish_connected_lv1_identity_for_generation(
        &self,
        generation: u64,
        identity: Lv1SystemIdentity,
    ) -> Option<AppViewState> {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return None;
        }
        if !matches!(
            inner
                .lv1_snapshot
                .as_ref()
                .map(|snapshot| &snapshot.connection),
            Some(ConnectionStatus::Connected)
        ) {
            return None;
        }

        inner.connected_lv1_identity = Some(identity);
        inner.pending_lv1_identity = None;
        refresh_discovered_statuses(&mut inner);
        drop(inner);
        self.snapshot_for_generation(generation).await
    }

    #[cfg(test)]
    pub async fn set_pending_lv1_identity(
        &self,
        identity: Option<Lv1SystemIdentity>,
    ) -> AppViewState {
        let mut inner = self.inner.lock().await;
        inner.pending_lv1_identity = identity;
        refresh_discovered_statuses(&mut inner);
        drop(inner);
        self.snapshot().await
    }

    pub async fn connected_lv1_identity(&self) -> Option<Lv1SystemIdentity> {
        self.inner.lock().await.connected_lv1_identity.clone()
    }

    pub async fn set_pending_lv1_identity_for_generation(
        &self,
        generation: u64,
        identity: Option<Lv1SystemIdentity>,
    ) -> Option<AppViewState> {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return None;
        }

        inner.pending_lv1_identity = identity;
        refresh_discovered_statuses(&mut inner);
        drop(inner);
        self.snapshot_for_generation(generation).await
    }

    pub async fn clear_pending_lv1_identity_for_generation(
        &self,
        generation: u64,
    ) -> Option<AppViewState> {
        self.set_pending_lv1_identity_for_generation(generation, None)
            .await
    }

    pub async fn fail_connect_for_generation(
        &self,
        generation: u64,
        message: impl Into<String>,
    ) -> Option<AppViewState> {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return None;
        }

        inner.lv1_snapshot = None;
        inner.pending_lv1_identity = None;
        inner.connected_lv1_identity = None;
        refresh_discovered_statuses(&mut inner);
        inner.push_log(LogSource::App, LogSeverity::Warning, message.into());
        drop(inner);
        self.snapshot_for_generation(generation).await
    }

    pub async fn fail_reconnect_for_generation(
        &self,
        generation: u64,
        message: impl Into<String>,
    ) -> Option<AppViewState> {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return None;
        }

        inner.lv1_snapshot = None;
        inner.pending_lv1_identity = None;
        refresh_discovered_statuses(&mut inner);
        inner.push_log(LogSource::App, LogSeverity::Warning, message.into());
        drop(inner);
        self.snapshot_for_generation(generation).await
    }

    pub async fn set_discovered_lv1_systems(
        &self,
        systems: Vec<DiscoveredLv1System>,
    ) -> AppViewState {
        let mut inner = self.inner.lock().await;
        inner.discovered_lv1_systems = systems;
        refresh_discovered_statuses(&mut inner);
        drop(inner);
        self.snapshot().await
    }

    pub async fn push_log(&self, source: LogSource, severity: LogSeverity, message: String) {
        let mut inner = self.inner.lock().await;
        inner.push_log(source, severity, message);
    }

    #[cfg(test)]
    pub async fn set_reconnect_active(&self, active: bool) -> AppViewState {
        let mut inner = self.inner.lock().await;
        if active {
            inner.reconnect_state.attempt = inner.reconnect_state.attempt.saturating_add(1);
        }
        inner.reconnect_state.active = active;
        drop(inner);
        self.snapshot().await
    }

    pub async fn reconnect_timed_out(&self, attempt: u64) -> AppViewState {
        let mut inner = self.inner.lock().await;
        if inner.reconnect_state.active && inner.reconnect_state.attempt == attempt {
            inner.generation = inner.generation.saturating_add(1);
            inner.reconnect_state.active = false;
        }
        drop(inner);
        self.snapshot().await
    }

    pub async fn reconnect_timeout_generation(&self, attempt: u64) -> Option<u64> {
        let inner = self.inner.lock().await;
        if inner.reconnect_state.active && inner.reconnect_state.attempt == attempt {
            Some(inner.generation)
        } else {
            None
        }
    }

    pub async fn clear_runtime_handles_for_generation(
        &self,
        generation: u64,
        active_command_bus: &ActiveCommandBus,
    ) {
        let inner = self.inner.lock().await;
        if inner.generation != generation {
            return;
        }

        let mut handles = self.handles.lock().await;
        handles.abort_all().await;
        active_command_bus.set(None).await;
    }

    pub async fn abort_current_runtime(&self, active_command_bus: &ActiveCommandBus) {
        let mut handles = self.handles.lock().await;
        handles.abort_all().await;
        active_command_bus.set(None).await;
    }

    pub async fn clear_runtime_handles_with_active_generation(
        &self,
        generation: u64,
        active_command_bus: &ActiveCommandBus,
    ) {
        let mut handles = self.handles.lock().await;
        if handles.active_generation != generation {
            return;
        }

        handles.abort_all().await;
        active_command_bus.set(None).await;
    }

    pub async fn install_runtime_handles_for_generation(
        &self,
        generation: u64,
        mut next: RuntimeHandles,
        active_command_bus: &ActiveCommandBus,
    ) -> Result<(), RuntimeHandles> {
        let current_generation = { self.inner.lock().await.generation };
        if current_generation != generation {
            next.abort_all().await;
            return Err(next);
        }

        active_command_bus.set(next.command_bus.clone()).await;
        let mut handles = self.handles.lock().await;
        handles.abort_all().await;
        next.active_generation = generation;
        *handles = next;
        Ok(())
    }
}

pub(super) fn refresh_discovered_statuses(inner: &mut ShellInner) {
    let connected_identity_is_live = matches!(
        inner
            .lv1_snapshot
            .as_ref()
            .map(|snapshot| &snapshot.connection),
        Some(ConnectionStatus::Connected)
    );
    for system in &mut inner.discovered_lv1_systems {
        system.status = if connected_identity_is_live
            && Some(&system.identity) == inner.connected_lv1_identity.as_ref()
        {
            crate::connection_state::DiscoveredLv1Status::Connected
        } else if Some(&system.identity) == inner.pending_lv1_identity.as_ref() {
            crate::connection_state::DiscoveredLv1Status::Connecting
        } else {
            crate::connection_state::DiscoveredLv1Status::Available
        };
    }
}

fn cover_state_variants() {
    let discovery_entry = advanced_show_control::lv1::discovery::DiscoveryEntry {
        service: "_waveslv113._tcp".to_string(),
        uuid: Some("uuid".to_string()),
        host: Some("LV1".to_string()),
        port: Some(50000),
        addresses: vec!["192.168.1.35".to_string()],
        ipv6: Vec::new(),
        source: "192.168.1.35".to_string(),
    };

    let _ = (
        LogSource::Fade,
        LogSeverity::Error,
        AppFadeState::Running,
        AppFadeState::Blocked,
        crate::connection_state::DiscoveredLv1Status::Available,
        crate::connection_state::DiscoveredLv1Status::Connecting,
        crate::connection_state::DiscoveredLv1Status::Connected,
        crate::connection_state::DiscoveredLv1Status::Unavailable,
        crate::connection_state::identity_from_discovery(&discovery_entry),
    );
}

impl RuntimeHandles {
    pub async fn abort_all(&mut self) {
        if let Some(command_bus) = self.command_bus.clone() {
            command_bus.clear_targets().await;
        }
        if let Some(projector) = self.projector.take() {
            projector.abort();
        }
        if let Some(scene_recall_fader) = self.scene_recall_fader.take() {
            scene_recall_fader.abort();
        }
        self.active_generation = 0;
        self.lv1 = None;
        self.fade = None;
        self.command_bus = None;
    }
}

fn snapshot_inner(inner: &ShellInner) -> InnerSnapshot {
    let connection = inner
        .lv1_snapshot
        .as_ref()
        .map(|snapshot| match snapshot.connection {
            ConnectionStatus::Connecting => AppConnectionState::Connecting,
            ConnectionStatus::Connected => AppConnectionState::Connected,
            ConnectionStatus::Disconnected => AppConnectionState::Disconnected,
        })
        .unwrap_or(AppConnectionState::Disconnected);

    let current_scene = inner.lv1_snapshot.as_ref().and_then(|snapshot| {
        snapshot.scene.as_ref().map(|scene| SceneSummary {
            index: scene.index,
            name: scene.name.clone(),
        })
    });

    let scenes = inner
        .lv1_snapshot
        .as_ref()
        .map(|snapshot| {
            snapshot
                .scene_list
                .iter()
                .map(|scene| SceneSummary {
                    index: scene.index,
                    name: scene.name.clone(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let channel_count = inner
        .lv1_snapshot
        .as_ref()
        .map(|snapshot| snapshot.channels.len())
        .unwrap_or(0);

    let channels = inner
        .lv1_snapshot
        .as_ref()
        .map(|snapshot| {
            snapshot
                .channels
                .iter()
                .map(|channel| ChannelSummary {
                    group: channel.group,
                    channel: channel.channel,
                    name: channel.name.clone(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    InnerSnapshot {
        connection,
        discovered_lv1_systems: inner.discovered_lv1_systems.clone(),
        connected_lv1_identity: inner.connected_lv1_identity.clone(),
        pending_lv1_identity: inner.pending_lv1_identity.clone(),
        reconnect: inner.reconnect_state.clone(),
        current_scene,
        scene_count: scenes.len(),
        scenes,
        channel_count,
        channels,
        fade_state: inner.fade_state.clone(),
        selected_scene_id: inner.selected_scene_id.clone(),
        show_file_name: inner
            .show_file_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .unwrap_or_else(|| "Untitled Show".to_string()),
        show_file_path: inner
            .show_file_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        show_file_dirty: inner.show_file_dirty,
        show_file_last_saved_at: inner.show_file_last_saved_at.clone(),
        logs: inner.logs.iter().cloned().collect(),
        last_event_at: inner.last_event_at.clone(),
    }
}

struct InnerSnapshot {
    connection: AppConnectionState,
    discovered_lv1_systems: Vec<DiscoveredLv1System>,
    connected_lv1_identity: Option<Lv1SystemIdentity>,
    pending_lv1_identity: Option<Lv1SystemIdentity>,
    reconnect: ReconnectState,
    current_scene: Option<SceneSummary>,
    scenes: Vec<SceneSummary>,
    scene_count: usize,
    channel_count: usize,
    channels: Vec<ChannelSummary>,
    fade_state: AppFadeState,
    selected_scene_id: Option<String>,
    show_file_name: String,
    show_file_path: Option<String>,
    show_file_dirty: bool,
    show_file_last_saved_at: Option<String>,
    logs: Vec<AppLogEntry>,
    last_event_at: Option<String>,
}

fn snapshot_from_parts(
    inner: InnerSnapshot,
    show: ShowSnapshot,
    state_version: u64,
) -> AppViewState {
    AppViewState {
        connection: inner.connection,
        discovered_lv1_systems: inner.discovered_lv1_systems,
        connected_lv1_identity: inner.connected_lv1_identity,
        pending_lv1_identity: inner.pending_lv1_identity,
        reconnect: inner.reconnect,
        current_scene: inner.current_scene,
        scene_count: inner.scene_count,
        scenes: inner.scenes,
        channel_count: inner.channel_count,
        channels: inner.channels,
        fade_state: inner.fade_state,
        lockout: show.lockout,
        scene_configs: show.scene_configs,
        selected_scene_id: inner.selected_scene_id,
        show_file_name: inner.show_file_name,
        show_file_path: inner.show_file_path,
        show_file_dirty: inner.show_file_dirty,
        show_file_last_saved_at: inner.show_file_last_saved_at,
        logs: inner.logs,
        last_event_at: inner.last_event_at,
        state_version,
    }
}

pub(super) fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    millis.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use advanced_show_control::lv1::events::Lv1Event;
    use advanced_show_control::lv1::types::{ChannelInfo, SceneListEntry, SceneState};

    async fn store_intro_scene_config(state: &ShellState) {
        state
            .show
            .store_scene_config(
                "1::Intro".to_string(),
                vec![ChannelInfo {
                    group: 0,
                    channel: 1,
                    name: "Lead".to_string(),
                    gain_db: -6.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
            )
            .await
            .unwrap();
    }

    #[test]
    fn default_construction_does_not_require_tokio_runtime() {
        let _state = ShellState::default();
    }

    #[tokio::test]
    async fn default_snapshot_exposes_untitled_show_and_is_not_dirty() {
        let state = ShellState::default();
        let snapshot = state.snapshot().await;

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert_eq!(snapshot.current_scene, None);
        assert_eq!(snapshot.scene_count, 0);
        assert_eq!(snapshot.channel_count, 0);
        assert!(snapshot.channels.is_empty());
        assert_eq!(snapshot.fade_state, AppFadeState::Idle);
        assert!(!snapshot.lockout);
        assert!(snapshot.scene_configs.is_empty());
        assert_eq!(snapshot.selected_scene_id, None);
        assert_eq!(snapshot.show_file_name, "Untitled Show");
        assert_eq!(snapshot.show_file_path, None);
        assert!(!snapshot.show_file_dirty);
        assert_eq!(snapshot.show_file_last_saved_at, None);
    }

    #[tokio::test]
    async fn first_snapshot_state_version_is_greater_than_initial_ui_version() {
        let state = ShellState::default();

        // The UI starts at stateVersion 0 and drops snapshots that are not
        // strictly newer, so the very first snapshot must already exceed 0.
        let snapshot = state.snapshot().await;

        assert!(snapshot.state_version > 0);
    }

    #[tokio::test]
    async fn snapshot_state_versions_are_strictly_increasing() {
        let state = ShellState::default();

        let first = state.snapshot().await;
        let second = state.snapshot().await;

        assert!(second.state_version > first.state_version);
    }

    #[tokio::test]
    async fn lockout_is_owned_by_rust_state() {
        let state = ShellState::default();
        let snapshot = state.set_lockout(true).await;

        assert!(snapshot.lockout);
        assert_eq!(snapshot.logs.len(), 1);
        assert_eq!(snapshot.logs[0].message, "Lockout enabled");
    }

    #[tokio::test]
    async fn lockout_marks_show_file_dirty() {
        let state = ShellState::default();

        let snapshot = state.set_lockout(true).await;

        assert!(snapshot.show_file_dirty);
    }

    #[tokio::test]
    async fn stale_runtime_handle_installation_is_rejected() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;

        let current_handles = RuntimeHandles {
            active_generation: 0,
            lv1: None,
            fade: None,
            command_bus: None,
            projector: Some(tokio::spawn(async {})),
            scene_recall_fader: None,
        };

        let active_command_bus = crate::commands::ActiveCommandBus::default();

        match state
            .install_runtime_handles_for_generation(
                generation,
                current_handles,
                &active_command_bus,
            )
            .await
        {
            Ok(()) => {}
            Err(_) => panic!("expected current generation install to succeed"),
        }

        let stale_handles = RuntimeHandles {
            active_generation: 0,
            lv1: None,
            fade: None,
            command_bus: None,
            projector: Some(tokio::spawn(async {})),
            scene_recall_fader: None,
        };

        let rejected = state
            .install_runtime_handles_for_generation(
                generation.saturating_sub(1),
                stale_handles,
                &active_command_bus,
            )
            .await;

        assert!(rejected.is_err());
        let handles = state.handles.lock().await;
        assert_eq!(handles.active_generation, generation);
    }

    #[tokio::test]
    async fn replacement_connect_cleanup_aborts_existing_runtime_and_clears_command_bus() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        let active_command_bus = crate::commands::ActiveCommandBus::default();
        let command_bus =
            AppCommandBus::new(advanced_show_control::runtime::events::AppEventBus::default());

        let installed = state
            .install_runtime_handles_for_generation(
                generation,
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

        let _ = state.begin_connecting().await;
        state.abort_current_runtime(&active_command_bus).await;

        assert!(active_command_bus.current().await.is_none());
        let handles = state.handles.lock().await;
        assert_eq!(handles.active_generation, 0);
        assert!(handles.command_bus.is_none());
        assert!(handles.projector.is_none());
        assert!(handles.scene_recall_fader.is_none());
    }

    #[tokio::test]
    async fn matching_reconnect_timeout_invalidates_current_generation() {
        let state = ShellState::default();
        state
            .set_connected_lv1_identity(Some(crate::connection_state::Lv1SystemIdentity {
                uuid: Some("uuid-1".to_string()),
                host: Some("LV1-FOH".to_string()),
                address: "192.168.1.35".to_string(),
                port: 50000,
            }))
            .await;
        let (generation, _) = state.begin_connecting().await;
        let reconnecting = state
            .apply_lv1_event_for_generation(generation, &Lv1Event::Disconnected)
            .await
            .expect("disconnect should enter reconnect state");

        let snapshot = state
            .reconnect_timed_out(reconnecting.reconnect.attempt)
            .await;

        assert!(!snapshot.reconnect.active);
        assert!(state.snapshot_for_generation(generation).await.is_none());
        assert!(
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
                .is_none()
        );
    }

    #[tokio::test]
    async fn stale_reconnect_timeout_does_not_invalidate_newer_generation() {
        let state = ShellState::default();
        state
            .set_connected_lv1_identity(Some(crate::connection_state::Lv1SystemIdentity {
                uuid: Some("uuid-1".to_string()),
                host: Some("LV1-FOH".to_string()),
                address: "192.168.1.35".to_string(),
                port: 50000,
            }))
            .await;
        let (generation, _) = state.begin_connecting().await;
        let first_reconnect = state
            .apply_lv1_event_for_generation(generation, &Lv1Event::Disconnected)
            .await
            .expect("first disconnect should enter reconnect state");
        state
            .apply_lv1_event_for_generation(generation, &Lv1Event::Connected)
            .await
            .expect("connected event should clear first reconnect state");
        let second_reconnect = state
            .apply_lv1_event_for_generation(generation, &Lv1Event::Disconnected)
            .await
            .expect("second disconnect should enter reconnect state");

        let snapshot = state
            .reconnect_timed_out(first_reconnect.reconnect.attempt)
            .await;

        assert!(snapshot.reconnect.active);
        assert_eq!(
            snapshot.reconnect.attempt,
            second_reconnect.reconnect.attempt
        );
        assert!(state.snapshot_for_generation(generation).await.is_some());
    }

    #[tokio::test]
    async fn snapshot_maps_lv1_scene_and_counts() {
        let state = ShellState::default();

        let snapshot = state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: Some(SceneState {
                    index: 3,
                    name: "Verse".to_string(),
                }),
                scene_list: vec![SceneListEntry {
                    index: 3,
                    name: "Verse".to_string(),
                }],
                channels: vec![ChannelInfo {
                    group: 0,
                    channel: 0,
                    name: "Lead".to_string(),
                    gain_db: -6.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
            })
            .await;

        assert_eq!(snapshot.connection, AppConnectionState::Connected);
        assert_eq!(snapshot.current_scene.unwrap().name, "Verse");
        assert_eq!(snapshot.scene_count, 1);
        assert_eq!(snapshot.channel_count, 1);
        assert_eq!(snapshot.channels.len(), 1);
        assert_eq!(snapshot.channels[0].group, 0);
        assert_eq!(snapshot.channels[0].channel, 0);
        assert_eq!(snapshot.channels[0].name, "Lead");
        assert_eq!(snapshot.scene_configs.len(), 1);
        assert_eq!(snapshot.scene_configs[0].scene_id, "3::Verse");
        assert_eq!(snapshot.selected_scene_id, Some("3::Verse".to_string()));
    }

    #[tokio::test]
    async fn snapshot_includes_discovered_lv1_systems_and_reconnect_state() {
        let state = ShellState::default();
        let identity = crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };

        state
            .set_connected_lv1_identity(Some(identity.clone()))
            .await;
        state.set_reconnect_active(true).await;
        let snapshot = state
            .set_discovered_lv1_systems(vec![crate::connection_state::DiscoveredLv1System {
                identity,
                latency_ms: Some(12),
                status: crate::connection_state::DiscoveredLv1Status::Available,
            }])
            .await;

        assert_eq!(snapshot.discovered_lv1_systems.len(), 1);
        assert_eq!(
            snapshot.discovered_lv1_systems[0].identity.address,
            "192.168.1.35"
        );
        assert_eq!(
            snapshot.connected_lv1_identity.unwrap().address,
            "192.168.1.35"
        );
        assert!(snapshot.reconnect.active);
    }

    #[tokio::test]
    async fn set_discovered_lv1_systems_marks_connected_and_pending_rows() {
        let state = ShellState::default();
        let connected = crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };
        let pending = crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-2".to_string()),
            host: Some("LV1-MON".to_string()),
            address: "192.168.1.36".to_string(),
            port: 50000,
        };
        state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: Vec::new(),
                channels: Vec::new(),
            })
            .await;
        state
            .set_connected_lv1_identity(Some(connected.clone()))
            .await;
        state.set_pending_lv1_identity(Some(pending.clone())).await;

        let snapshot = state
            .set_discovered_lv1_systems(vec![
                crate::connection_state::DiscoveredLv1System {
                    identity: connected,
                    latency_ms: Some(10),
                    status: crate::connection_state::DiscoveredLv1Status::Available,
                },
                crate::connection_state::DiscoveredLv1System {
                    identity: pending,
                    latency_ms: Some(20),
                    status: crate::connection_state::DiscoveredLv1Status::Available,
                },
            ])
            .await;

        assert_eq!(
            snapshot.discovered_lv1_systems[0].status,
            crate::connection_state::DiscoveredLv1Status::Connected
        );
        assert_eq!(
            snapshot.discovered_lv1_systems[1].status,
            crate::connection_state::DiscoveredLv1Status::Connecting
        );
    }

    #[tokio::test]
    async fn disconnected_snapshot_does_not_mark_discovered_row_connected() {
        let state = ShellState::default();
        let connected = crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };
        state
            .set_connected_lv1_identity(Some(connected.clone()))
            .await;

        let snapshot = state
            .set_discovered_lv1_systems(vec![crate::connection_state::DiscoveredLv1System {
                identity: connected,
                latency_ms: Some(10),
                status: crate::connection_state::DiscoveredLv1Status::Available,
            }])
            .await;

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert_ne!(
            snapshot.discovered_lv1_systems[0].status,
            crate::connection_state::DiscoveredLv1Status::Connected
        );
    }

    #[tokio::test]
    async fn stale_generation_cannot_establish_connected_or_clear_current_pending_identity() {
        let state = ShellState::default();
        let stale_identity = crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };
        let current_identity = crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-2".to_string()),
            host: Some("LV1-MON".to_string()),
            address: "192.168.1.36".to_string(),
            port: 50000,
        };

        let (stale_generation, _) = state.begin_connecting().await;
        let (current_generation, _) = state.begin_connecting().await;
        state
            .set_pending_lv1_identity_for_generation(
                current_generation,
                Some(current_identity.clone()),
            )
            .await
            .expect("current generation should set pending identity");

        let stale_establish = state
            .establish_connected_lv1_identity_for_generation(stale_generation, stale_identity)
            .await;
        let stale_clear = state
            .clear_pending_lv1_identity_for_generation(stale_generation)
            .await;
        let snapshot = state.snapshot().await;

        assert!(stale_establish.is_none());
        assert!(stale_clear.is_none());
        assert_eq!(snapshot.connected_lv1_identity, None);
        assert_eq!(snapshot.pending_lv1_identity, Some(current_identity));
    }

    #[tokio::test]
    async fn cannot_establish_connected_identity_until_lv1_snapshot_is_connected() {
        let state = ShellState::default();
        let identity = crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };
        let (generation, _) = state.begin_connecting().await;

        let snapshot = state
            .establish_connected_lv1_identity_for_generation(generation, identity)
            .await;

        assert!(snapshot.is_none());
        assert_eq!(state.snapshot().await.connected_lv1_identity, None);
    }

    #[tokio::test]
    async fn established_connected_identity_snapshot_includes_show_configs() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        store_intro_scene_config(&state).await;
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
            .unwrap();

        let snapshot = state
            .establish_connected_lv1_identity_for_generation(
                generation,
                crate::connection_state::Lv1SystemIdentity {
                    uuid: Some("uuid-1".to_string()),
                    host: Some("LV1-FOH".to_string()),
                    address: "192.168.1.35".to_string(),
                    port: 50000,
                },
            )
            .await
            .expect("current connected generation should establish identity");

        assert_eq!(snapshot.scene_configs.len(), 1);
        assert_eq!(snapshot.scene_configs[0].scene_id, "1::Intro");
    }

    #[tokio::test]
    async fn pending_identity_snapshot_includes_show_configs() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        store_intro_scene_config(&state).await;

        let snapshot = state
            .set_pending_lv1_identity_for_generation(
                generation,
                Some(crate::connection_state::Lv1SystemIdentity {
                    uuid: Some("uuid-1".to_string()),
                    host: Some("LV1-FOH".to_string()),
                    address: "192.168.1.35".to_string(),
                    port: 50000,
                }),
            )
            .await
            .expect("current generation should set pending identity");

        assert_eq!(snapshot.scene_configs.len(), 1);
        assert_eq!(snapshot.scene_configs[0].scene_id, "1::Intro");
    }

    #[tokio::test]
    async fn connect_failure_snapshot_includes_show_configs() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        store_intro_scene_config(&state).await;

        let snapshot = state
            .fail_connect_for_generation(generation, "LV1 did not connect")
            .await
            .expect("current generation failure should apply");

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert_eq!(snapshot.scene_configs.len(), 1);
        assert_eq!(snapshot.scene_configs[0].scene_id, "1::Intro");
    }

    #[tokio::test]
    async fn discovered_systems_snapshot_includes_show_configs() {
        let state = ShellState::default();
        store_intro_scene_config(&state).await;

        let snapshot = state
            .set_discovered_lv1_systems(vec![crate::connection_state::DiscoveredLv1System {
                identity: crate::connection_state::Lv1SystemIdentity {
                    uuid: Some("uuid-1".to_string()),
                    host: Some("LV1-FOH".to_string()),
                    address: "192.168.1.35".to_string(),
                    port: 50000,
                },
                latency_ms: Some(10),
                status: crate::connection_state::DiscoveredLv1Status::Available,
            }])
            .await;

        assert_eq!(snapshot.scene_configs.len(), 1);
        assert_eq!(snapshot.scene_configs[0].scene_id, "1::Intro");
    }

    #[tokio::test]
    async fn current_generation_connect_failure_clears_connecting_state_and_pending_identity() {
        let state = ShellState::default();
        let identity = crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };
        let (generation, _) = state.begin_connecting().await;
        state
            .set_pending_lv1_identity_for_generation(generation, Some(identity))
            .await
            .expect("current generation should set pending identity");

        let snapshot = state
            .fail_connect_for_generation(generation, "LV1 did not connect")
            .await
            .expect("current generation failure should apply");

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert_eq!(snapshot.pending_lv1_identity, None);
        assert_eq!(snapshot.connected_lv1_identity, None);
        assert_eq!(snapshot.logs.last().unwrap().severity, LogSeverity::Warning);
        assert_eq!(snapshot.logs.last().unwrap().message, "LV1 did not connect");
    }

    #[tokio::test]
    async fn reconnect_failure_preserves_connected_identity_for_next_uuid_match() {
        let state = ShellState::default();
        let identity = crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };
        state
            .set_connected_lv1_identity(Some(identity.clone()))
            .await;
        state.set_reconnect_active(true).await;
        let (generation, _) = state.begin_connecting().await;
        state
            .set_pending_lv1_identity_for_generation(generation, Some(identity.clone()))
            .await
            .expect("current generation should set pending identity");

        let snapshot = state
            .fail_reconnect_for_generation(generation, "LV1 did not connect")
            .await
            .expect("current generation reconnect failure should apply");

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert_eq!(snapshot.pending_lv1_identity, None);
        assert_eq!(snapshot.connected_lv1_identity, Some(identity.clone()));
        assert_eq!(state.connected_lv1_identity().await, Some(identity));
        assert!(snapshot.reconnect.active);
    }

    #[tokio::test]
    async fn stale_generation_connect_failure_does_not_clear_newer_pending_identity() {
        let state = ShellState::default();
        let current_identity = crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-2".to_string()),
            host: Some("LV1-MON".to_string()),
            address: "192.168.1.36".to_string(),
            port: 50000,
        };

        let (stale_generation, _) = state.begin_connecting().await;
        let (current_generation, _) = state.begin_connecting().await;
        state
            .set_pending_lv1_identity_for_generation(
                current_generation,
                Some(current_identity.clone()),
            )
            .await
            .expect("current generation should set pending identity");

        let stale = state
            .fail_connect_for_generation(stale_generation, "LV1 did not connect")
            .await;
        let snapshot = state.snapshot().await;

        assert!(stale.is_none());
        assert_eq!(snapshot.connection, AppConnectionState::Connecting);
        assert_eq!(snapshot.pending_lv1_identity, Some(current_identity));
    }

    #[test]
    fn enum_variants_are_kept_for_state_space_coverage() {
        let _ = (
            LogSource::Fade,
            LogSeverity::Error,
            AppFadeState::Running,
            AppFadeState::Blocked,
        );
    }
}
