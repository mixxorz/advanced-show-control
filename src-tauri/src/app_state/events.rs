use advanced_show_control::fade::events::FadeEvent;
use advanced_show_control::lv1::events::Lv1Event;
use advanced_show_control::lv1::types::{ConnectionStatus, Lv1StateSnapshot};

use super::shell::{
    MAX_LOGS, ShellInner, ShellState, current_timestamp, refresh_discovered_statuses,
};
use super::view::{AppFadeState, AppLogEntry, AppViewState, LogSeverity, LogSource};

impl ShellState {
    #[cfg(test)]
    pub async fn begin_connecting(&self) -> (u64, AppViewState) {
        self.begin_connecting_unchecked().await
    }

    pub async fn try_begin_connecting(&self) -> Option<(u64, AppViewState)> {
        let inner = self.inner.lock().await;
        if matches!(
            inner
                .lv1_snapshot
                .as_ref()
                .map(|snapshot| &snapshot.connection),
            Some(ConnectionStatus::Connecting)
        ) {
            return None;
        }
        drop(inner);

        Some(self.begin_connecting_unchecked().await)
    }

    async fn begin_connecting_unchecked(&self) -> (u64, AppViewState) {
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
        drop(inner);
        (generation, self.snapshot().await)
    }

    pub async fn set_lockout(&self, enabled: bool) -> AppViewState {
        self.show.set_lockout(enabled).await;
        let mut inner = self.inner.lock().await;
        inner.show_file_dirty = true;
        inner.push_log(
            LogSource::App,
            LogSeverity::Info,
            format!("Lockout {}", if enabled { "enabled" } else { "disabled" }),
        );
        drop(inner);
        self.snapshot().await
    }

    #[cfg(test)]
    pub async fn begin_connection(&self, snapshot: Lv1StateSnapshot) -> AppViewState {
        let mut inner = self.inner.lock().await;
        apply_begin_connection(&mut inner, snapshot);
        let scene_list = inner
            .lv1_snapshot
            .as_ref()
            .map(|snapshot| snapshot.scene_list.clone())
            .unwrap_or_default();
        let generation = inner.generation;
        drop(inner);

        if !scene_list.is_empty() {
            let changed = self.show.reconcile_scene_list(scene_list.clone()).await;
            let mut inner = self.inner.lock().await;
            if inner.generation == generation
                && inner
                    .lv1_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.scene_list.clone())
                    == Some(scene_list.clone())
            {
                if changed {
                    inner.show_file_dirty = true;
                }
                if inner.selected_scene_id.is_none() {
                    inner.selected_scene_id = scene_list
                        .first()
                        .map(|scene| format!("{}::{}", scene.index, scene.name));
                }
            }
            drop(inner);
        }
        self.snapshot().await
    }

    pub async fn begin_connection_for_generation(
        &self,
        generation: u64,
        snapshot: Lv1StateSnapshot,
    ) -> Option<AppViewState> {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return None;
        }

        apply_begin_connection(&mut inner, snapshot);
        let scene_list = inner
            .lv1_snapshot
            .as_ref()
            .map(|snapshot| snapshot.scene_list.clone())
            .unwrap_or_default();
        let generation = inner.generation;
        drop(inner);

        if !scene_list.is_empty() {
            if !self.is_generation_current(generation).await {
                return None;
            }
            let changed = self.show.reconcile_scene_list(scene_list.clone()).await;
            let mut inner = self.inner.lock().await;
            if inner.generation != generation {
                return None;
            }
            if inner
                .lv1_snapshot
                .as_ref()
                .map(|snapshot| snapshot.scene_list.clone())
                == Some(scene_list.clone())
            {
                if changed {
                    inner.show_file_dirty = true;
                }
                if inner.selected_scene_id.is_none() {
                    inner.selected_scene_id = scene_list
                        .first()
                        .map(|scene| format!("{}::{}", scene.index, scene.name));
                }
            }
            drop(inner);
        }
        Some(self.snapshot().await)
    }

    pub async fn disconnect(&self) -> (u64, AppViewState) {
        let mut inner = self.inner.lock().await;
        inner.generation = inner.generation.saturating_add(1);
        inner.lv1_snapshot = None;
        inner.connected_lv1_identity = None;
        inner.pending_lv1_identity = None;
        inner.reconnect_state.active = false;
        refresh_discovered_statuses(&mut inner);
        inner.push_log(
            LogSource::App,
            LogSeverity::Info,
            "Disconnected from LV1".to_string(),
        );
        let generation = inner.generation;
        drop(inner);
        (generation, self.snapshot().await)
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
                inner.reconnect_state.active = false;
                refresh_discovered_statuses(&mut inner);
                inner.push_log(
                    LogSource::Lv1,
                    LogSeverity::Info,
                    "LV1 connected".to_string(),
                );
            }
            Lv1Event::Disconnected => {
                let had_connected_identity = inner.connected_lv1_identity.is_some();
                inner.lv1_snapshot = None;
                inner.pending_lv1_identity = None;
                inner.reconnect_state.active = had_connected_identity;
                if had_connected_identity {
                    inner.reconnect_state.attempt = inner.reconnect_state.attempt.saturating_add(1);
                }
                refresh_discovered_statuses(&mut inner);
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
                let generation = inner.generation;
                if inner.generation != generation {
                    return None;
                }

                // Gather diagnostics before holding the lock to avoid lock contention
                drop(inner);
                let before_count = self.show.get_snapshot().await.scene_configs.len();
                let reconciliation_diagnostic = self
                    .show
                    .scene_reconciliation_diagnostic(scenes.clone())
                    .await;

                // Re-acquire inner lock and hold it across reconciliation to prevent stale mutations.
                // Lock ordering: inner then show (consistent with snapshot()).
                let mut inner = self.inner.lock().await;
                if inner.generation != generation {
                    return None;
                }

                // Call reconcile_scene_list while holding inner lock to ensure generation
                // doesn't change between the check and the mutation
                let changed = self.show.reconcile_scene_list(scenes.clone()).await;
                let after_count = self.show.get_snapshot().await.scene_configs.len();

                ensure_lv1_snapshot(&mut inner).scene_list = scenes.clone();
                if changed {
                    inner.show_file_dirty = true;
                }
                inner.push_log(
                    LogSource::Lv1,
                    LogSeverity::Info,
                    format!("Scene list updated: {} scenes", scenes.len()),
                );
                inner.push_log(LogSource::App, LogSeverity::Info, reconciliation_diagnostic);
                inner.push_log(
                    LogSource::App,
                    LogSeverity::Info,
                    format!(
                        "Diagnostic: scene list projection scenes={} changed={} configs_before={} configs_after={}",
                        scenes.len(),
                        changed,
                        before_count,
                        after_count
                    ),
                );
                drop(inner);
                return Some(self.snapshot().await);
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
            Lv1Event::PanChanged {
                group,
                channel,
                pan,
            } => {
                if let Some(existing) = ensure_lv1_snapshot(&mut inner)
                    .channels
                    .iter_mut()
                    .find(|ch| ch.group == *group && ch.channel == *channel)
                {
                    existing.pan = Some(*pan);
                }
            }
            Lv1Event::BalanceChanged {
                group,
                channel,
                balance,
            } => {
                if let Some(existing) = ensure_lv1_snapshot(&mut inner)
                    .channels
                    .iter_mut()
                    .find(|ch| ch.group == *group && ch.channel == *channel)
                {
                    existing.balance = Some(*balance);
                }
            }
            Lv1Event::WidthChanged {
                group,
                channel,
                width,
            } => {
                if let Some(existing) = ensure_lv1_snapshot(&mut inner)
                    .channels
                    .iter_mut()
                    .find(|ch| ch.group == *group && ch.channel == *channel)
                {
                    existing.width = Some(*width);
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

        drop(inner);
        Some(self.snapshot().await)
    }

    #[cfg(test)]
    pub async fn apply_fade_event(&self, event: &FadeEvent) -> AppViewState {
        let mut inner = self.inner.lock().await;
        apply_fade_event_locked(&mut inner, event);
        drop(inner);
        self.snapshot().await
    }

    pub async fn apply_fade_event_for_generation(
        &self,
        generation: u64,
        event: &FadeEvent,
    ) -> Option<AppViewState> {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return None;
        }

        apply_fade_event_locked(&mut inner, event);
        drop(inner);
        self.snapshot_for_generation(generation).await
    }
}

impl ShellState {
    async fn is_generation_current(&self, generation: u64) -> bool {
        self.inner.lock().await.generation == generation
    }
}

fn apply_fade_event_locked(inner: &mut ShellInner, event: &FadeEvent) {
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
        FadeEvent::ChannelCompleted { group, channel } => {
            inner.push_log(
                LogSource::Fade,
                LogSeverity::Info,
                format!("Fade channel completed: group {group}, channel {channel}"),
            );
        }
        FadeEvent::ChannelOverride { group, channel } => {
            inner.fade_state = AppFadeState::Blocked;
            inner.push_log(
                LogSource::Fade,
                LogSeverity::Warning,
                format!("Fade channel override detected: group={group} channel={channel}"),
            );
        }
        FadeEvent::ChannelCancelled { group, channel } => {
            inner.push_log(
                LogSource::Fade,
                LogSeverity::Warning,
                format!("Fade channel cancelled: group {group}, channel {channel}"),
            );
        }
        FadeEvent::WriteFailed { reason } => {
            inner.push_log(
                LogSource::Fade,
                LogSeverity::Error,
                format!("Fade write failed: {reason}"),
            );
        }
    }
}

impl ShellInner {
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

fn apply_begin_connection(inner: &mut ShellInner, snapshot: Lv1StateSnapshot) {
    let connected = matches!(snapshot.connection, ConnectionStatus::Connected);
    inner.lv1_snapshot = Some(snapshot);
    if connected {
        inner.reconnect_state.active = false;
    }
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
}
