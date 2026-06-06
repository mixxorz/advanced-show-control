use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

use lv1_scene_fade_utility::lv1::state::{
    ConnectionStatus, Lv1Event, Lv1StateSnapshot, SceneListEntry,
};
use serde::Serialize;
use tokio::sync::Mutex;

use crate::show_file::DEFAULT_DURATION_MS;

const MAX_LOGS: usize = 200;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SceneSummary {
    pub index: i32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSummary {
    pub group: i32,
    pub channel: i32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FadeTarget {
    pub group: i32,
    pub channel: i32,
    pub channel_name: String,
    pub target_db: f64,
    pub enabled: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneFadeConfig {
    pub scene_id: String,
    pub scene_index: i32,
    pub scene_name: String,
    pub fade_enabled: bool,
    pub duration_ms: u64,
    pub fade_targets: Vec<FadeTarget>,
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

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppViewState {
    pub connection: AppConnectionState,
    pub current_scene: Option<SceneSummary>,
    pub scenes: Vec<SceneSummary>,
    pub scene_count: usize,
    pub channel_count: usize,
    pub channels: Vec<ChannelSummary>,
    pub fade_state: AppFadeState,
    pub lockout: bool,
    pub scene_fade_configs: Vec<SceneFadeConfig>,
    pub selected_scene_id: Option<String>,
    pub listen_mode_active: bool,
    pub show_file_name: String,
    pub show_file_path: Option<String>,
    pub show_file_dirty: bool,
    pub show_file_last_saved_at: Option<String>,
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
    scene_fade_configs: Vec<SceneFadeConfig>,
    selected_scene_id: Option<String>,
    listen_mode_active: bool,
    show_file_path: Option<String>,
    show_file_dirty: bool,
    show_file_last_saved_at: Option<String>,
    unknown_fader_warnings: HashSet<(i32, i32)>,
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
    pub async fn snapshot(&self) -> AppViewState {
        let inner = self.inner.lock().await;
        snapshot_from_inner(&inner)
    }

    #[allow(dead_code)]
    pub async fn select_scene_config(&self, scene_id: String) -> Result<AppViewState, String> {
        let mut inner = self.inner.lock().await;

        if inner.listen_mode_active {
            return Err("Stop Listen Mode before selecting another scene".to_string());
        }

        if !inner
            .scene_fade_configs
            .iter()
            .any(|config| config.scene_id == scene_id)
        {
            return Err("Scene config not found".to_string());
        }

        inner.selected_scene_id = Some(scene_id);
        Ok(snapshot_from_inner(&inner))
    }

    #[allow(dead_code)]
    pub async fn set_scene_fade_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<AppViewState, String> {
        let mut inner = self.inner.lock().await;
        let config = inner
            .scene_fade_configs
            .iter_mut()
            .find(|config| config.scene_id == scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;

        config.fade_enabled = enabled;
        inner.show_file_dirty = true;
        Ok(snapshot_from_inner(&inner))
    }

    pub async fn set_listen_mode(&self, active: bool) -> Result<AppViewState, String> {
        let mut inner = self.inner.lock().await;

        if active {
            if inner.selected_scene_id.is_none() {
                return Err("Select a scene before starting Listen Mode".to_string());
            }

            if inner
                .lv1_snapshot
                .as_ref()
                .map(|snapshot| snapshot.channels.is_empty())
                .unwrap_or(true)
            {
                return Err("LV1 channel list is empty".to_string());
            }
        }

        inner.listen_mode_active = active;
        Ok(snapshot_from_inner(&inner))
    }

    pub async fn set_scene_duration_ms(
        &self,
        scene_id: String,
        duration_ms: u64,
    ) -> Result<AppViewState, String> {
        if !(100..=120_000).contains(&duration_ms) {
            return Err("Fade duration must be between 100 ms and 120000 ms".to_string());
        }

        let mut inner = self.inner.lock().await;
        let config = inner
            .scene_fade_configs
            .iter_mut()
            .find(|config| config.scene_id == scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;

        config.duration_ms = duration_ms;
        inner.show_file_dirty = true;
        Ok(snapshot_from_inner(&inner))
    }

    #[allow(dead_code)]
    pub async fn set_fade_target_enabled(
        &self,
        scene_id: String,
        group: i32,
        channel: i32,
        enabled: bool,
    ) -> Result<AppViewState, String> {
        let mut inner = self.inner.lock().await;
        let target = find_target_mut(&mut inner, &scene_id, group, channel)?;

        target.enabled = enabled;
        inner.show_file_dirty = true;
        Ok(snapshot_from_inner(&inner))
    }

    pub async fn remove_fade_target(
        &self,
        scene_id: &str,
        group: i32,
        channel: i32,
    ) -> Result<AppViewState, String> {
        let mut inner = self.inner.lock().await;
        let config = inner
            .scene_fade_configs
            .iter_mut()
            .find(|config| config.scene_id == scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        let before = config.fade_targets.len();
        config
            .fade_targets
            .retain(|target| !(target.group == group && target.channel == channel));

        if config.fade_targets.len() == before {
            return Err("Fade target not found".to_string());
        }

        inner.show_file_dirty = true;
        Ok(snapshot_from_inner(&inner))
    }

    pub async fn begin_connecting(&self) -> (u64, AppViewState) {
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

    pub async fn set_lockout(&self, enabled: bool) -> AppViewState {
        let mut inner = self.inner.lock().await;
        inner.lockout = enabled;
        inner.show_file_dirty = true;
        inner.push_log(
            LogSource::App,
            LogSeverity::Info,
            format!("Lockout {}", if enabled { "enabled" } else { "disabled" }),
        );
        snapshot_from_inner(&inner)
    }

    pub async fn begin_connection(&self, snapshot: Lv1StateSnapshot) -> AppViewState {
        let mut inner = self.inner.lock().await;
        inner.lv1_snapshot = Some(snapshot);
        let scenes = inner
            .lv1_snapshot
            .as_ref()
            .map(|snapshot| snapshot.scene_list.clone())
            .unwrap_or_default();
        inner.reconcile_scene_fade_configs(&scenes);
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

    pub async fn disconnect(&self) -> AppViewState {
        let mut inner = self.inner.lock().await;
        inner.generation = inner.generation.saturating_add(1);
        inner.lv1_snapshot = None;
        inner.listen_mode_active = false;
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
    ) -> Option<AppViewState> {
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
                inner.listen_mode_active = false;
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
                inner.reconcile_scene_fade_configs(scenes);
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

                inner.record_fader_target(*group, *channel, *gain_db);
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
    fn reconcile_scene_fade_configs(&mut self, scenes: &[SceneListEntry]) {
        let previous_scene_ids: HashSet<_> = self
            .scene_fade_configs
            .iter()
            .map(|config| config.scene_id.clone())
            .collect();
        let mut next = Vec::with_capacity(scenes.len());

        for scene in scenes {
            let id = scene_id(scene.index, &scene.name);
            if let Some(mut existing) = self
                .scene_fade_configs
                .iter()
                .find(|config| config.scene_id == id)
                .cloned()
            {
                existing.scene_index = scene.index;
                existing.scene_name = scene.name.clone();
                next.push(existing);
            } else {
                next.push(SceneFadeConfig {
                    scene_id: id,
                    scene_index: scene.index,
                    scene_name: scene.name.clone(),
                    fade_enabled: false,
                    duration_ms: DEFAULT_DURATION_MS,
                    fade_targets: Vec::new(),
                });
            }
        }

        let had_selected_scene = self.selected_scene_id.is_some();
        let selected_still_exists = self
            .selected_scene_id
            .as_ref()
            .is_some_and(|selected| next.iter().any(|config| &config.scene_id == selected));

        if !selected_still_exists {
            if had_selected_scene && self.listen_mode_active {
                self.listen_mode_active = false;
                self.push_log(
                    LogSource::App,
                    LogSeverity::Warning,
                    "Listen Mode stopped because selected scene is no longer available".to_string(),
                );
            }
            self.selected_scene_id = next.first().map(|config| config.scene_id.clone());
        }

        let next_scene_ids: HashSet<_> =
            next.iter().map(|config| config.scene_id.clone()).collect();
        let scene_set_changed = previous_scene_ids != next_scene_ids;

        self.scene_fade_configs = next;

        if scene_set_changed && (self.show_file_path.is_some() || self.show_file_dirty) {
            self.show_file_dirty = true;
        }
    }

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

    fn record_fader_target(&mut self, group: i32, channel: i32, gain_db: f64) {
        if !self.listen_mode_active {
            return;
        }

        let Some(selected_scene_id) = self.selected_scene_id.clone() else {
            return;
        };

        let channel_known = self.lv1_snapshot.as_ref().is_some_and(|snapshot| {
            snapshot
                .channels
                .iter()
                .any(|ch| ch.group == group && ch.channel == channel)
        });

        if !channel_known {
            if self.unknown_fader_warnings.insert((group, channel)) {
                self.push_log(
                    LogSource::Lv1,
                    LogSeverity::Warning,
                    format!("Ignored fader target for unknown channel {group}/{channel}"),
                );
            }
            return;
        }

        let timestamp = current_timestamp();
        let channel_name = self
            .lv1_snapshot
            .as_ref()
            .and_then(|snapshot| {
                snapshot
                    .channels
                    .iter()
                    .find(|ch| ch.group == group && ch.channel == channel)
            })
            .map(|channel| channel.name.clone())
            .unwrap_or_default();

        if let Some(config) = self
            .scene_fade_configs
            .iter_mut()
            .find(|config| config.scene_id == selected_scene_id)
        {
            if let Some(target) = config
                .fade_targets
                .iter_mut()
                .find(|target| target.group == group && target.channel == channel)
            {
                target.target_db = gain_db;
                target.updated_at = timestamp;
                target.channel_name = channel_name;
            } else {
                config.fade_targets.push(FadeTarget {
                    group,
                    channel,
                    channel_name,
                    target_db: gain_db,
                    enabled: true,
                    updated_at: timestamp,
                });
            }
            self.show_file_dirty = true;
        }
    }
}

fn scene_id(index: i32, name: &str) -> String {
    format!("{index}::{name}")
}

#[allow(dead_code)]
fn find_target_mut<'a>(
    inner: &'a mut ShellInner,
    scene_id: &str,
    group: i32,
    channel: i32,
) -> Result<&'a mut FadeTarget, String> {
    let config = inner
        .scene_fade_configs
        .iter_mut()
        .find(|config| config.scene_id == scene_id)
        .ok_or_else(|| "Scene config not found".to_string())?;

    config
        .fade_targets
        .iter_mut()
        .find(|target| target.group == group && target.channel == channel)
        .ok_or_else(|| "Fade target not found".to_string())
}

fn ensure_lv1_snapshot(inner: &mut ShellInner) -> &mut Lv1StateSnapshot {
    inner.lv1_snapshot.get_or_insert_with(|| Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: None,
        scene_list: Vec::new(),
        channels: Vec::new(),
    })
}

fn snapshot_from_inner(inner: &ShellInner) -> AppViewState {
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
        scene_fade_configs: inner.scene_fade_configs.clone(),
        selected_scene_id: inner.selected_scene_id.clone(),
        listen_mode_active: inner.listen_mode_active,
        show_file_name: inner
            .show_file_path
            .as_ref()
            .and_then(|path| Path::new(path).file_name())
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .unwrap_or_else(|| "Untitled Show".to_string()),
        show_file_path: inner.show_file_path.clone(),
        show_file_dirty: inner.show_file_dirty,
        show_file_last_saved_at: inner.show_file_last_saved_at.clone(),
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
    use crate::show_file::DEFAULT_DURATION_MS;
    use lv1_scene_fade_utility::lv1::state::{ChannelInfo, SceneListEntry, SceneState};

    fn connected_snapshot() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
        }
    }

    fn connected_state_with_scene_and_channel() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }],
            channels: vec![ChannelInfo {
                group: 0,
                channel: 2,
                name: "Lead".to_string(),
                gain_db: -8.0,
                muted: false,
            }],
        }
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
    fn scene_list_reconciliation_creates_default_configs() {
        let mut inner = ShellInner::default();
        inner.reconcile_scene_fade_configs(&[
            SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            },
            SceneListEntry {
                index: 2,
                name: "Verse".to_string(),
            },
        ]);

        assert_eq!(inner.scene_fade_configs.len(), 2);
        assert_eq!(inner.scene_fade_configs[0].scene_id, "1::Intro");
        assert!(!inner.scene_fade_configs[0].fade_enabled);
        assert_eq!(inner.scene_fade_configs[0].duration_ms, DEFAULT_DURATION_MS);
        assert!(inner.scene_fade_configs[0].fade_targets.is_empty());
        assert_eq!(inner.selected_scene_id.as_deref(), Some("1::Intro"));
    }

    #[test]
    fn scene_list_reconciliation_preserves_matching_config_data() {
        let mut inner = ShellInner::default();
        inner.scene_fade_configs = vec![SceneFadeConfig {
            scene_id: "2::Verse".to_string(),
            scene_index: 2,
            scene_name: "Verse".to_string(),
            fade_enabled: true,
            duration_ms: DEFAULT_DURATION_MS,
            fade_targets: vec![FadeTarget {
                group: 0,
                channel: 4,
                channel_name: "Lead".to_string(),
                target_db: -5.5,
                enabled: true,
                updated_at: "123".to_string(),
            }],
        }];
        inner.selected_scene_id = Some("2::Verse".to_string());

        inner.reconcile_scene_fade_configs(&[
            SceneListEntry {
                index: 2,
                name: "Verse".to_string(),
            },
            SceneListEntry {
                index: 3,
                name: "Chorus".to_string(),
            },
        ]);

        let verse = inner
            .scene_fade_configs
            .iter()
            .find(|scene| scene.scene_id == "2::Verse")
            .unwrap();
        assert!(verse.fade_enabled);
        assert_eq!(verse.fade_targets.len(), 1);
        assert_eq!(inner.scene_fade_configs.len(), 2);
        assert_eq!(inner.selected_scene_id.as_deref(), Some("2::Verse"));
    }

    #[test]
    fn scene_list_reconciliation_turns_off_listen_mode_when_selected_scene_disappears() {
        let mut inner = ShellInner::default();
        inner.scene_fade_configs = vec![SceneFadeConfig {
            scene_id: "1::Intro".to_string(),
            scene_index: 1,
            scene_name: "Intro".to_string(),
            fade_enabled: false,
            duration_ms: DEFAULT_DURATION_MS,
            fade_targets: Vec::new(),
        }];
        inner.selected_scene_id = Some("1::Intro".to_string());
        inner.listen_mode_active = true;

        inner.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 2,
            name: "Verse".to_string(),
        }]);

        assert!(!inner.listen_mode_active);
        assert_eq!(inner.selected_scene_id.as_deref(), Some("2::Verse"));
        assert!(inner.logs.iter().any(|entry| entry.message
            == "Listen Mode stopped because selected scene is no longer available"));
    }

    #[test]
    fn scene_reconciliation_marks_loaded_show_dirty_when_scene_removed() {
        let mut inner = ShellInner::default();
        inner.show_file_path = Some("/tmp/test.lv1show".to_string());
        inner.scene_fade_configs = vec![SceneFadeConfig {
            scene_id: "1::Intro".to_string(),
            scene_index: 1,
            scene_name: "Intro".to_string(),
            fade_enabled: true,
            duration_ms: DEFAULT_DURATION_MS,
            fade_targets: vec![FadeTarget {
                group: 0,
                channel: 2,
                channel_name: "Lead".to_string(),
                target_db: -5.0,
                enabled: true,
                updated_at: "123".to_string(),
            }],
        }];

        inner.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 2,
            name: "Verse".to_string(),
        }]);

        assert!(inner.show_file_dirty);
        assert_eq!(inner.scene_fade_configs.len(), 1);
        assert_eq!(inner.scene_fade_configs[0].scene_id, "2::Verse");
    }

    #[tokio::test]
    async fn set_scene_fade_enabled_marks_show_file_dirty() {
        let state = ShellState::default();
        state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: vec![SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                }],
                channels: vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Lead".to_string(),
                    gain_db: -8.0,
                    muted: false,
                }],
            })
            .await;

        let snapshot = state
            .set_scene_fade_enabled("1::Intro".to_string(), true)
            .await
            .unwrap();

        assert!(snapshot.scene_fade_configs[0].fade_enabled);
        assert!(snapshot.show_file_dirty);
    }

    #[tokio::test]
    async fn set_scene_duration_ms_updates_duration_and_marks_dirty() {
        let state = ShellState::default();
        state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: vec![SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                }],
                channels: vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Lead".to_string(),
                    gain_db: -8.0,
                    muted: false,
                }],
            })
            .await;

        let snapshot = state
            .set_scene_duration_ms("1::Intro".to_string(), 8_000)
            .await
            .unwrap();

        assert_eq!(snapshot.scene_fade_configs[0].duration_ms, 8_000);
        assert!(snapshot.show_file_dirty);
    }

    #[test]
    fn scene_list_reconciliation_keeps_listen_mode_when_no_scene_was_selected() {
        let mut inner = ShellInner::default();
        inner.listen_mode_active = true;

        inner.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 2,
            name: "Verse".to_string(),
        }]);

        assert!(inner.listen_mode_active);
        assert_eq!(inner.selected_scene_id.as_deref(), Some("2::Verse"));
        assert!(!inner.logs.iter().any(|entry| entry.message
            == "Listen Mode stopped because selected scene is no longer available"));
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
        assert_eq!(snapshot.channels.len(), 1);
        assert_eq!(snapshot.channels[0].group, 0);
        assert_eq!(snapshot.channels[0].channel, 0);
        assert_eq!(snapshot.channels[0].name, "Lead");
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

    #[tokio::test]
    async fn disconnect_turns_off_listen_mode() {
        let state = ShellState::default();
        let _ = state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: vec![SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                }],
                channels: Vec::new(),
            })
            .await;

        {
            let mut inner = state.inner.lock().await;
            inner.listen_mode_active = true;
        }

        let snapshot = state.disconnect().await;

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert!(!snapshot.listen_mode_active);
    }

    #[tokio::test]
    async fn listen_mode_requires_selected_scene_and_known_channels() {
        let state = ShellState::default();

        let err = state.set_listen_mode(true).await.unwrap_err();
        assert_eq!(err, "Select a scene before starting Listen Mode");

        let snapshot = state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: vec![SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                }],
                channels: Vec::new(),
            })
            .await;
        assert_eq!(snapshot.selected_scene_id.as_deref(), Some("1::Intro"));

        let err = state.set_listen_mode(true).await.unwrap_err();
        assert_eq!(err, "LV1 channel list is empty");
    }

    #[tokio::test]
    async fn fader_events_write_targets_only_while_listen_mode_is_active() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;

        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -4.5,
                },
            )
            .await;
        assert!(
            state.snapshot().await.scene_fade_configs[0]
                .fade_targets
                .is_empty()
        );

        state.set_listen_mode(true).await.unwrap();
        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -4.5,
                },
            )
            .await;

        let view = state.snapshot().await;
        let targets = &view.scene_fade_configs[0].fade_targets;
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].group, 0);
        assert_eq!(targets[0].channel, 2);
        assert_eq!(targets[0].channel_name, "Lead");
        assert_eq!(targets[0].target_db, -4.5);
        assert!(targets[0].enabled);
        assert!(view.show_file_dirty);
    }

    #[tokio::test]
    async fn repeated_fader_event_updates_existing_target() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;
        state.set_listen_mode(true).await.unwrap();

        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -4.5,
                },
            )
            .await;
        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -3.0,
                },
            )
            .await;

        let targets = &state.snapshot().await.scene_fade_configs[0].fade_targets;
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target_db, -3.0);
        assert_eq!(targets[0].channel_name, "Lead");
    }

    #[tokio::test]
    async fn repeated_fader_event_preserves_disabled_target_state() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;
        state.set_listen_mode(true).await.unwrap();

        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -4.5,
                },
            )
            .await;
        state
            .set_fade_target_enabled("1::Intro".to_string(), 0, 2, false)
            .await
            .unwrap();

        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -3.25,
                },
            )
            .await;

        let targets = &state.snapshot().await.scene_fade_configs[0].fade_targets;
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target_db, -3.25);
        assert!(!targets[0].enabled);
        assert_eq!(targets[0].channel_name, "Lead");
    }

    #[tokio::test]
    async fn removed_target_can_be_recaptured_while_listen_mode_is_active() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;
        state.set_listen_mode(true).await.unwrap();

        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -4.5,
                },
            )
            .await;
        state.remove_fade_target("1::Intro", 0, 2).await.unwrap();
        let snapshot = state.snapshot().await;
        assert!(snapshot.scene_fade_configs[0].fade_targets.is_empty());
        assert!(snapshot.show_file_dirty);

        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -2.0,
                },
            )
            .await;
        let targets = &state.snapshot().await.scene_fade_configs[0].fade_targets;
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target_db, -2.0);
    }

    #[tokio::test]
    async fn unknown_channel_fader_warning_logs_only_once_per_channel() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;
        state.set_listen_mode(true).await.unwrap();

        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 99,
                    gain_db: -1.0,
                },
            )
            .await;
        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 99,
                    gain_db: -2.0,
                },
            )
            .await;

        let warnings: Vec<_> = state
            .snapshot()
            .await
            .logs
            .into_iter()
            .filter(|entry| entry.message == "Ignored fader target for unknown channel 0/99")
            .collect();
        assert_eq!(warnings.len(), 1);
    }

    #[tokio::test]
    async fn disconnect_turns_off_listen_mode_and_preserves_configs() {
        let state = ShellState::default();
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;
        state.set_listen_mode(true).await.unwrap();

        let view = state.disconnect().await;

        assert!(!view.listen_mode_active);
        assert_eq!(view.scene_fade_configs.len(), 1);
    }

    #[tokio::test]
    async fn lv1_disconnected_event_turns_off_listen_mode() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        let _ = state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: vec![SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                }],
                channels: Vec::new(),
            })
            .await;

        {
            let mut inner = state.inner.lock().await;
            inner.listen_mode_active = true;
        }

        let snapshot = state
            .apply_lv1_event_for_generation(generation, &Lv1Event::Disconnected)
            .await
            .expect("event should apply to current generation");

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert!(!snapshot.listen_mode_active);
    }
}
