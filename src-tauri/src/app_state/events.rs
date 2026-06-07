use std::collections::HashSet;

use lv1_scene_fade_utility::fade::types::FadeEvent;
use lv1_scene_fade_utility::lv1::messages::Lv1Event;
use lv1_scene_fade_utility::lv1::model::{ConnectionStatus, Lv1StateSnapshot, SceneListEntry};

use super::shell::{
    MAX_LOGS, ShellInner, ShellState, current_timestamp, scene_id, snapshot_from_inner,
};
use super::view::{AppFadeState, AppLogEntry, AppViewState, LogSeverity, LogSource, SceneConfig};

impl ShellState {
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

    pub async fn apply_fade_event(&self, event: &FadeEvent) -> AppViewState {
        let mut inner = self.inner.lock().await;

        match event {
            FadeEvent::FadeStarted => {
                inner.fade_state = AppFadeState::Running;
                inner.push_log(
                    LogSource::Fade,
                    LogSeverity::Info,
                    "Fade started".to_string(),
                );
            }
            FadeEvent::FadeCompleted => {
                inner.fade_state = AppFadeState::Idle;
                inner.push_log(
                    LogSource::Fade,
                    LogSeverity::Info,
                    "Fade completed".to_string(),
                );
            }
            FadeEvent::FadeAborted => {
                inner.fade_state = AppFadeState::Idle;
                inner.push_log(
                    LogSource::Fade,
                    LogSeverity::Warning,
                    "Fade aborted".to_string(),
                );
            }
            FadeEvent::ChannelOverride { group, channel } => {
                inner.fade_state = AppFadeState::Blocked;
                inner.push_log(
                    LogSource::Fade,
                    LogSeverity::Warning,
                    format!("Fade blocked by channel override: group {group}, channel {channel}"),
                );
            }
            FadeEvent::ChannelCancelled { group, channel } => {
                inner.push_log(
                    LogSource::Fade,
                    LogSeverity::Warning,
                    format!("Fade channel cancelled: group {group}, channel {channel}"),
                );
            }
        }

        snapshot_from_inner(&inner)
    }
}

impl ShellInner {
    pub(super) fn reconcile_scene_fade_configs(&mut self, scenes: &[SceneListEntry]) {
        let previous_scene_ids: HashSet<_> = self
            .scene_configs
            .iter()
            .map(|config| config.scene_id.clone())
            .collect();
        let mut next = Vec::with_capacity(scenes.len());

        for scene in scenes {
            let id = scene_id(scene.index, &scene.name);
            if let Some(mut existing) = self
                .scene_configs
                .iter()
                .find(|config| config.scene_id == id)
                .cloned()
            {
                existing.scene_index = scene.index;
                existing.scene_name = scene.name.clone();
                next.push(existing);
            } else {
                next.push(SceneConfig {
                    scene_id: id,
                    scene_index: scene.index,
                    scene_name: scene.name.clone(),
                    duration_ms: 0,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                });
            }
        }

        let selected_still_exists = self
            .selected_scene_id
            .as_ref()
            .is_some_and(|selected| next.iter().any(|config| &config.scene_id == selected));

        if !selected_still_exists {
            self.selected_scene_id = next.first().map(|config| config.scene_id.clone());
        }

        let next_scene_ids: HashSet<_> =
            next.iter().map(|config| config.scene_id.clone()).collect();
        let scene_set_changed = previous_scene_ids != next_scene_ids;

        self.scene_configs = next;

        if scene_set_changed && (self.show_file_path.is_some() || self.show_file_dirty) {
            self.show_file_dirty = true;
        }
    }

    pub(super) fn push_log(&mut self, source: LogSource, severity: LogSeverity, message: String) {
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
