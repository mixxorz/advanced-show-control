use advanced_show_control::fade::events::FadeEvent;
use advanced_show_control::lv1::events::Lv1Event;
use advanced_show_control::lv1::types::{ChannelInfo, ConnectionStatus, Lv1StateSnapshot};
use advanced_show_control::show::types::scene_id;

use super::shell::{MAX_LOGS, ShellInner, ShellState, refresh_discovered_statuses};
use super::view::{AppFadeState, AppLogEntry, AppViewState, LogSeverity};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionOutcome {
    Applied,
    Stale,
    Ignored,
}

impl ProjectionOutcome {
    pub fn was_applied(self) -> bool {
        matches!(self, Self::Applied)
    }
}

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
        tracing::info!(event = "lv1_connecting", "Connecting to LV1");
        let generation = inner.generation;
        drop(inner);
        (generation, self.snapshot().await)
    }

    pub async fn set_lockout(&self, enabled: bool) -> AppViewState {
        self.show.set_lockout(enabled).await;
        let mut inner = self.inner.lock().await;
        inner.show_file_dirty = true;
        tracing::info!(
            event = "lockout_changed",
            enabled = enabled,
            "Lockout {}",
            if enabled { "enabled" } else { "disabled" }
        );
        drop(inner);
        self.snapshot().await
    }

    pub async fn begin_connection(
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

        if !scene_list.is_empty() {
            let changed = self.show.reconcile_scene_list(scene_list.clone()).await;
            if changed {
                inner.show_file_dirty = true;
            }
            if inner.selected_scene_id.is_none() {
                inner.selected_scene_id = scene_list
                    .first()
                    .map(|scene| scene_id(scene.index, &scene.name));
            }
        }
        drop(inner);
        self.snapshot_for_generation(generation).await
    }

    pub async fn disconnect(&self) -> (u64, AppViewState) {
        let mut inner = self.inner.lock().await;
        inner.generation = inner.generation.saturating_add(1);
        inner.lv1_snapshot = None;
        inner.connected_lv1_identity = None;
        inner.pending_lv1_identity = None;
        inner.reconnect_state.active = false;
        refresh_discovered_statuses(&mut inner);
        tracing::info!(event = "lv1_disconnected", "Disconnected from LV1");
        let generation = inner.generation;
        drop(inner);
        (generation, self.snapshot().await)
    }

    pub async fn apply_lv1_event_to_projection(
        &self,
        generation: u64,
        event: &Lv1Event,
    ) -> ProjectionOutcome {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return ProjectionOutcome::Stale;
        }

        match event {
            Lv1Event::Connected => {
                ensure_lv1_snapshot(&mut inner).connection = ConnectionStatus::Connected;
                inner.reconnect_state.active = false;
                refresh_discovered_statuses(&mut inner);
                tracing::info!(event = "lv1_connected", "LV1 connected");
            }
            Lv1Event::Disconnected { reason } => {
                let had_connected_identity = inner.connected_lv1_identity.is_some();
                inner.lv1_snapshot = None;
                inner.pending_lv1_identity = None;
                inner.reconnect_state.active = had_connected_identity;
                if had_connected_identity {
                    inner.reconnect_state.attempt = inner.reconnect_state.attempt.saturating_add(1);
                }
                refresh_discovered_statuses(&mut inner);
                tracing::warn!(event = "lv1_disconnected", reason = %reason, "LV1 disconnected: {reason}");
            }
            Lv1Event::SceneChanged(scene) => {
                ensure_lv1_snapshot(&mut inner).scene = Some(scene.clone());
                tracing::info!(
                    event = "lv1_scene_changed",
                    scene_index = scene.index,
                    scene_name = %scene.name,
                    "Scene changed to {}: {}",
                    scene.index,
                    scene.name
                );
            }
            Lv1Event::SceneListChanged(scenes) => {
                let generation = inner.generation;
                drop(inner);

                let mut inner = self.inner.lock().await;
                if inner.generation != generation {
                    return ProjectionOutcome::Stale;
                }

                let changed = self.show.reconcile_scene_list(scenes.clone()).await;
                ensure_lv1_snapshot(&mut inner).scene_list = scenes.clone();
                if changed {
                    inner.show_file_dirty = true;
                }
                return ProjectionOutcome::Applied;
            }
            Lv1Event::FaderChanged {
                group,
                channel,
                gain_db,
            } => {
                update_channel(&mut inner, *group, *channel, |existing| {
                    existing.gain_db = *gain_db;
                });
            }
            Lv1Event::MuteChanged {
                group,
                channel,
                muted,
            } => {
                update_channel(&mut inner, *group, *channel, |existing| {
                    existing.muted = *muted;
                });
            }
            Lv1Event::PanChanged {
                group,
                channel,
                pan,
            } => {
                update_channel(&mut inner, *group, *channel, |existing| {
                    existing.pan = Some(*pan);
                });
            }
            Lv1Event::BalanceChanged {
                group,
                channel,
                balance,
            } => {
                update_channel(&mut inner, *group, *channel, |existing| {
                    existing.balance = Some(*balance);
                });
            }
            Lv1Event::WidthChanged {
                group,
                channel,
                width,
            } => {
                update_channel(&mut inner, *group, *channel, |existing| {
                    existing.width = Some(*width);
                });
            }
            Lv1Event::ChannelTopologyChanged(channels) => {
                ensure_lv1_snapshot(&mut inner).channels = channels.clone();
            }
        }

        ProjectionOutcome::Applied
    }

    #[cfg(test)]
    pub async fn apply_fade_event(&self, event: &FadeEvent) -> AppViewState {
        let mut inner = self.inner.lock().await;
        apply_fade_event_locked(&mut inner, event);
        drop(inner);
        self.snapshot().await
    }

    pub async fn apply_fade_event_to_projection(
        &self,
        generation: u64,
        event: &FadeEvent,
    ) -> ProjectionOutcome {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return ProjectionOutcome::Stale;
        }

        apply_fade_event_locked(&mut inner, event);
        ProjectionOutcome::Applied
    }
}

fn update_channel(
    inner: &mut ShellInner,
    group: i32,
    channel: i32,
    apply: impl FnOnce(&mut ChannelInfo),
) {
    if let Some(existing) = ensure_lv1_snapshot(inner)
        .channels
        .iter_mut()
        .find(|ch| ch.group == group && ch.channel == channel)
    {
        apply(existing);
    }
}

fn apply_fade_event_locked(inner: &mut ShellInner, event: &FadeEvent) {
    match event {
        FadeEvent::FadeStarted => {
            inner.fade_state = AppFadeState::Running;
        }
        FadeEvent::FadeCompleted => {
            inner.fade_state = AppFadeState::Idle;
        }
        FadeEvent::FadeAborted => {
            inner.fade_state = AppFadeState::Idle;
        }
        FadeEvent::ChannelCompleted { .. } => {}
        FadeEvent::ChannelOverride { group, channel, .. } => {
            inner.fade_state = AppFadeState::Blocked;
            let _ = (group, channel);
        }
        FadeEvent::ChannelCancelled { group, channel, .. } => {
            let _ = (group, channel);
        }
        FadeEvent::WriteFailed { reason } => {
            let _ = reason;
        }
    }
}

impl ShellInner {
    pub(super) fn append_log(&mut self, severity: LogSeverity, message: String) {
        self.next_log_id += 1;
        let timestamp = crate::time::current_timestamp_millis();
        self.last_event_at = Some(timestamp.clone());
        self.logs.push_back(AppLogEntry {
            id: self.next_log_id,
            timestamp,
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
    match inner
        .lv1_snapshot
        .as_ref()
        .map(|snapshot| &snapshot.connection)
    {
        Some(ConnectionStatus::Connecting) => {
            tracing::info!(event = "lv1_connecting", "Connecting to LV1");
        }
        Some(ConnectionStatus::Connected) => {
            tracing::info!(event = "lv1_connected", "LV1 connected");
        }
        Some(ConnectionStatus::Disconnected) | None => {
            tracing::info!(event = "lv1_disconnected", "LV1 disconnected");
        }
    }
}
