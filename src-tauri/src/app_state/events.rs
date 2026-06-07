use std::collections::HashSet;

use lv1_scene_fade_utility::lv1::messages::Lv1Event;
use lv1_scene_fade_utility::lv1::model::{ConnectionStatus, Lv1StateSnapshot, SceneListEntry};

use super::shell::{
    MAX_LOGS, ShellInner, ShellState, current_timestamp, scene_id, snapshot_from_inner,
};
use super::view::{AppLogEntry, AppViewState, FadeTarget, LogSeverity, LogSource, SceneFadeConfig};
use crate::show_file::DEFAULT_DURATION_MS;

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
    pub(super) fn reconcile_scene_fade_configs(&mut self, scenes: &[SceneListEntry]) {
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

    pub(super) fn record_fader_target(&mut self, group: i32, channel: i32, gain_db: f64) {
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

fn ensure_lv1_snapshot(inner: &mut ShellInner) -> &mut Lv1StateSnapshot {
    inner.lv1_snapshot.get_or_insert_with(|| Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: None,
        scene_list: Vec::new(),
        channels: Vec::new(),
    })
}
