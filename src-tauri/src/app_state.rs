use std::collections::VecDeque;
use std::sync::Arc;

use lv1_scene_fade_utility::lv1::state::{ConnectionStatus, Lv1Event, Lv1StateSnapshot};
use serde::Serialize;
use tokio::sync::Mutex;

const MAX_LOGS: usize = 200;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SceneSummary {
    pub index: i32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppLogEntry {
    pub id: u64,
    pub timestamp: String,
    pub source: LogSource,
    pub severity: LogSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LogSource {
    App,
    Lv1,
    Fade,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LogSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AppConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AppFadeState {
    Idle,
    Running,
    Blocked,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub connection: AppConnectionState,
    pub current_scene: Option<SceneSummary>,
    pub scenes: Vec<SceneSummary>,
    pub scene_count: usize,
    pub channel_count: usize,
    pub fade_state: AppFadeState,
    pub lockout: bool,
    pub logs: Vec<AppLogEntry>,
    pub last_event_at: Option<String>,
}

#[derive(Default)]
pub struct RuntimeHandles {
    pub lv1: Option<lv1_scene_fade_utility::lv1::state::Lv1ActorHandle>,
    pub fade: Option<lv1_scene_fade_utility::fade::engine::FadeEngineHandle>,
}

#[derive(Clone)]
pub struct ShellState {
    pub handles: Arc<Mutex<RuntimeHandles>>,
    inner: Arc<Mutex<ShellInner>>,
}

#[derive(Default)]
struct ShellInner {
    generation: u64,
    lv1_snapshot: Option<Lv1StateSnapshot>,
    fade_state: AppFadeState,
    lockout: bool,
    logs: VecDeque<AppLogEntry>,
    next_log_id: u64,
    last_event_at: Option<String>,
}

impl Default for AppFadeState {
    fn default() -> Self {
        Self::Idle
    }
}

fn cover_state_variants() {
    let _ = (
        LogSource::Fade,
        LogSeverity::Error,
        AppFadeState::Running,
        AppFadeState::Blocked,
    );
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
    pub async fn snapshot(&self) -> AppSnapshot {
        let inner = self.inner.lock().await;
        snapshot_from_inner(&inner)
    }

    pub async fn begin_connecting(&self) -> (u64, AppSnapshot) {
        let mut inner = self.inner.lock().await;
        inner.generation = inner.generation.saturating_add(1);
        inner.lv1_snapshot = Some(Lv1StateSnapshot {
            connection: ConnectionStatus::Connecting,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
        });
        inner.push_log(
            LogSource::Lv1,
            LogSeverity::Info,
            "Connecting to LV1".to_string(),
        );
        let generation = inner.generation;
        (generation, snapshot_from_inner(&inner))
    }

    pub async fn set_lockout(&self, enabled: bool) -> AppSnapshot {
        let mut inner = self.inner.lock().await;
        inner.lockout = enabled;
        inner.push_log(
            LogSource::App,
            LogSeverity::Info,
            format!("Lockout {}", if enabled { "enabled" } else { "disabled" }),
        );
        snapshot_from_inner(&inner)
    }

    pub async fn begin_connection(&self, snapshot: Lv1StateSnapshot) -> AppSnapshot {
        let mut inner = self.inner.lock().await;
        inner.lv1_snapshot = Some(snapshot);
        let message = match inner
            .lv1_snapshot
            .as_ref()
            .map(|snapshot| &snapshot.connection)
        {
            Some(ConnectionStatus::Connecting) => "Connecting to LV1",
            Some(ConnectionStatus::Connected) => "LV1 connected",
            Some(ConnectionStatus::Disconnected) => "LV1 disconnected",
            None => "LV1 disconnected",
        };
        inner.push_log(LogSource::Lv1, LogSeverity::Info, message.to_string());
        snapshot_from_inner(&inner)
    }

    pub async fn disconnect(&self) -> AppSnapshot {
        let mut inner = self.inner.lock().await;
        inner.generation = inner.generation.saturating_add(1);
        inner.lv1_snapshot = None;
        inner.push_log(
            LogSource::App,
            LogSeverity::Info,
            "Disconnected from LV1".to_string(),
        );
        snapshot_from_inner(&inner)
    }

    pub async fn apply_lv1_event_for_generation(
        &self,
        generation: u64,
        event: &Lv1Event,
    ) -> Option<AppSnapshot> {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return None;
        }

        match event {
            Lv1Event::Connected => {
                ensure_lv1_snapshot(&mut inner).connection = ConnectionStatus::Connected;
                inner.push_log(
                    LogSource::Lv1,
                    LogSeverity::Info,
                    "LV1 connected".to_string(),
                );
            }
            Lv1Event::Disconnected => {
                inner.lv1_snapshot = None;
                inner.push_log(
                    LogSource::Lv1,
                    LogSeverity::Warning,
                    "LV1 disconnected".to_string(),
                );
            }
            Lv1Event::SceneChanged(scene) => {
                ensure_lv1_snapshot(&mut inner).scene = Some(scene.clone());
                inner.push_log(
                    LogSource::Lv1,
                    LogSeverity::Info,
                    format!("Scene changed to {}: {}", scene.index, scene.name),
                );
            }
            Lv1Event::SceneListChanged(scenes) => {
                ensure_lv1_snapshot(&mut inner).scene_list = scenes.clone();
                inner.push_log(
                    LogSource::Lv1,
                    LogSeverity::Info,
                    format!("Scene list updated: {} scenes", scenes.len()),
                );
            }
            Lv1Event::FaderChanged {
                group,
                channel,
                gain_db,
            } => {
                if let Some(existing) = ensure_lv1_snapshot(&mut inner)
                    .channels
                    .iter_mut()
                    .find(|ch| ch.group == *group && ch.channel == *channel)
                {
                    existing.gain_db = *gain_db;
                }
            }
            Lv1Event::MuteChanged {
                group,
                channel,
                muted,
            } => {
                if let Some(existing) = ensure_lv1_snapshot(&mut inner)
                    .channels
                    .iter_mut()
                    .find(|ch| ch.group == *group && ch.channel == *channel)
                {
                    existing.muted = *muted;
                }
            }
            Lv1Event::ChannelTopologyChanged(channels) => {
                ensure_lv1_snapshot(&mut inner).channels = channels.clone();
                inner.push_log(
                    LogSource::Lv1,
                    LogSeverity::Info,
                    format!("Channel topology updated: {} channels", channels.len()),
                );
            }
        }

        Some(snapshot_from_inner(&inner))
    }
}

impl ShellInner {
    fn push_log(&mut self, source: LogSource, severity: LogSeverity, message: String) {
        self.next_log_id += 1;
        let timestamp = current_timestamp();
        self.last_event_at = Some(timestamp.clone());
        self.logs.push_back(AppLogEntry {
            id: self.next_log_id,
            timestamp,
            source,
            severity,
            message,
        });
        while self.logs.len() > MAX_LOGS {
            self.logs.pop_front();
        }
    }
}

fn ensure_lv1_snapshot(inner: &mut ShellInner) -> &mut Lv1StateSnapshot {
    inner.lv1_snapshot.get_or_insert_with(|| Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: None,
        scene_list: Vec::new(),
        channels: Vec::new(),
    })
}

fn snapshot_from_inner(inner: &ShellInner) -> AppSnapshot {
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

    AppSnapshot {
        connection,
        current_scene,
        scene_count: scenes.len(),
        scenes,
        channel_count,
        fade_state: inner.fade_state.clone(),
        lockout: inner.lockout,
        logs: inner.logs.iter().cloned().collect(),
        last_event_at: inner.last_event_at.clone(),
    }
}

fn current_timestamp() -> String {
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
    use lv1_scene_fade_utility::lv1::state::{ChannelInfo, SceneListEntry, SceneState};

    fn connected_snapshot() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
        }
    }

    #[tokio::test]
    async fn default_snapshot_is_safe_and_disconnected() {
        let state = ShellState::default();
        let snapshot = state.snapshot().await;

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert_eq!(snapshot.current_scene, None);
        assert_eq!(snapshot.scene_count, 0);
        assert_eq!(snapshot.channel_count, 0);
        assert_eq!(snapshot.fade_state, AppFadeState::Idle);
        assert!(!snapshot.lockout);
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
    async fn begin_connecting_sets_connecting_snapshot_and_logs_it() {
        let state = ShellState::default();

        let (generation, snapshot) = state.begin_connecting().await;

        assert_eq!(generation, 1);
        assert_eq!(snapshot.connection, AppConnectionState::Connecting);
        assert_eq!(snapshot.logs.len(), 1);
        assert_eq!(snapshot.logs[0].message, "Connecting to LV1");
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

    #[tokio::test]
    async fn lv1_scene_event_updates_rust_owned_snapshot() {
        let state = ShellState::default();
        let (generation, _snapshot) = state.begin_connecting().await;
        let snapshot = state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::SceneChanged(SceneState {
                    index: 7,
                    name: "Chorus".to_string(),
                }),
            )
            .await;

        let snapshot = snapshot.expect("event should apply to current generation");

        assert_eq!(snapshot.connection, AppConnectionState::Connecting);
        assert_eq!(snapshot.current_scene.unwrap().name, "Chorus");
        assert_eq!(snapshot.logs.len(), 2);
    }

    #[tokio::test]
    async fn begin_connection_preserves_incoming_connection_state() {
        let state = ShellState::default();
        let (_, _connecting) = state.begin_connecting().await;

        let snapshot = state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connecting,
                scene: None,
                scene_list: Vec::new(),
                channels: Vec::new(),
            })
            .await;

        assert_eq!(snapshot.connection, AppConnectionState::Connecting);
        assert_eq!(snapshot.logs.last().unwrap().message, "Connecting to LV1");

        let snapshot = state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: Vec::new(),
                channels: Vec::new(),
            })
            .await;

        assert_eq!(snapshot.connection, AppConnectionState::Connected);
        assert_eq!(snapshot.logs.last().unwrap().message, "LV1 connected");
    }

    #[tokio::test]
    async fn stale_lv1_events_are_ignored_after_generation_change() {
        let state = ShellState::default();

        let (first_generation, first_snapshot) = state.begin_connecting().await;
        assert_eq!(first_snapshot.connection, AppConnectionState::Connecting);

        let (second_generation, second_connecting) = state.begin_connecting().await;
        assert_eq!(second_generation, first_generation + 1);
        assert_eq!(second_connecting.connection, AppConnectionState::Connecting);

        let second_snapshot = state
            .begin_connection(Lv1StateSnapshot {
                scene: None,
                scene_list: vec![],
                channels: vec![],
                connection: ConnectionStatus::Connected,
            })
            .await;
        assert_eq!(second_snapshot.connection, AppConnectionState::Connected);

        let stale = state
            .apply_lv1_event_for_generation(
                first_generation,
                &Lv1Event::SceneChanged(SceneState {
                    index: 5,
                    name: "Intro".to_string(),
                }),
            )
            .await;
        assert!(stale.is_none());

        let current = state
            .apply_lv1_event_for_generation(
                second_generation,
                &Lv1Event::SceneChanged(SceneState {
                    index: 6,
                    name: "Bridge".to_string(),
                }),
            )
            .await;
        assert!(current.is_some());

        let latest = current.expect("event should apply to current generation");
        assert_eq!(latest.current_scene.unwrap().name, "Bridge");
    }

    #[tokio::test]
    async fn disconnect_increments_generation_and_ignores_old_events() {
        let state = ShellState::default();
        let (generation, snapshot) = state.begin_connecting().await;
        assert_eq!(snapshot.connection, AppConnectionState::Connecting);

        let snapshot = state.begin_connection(connected_snapshot()).await;
        assert_eq!(snapshot.connection, AppConnectionState::Connected);

        let disconnected = state.disconnect().await;
        assert_eq!(disconnected.connection, AppConnectionState::Disconnected);

        let stale = state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::SceneChanged(SceneState {
                    index: 9,
                    name: "Outro".to_string(),
                }),
            )
            .await;
        assert!(stale.is_none());
    }
}
