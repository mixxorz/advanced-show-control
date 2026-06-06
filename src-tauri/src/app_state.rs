use std::collections::VecDeque;
use std::sync::Arc;

use lv1_scene_fade_utility::lv1::state::{ConnectionStatus, Lv1StateSnapshot};
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

impl Default for ShellState {
    fn default() -> Self {
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

    pub async fn clear_lv1_snapshot(&self) -> AppSnapshot {
        let mut inner = self.inner.lock().await;
        inner.lv1_snapshot = None;
        inner.push_log(LogSource::App, LogSeverity::Info, "Disconnected from LV1".to_string());
        snapshot_from_inner(&inner)
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

    let channel_count = inner.lv1_snapshot.as_ref().map(|snapshot| snapshot.channels.len()).unwrap_or(0);

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
}
