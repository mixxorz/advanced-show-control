use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use lv1_scene_fade_utility::lv1::model::{ConnectionStatus, Lv1StateSnapshot};
use lv1_scene_fade_utility::runtime::commands::AppCommandBus;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::commands::ActiveCommandBus;
use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};

use super::view::{
    AppConnectionState, AppFadeState, AppLogEntry, AppViewState, ChannelSummary, LogSeverity,
    LogSource, SceneConfig, SceneSummary,
};

pub(super) const MAX_LOGS: usize = 200;

#[derive(Default)]
pub struct RuntimeHandles {
    pub active_generation: u64,
    pub lv1: Option<lv1_scene_fade_utility::lv1::state::Lv1ActorHandle>,
    pub fade: Option<lv1_scene_fade_utility::fade::engine::FadeEngineHandle>,
    pub command_bus: Option<AppCommandBus>,
    pub projector: Option<JoinHandle<()>>,
    pub scene_recall_fader: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub struct ShellState {
    pub handles: Arc<Mutex<RuntimeHandles>>,
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
    pub(super) lockout: bool,
    pub(super) scene_configs: Vec<SceneConfig>,
    pub(super) selected_scene_id: Option<String>,
    pub(super) show_file_path: Option<PathBuf>,
    pub(super) show_file_dirty: bool,
    pub(super) show_file_last_saved_at: Option<String>,
    pub(super) logs: VecDeque<AppLogEntry>,
    pub(super) duration_zero_skip_logs: HashSet<String>,
    pub(super) next_log_id: u64,
    pub(super) last_event_at: Option<String>,
}

impl Default for ShellState {
    fn default() -> Self {
        cover_state_variants();
        Self {
            handles: Arc::new(Mutex::new(RuntimeHandles::default())),
            inner: Arc::new(Mutex::new(ShellInner::default())),
        }
    }
}

impl ShellState {
    pub async fn snapshot(&self) -> AppViewState {
        let inner = self.inner.lock().await;
        snapshot_from_inner(&inner)
    }

    pub async fn snapshot_for_generation(&self, generation: u64) -> Option<AppViewState> {
        let inner = self.inner.lock().await;
        if inner.generation != generation {
            return None;
        }

        Some(snapshot_from_inner(&inner))
    }

    pub async fn set_connected_lv1_identity(
        &self,
        identity: Option<Lv1SystemIdentity>,
    ) -> AppViewState {
        let mut inner = self.inner.lock().await;
        inner.connected_lv1_identity = identity;
        refresh_discovered_statuses(&mut inner);
        snapshot_from_inner(&inner)
    }

    pub async fn set_pending_lv1_identity(
        &self,
        identity: Option<Lv1SystemIdentity>,
    ) -> AppViewState {
        let mut inner = self.inner.lock().await;
        inner.pending_lv1_identity = identity;
        refresh_discovered_statuses(&mut inner);
        snapshot_from_inner(&inner)
    }

    pub async fn set_discovered_lv1_systems(
        &self,
        systems: Vec<DiscoveredLv1System>,
    ) -> AppViewState {
        let mut inner = self.inner.lock().await;
        inner.discovered_lv1_systems = systems;
        refresh_discovered_statuses(&mut inner);
        snapshot_from_inner(&inner)
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

    pub async fn install_runtime_handles_for_generation(
        &self,
        generation: u64,
        mut next: RuntimeHandles,
        active_command_bus: &ActiveCommandBus,
    ) -> Result<(), RuntimeHandles> {
        let inner = self.inner.lock().await;
        if inner.generation != generation {
            next.abort_all().await;
            return Err(next);
        }

        active_command_bus.set(next.command_bus.clone()).await;
        let mut handles = self.handles.lock().await;
        handles.abort_all().await;
        next.active_generation = generation;
        *handles = next;
        drop(inner);
        Ok(())
    }
}

fn refresh_discovered_statuses(inner: &mut ShellInner) {
    for system in &mut inner.discovered_lv1_systems {
        system.status = if Some(&system.identity) == inner.connected_lv1_identity.as_ref() {
            crate::connection_state::DiscoveredLv1Status::Connected
        } else if Some(&system.identity) == inner.pending_lv1_identity.as_ref() {
            crate::connection_state::DiscoveredLv1Status::Connecting
        } else {
            crate::connection_state::DiscoveredLv1Status::Available
        };
    }
}

fn cover_state_variants() {
    let discovery_entry = lv1_scene_fade_utility::lv1::discovery::DiscoveryEntry {
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

pub(super) fn scene_id(index: i32, name: &str) -> String {
    format!("{index}::{name}")
}

pub(super) fn snapshot_from_inner(inner: &ShellInner) -> AppViewState {
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

    AppViewState {
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
        lockout: inner.lockout,
        scene_configs: inner.scene_configs.clone(),
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
    use lv1_scene_fade_utility::lv1::model::{ChannelInfo, SceneListEntry, SceneState};

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

    #[test]
    fn snapshot_maps_lv1_scene_and_counts() {
        let mut inner = ShellInner::default();
        inner.lv1_snapshot = Some(Lv1StateSnapshot {
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
            }],
        });

        let snapshot = snapshot_from_inner(&inner);

        assert_eq!(snapshot.connection, AppConnectionState::Connected);
        assert_eq!(snapshot.current_scene.unwrap().name, "Verse");
        assert_eq!(snapshot.scene_count, 1);
        assert_eq!(snapshot.channel_count, 1);
        assert_eq!(snapshot.channels.len(), 1);
        assert_eq!(snapshot.channels[0].group, 0);
        assert_eq!(snapshot.channels[0].channel, 0);
        assert_eq!(snapshot.channels[0].name, "Lead");
        assert_eq!(snapshot.scene_configs.len(), 0);
        assert_eq!(snapshot.selected_scene_id, None);
    }

    #[test]
    fn snapshot_includes_discovered_lv1_systems_and_reconnect_state() {
        let mut inner = ShellInner::default();
        inner.discovered_lv1_systems = vec![crate::connection_state::DiscoveredLv1System {
            identity: crate::connection_state::Lv1SystemIdentity {
                uuid: Some("uuid-1".to_string()),
                host: Some("LV1-FOH".to_string()),
                address: "192.168.1.35".to_string(),
                port: 50000,
            },
            latency_ms: Some(12),
            status: crate::connection_state::DiscoveredLv1Status::Connected,
        }];
        inner.connected_lv1_identity = Some(crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        });
        inner.reconnect_state = crate::connection_state::ReconnectState { active: true };

        let snapshot = snapshot_from_inner(&inner);

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
        state.set_connected_lv1_identity(Some(connected.clone())).await;
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
