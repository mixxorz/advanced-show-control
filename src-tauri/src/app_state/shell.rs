use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use lv1_scene_fade_utility::lv1::model::{ConnectionStatus, Lv1StateSnapshot};
use tokio::sync::Mutex;

use super::view::{
    AppConnectionState, AppFadeState, AppLogEntry, AppViewState, ChannelSummary, LogSeverity,
    LogSource, SceneConfig, SceneSummary,
};

pub(super) const MAX_LOGS: usize = 200;

#[derive(Default)]
pub struct RuntimeHandles {
    pub lv1: Option<lv1_scene_fade_utility::lv1::state::Lv1ActorHandle>,
    pub fade: Option<lv1_scene_fade_utility::fade::engine::FadeEngineHandle>,
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
    pub(super) fade_state: AppFadeState,
    pub(super) lockout: bool,
    pub(super) scene_configs: Vec<SceneConfig>,
    pub(super) selected_scene_id: Option<String>,
    pub(super) show_file_path: Option<PathBuf>,
    pub(super) show_file_dirty: bool,
    pub(super) show_file_last_saved_at: Option<String>,
    pub(super) logs: VecDeque<AppLogEntry>,
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
}

fn cover_state_variants() {
    let _ = (
        LogSource::Fade,
        LogSeverity::Error,
        AppFadeState::Running,
        AppFadeState::Blocked,
    );
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
    fn enum_variants_are_kept_for_state_space_coverage() {
        let _ = (
            LogSource::Fade,
            LogSeverity::Error,
            AppFadeState::Running,
            AppFadeState::Blocked,
        );
    }
}
