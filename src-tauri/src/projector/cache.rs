use std::collections::VecDeque;
use std::path::PathBuf;

use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};
use crate::fade::FadeEvent;
use crate::logging::UiLogEvent;
use crate::lv1::{
    ChannelInfo, ConnectionStatus, Lv1Event, Lv1StateSnapshot, SceneListEntry, SceneState,
};
use crate::projector::{
    AppConnectionState, AppFadeState, AppLogEntry, AppViewState, ChannelSummary, SceneSummary,
};
use crate::scenes::ScenesProjectionState;
use crate::settings::AppSettings;
use crate::show::ShowProjectionState;

pub const MAX_PROJECTOR_LOGS: usize = 200;

#[derive(Debug)]
pub struct ProjectionCache {
    active_generation: u64,
    state_version: u64,
    lv1_snapshot: Option<Lv1StateSnapshot>,
    discovered_lv1_systems: Vec<DiscoveredLv1System>,
    connected_lv1_identity: Option<Lv1SystemIdentity>,
    pending_lv1_identity: Option<Lv1SystemIdentity>,
    reconnect_state: ReconnectState,
    fade_state: AppFadeState,
    selected_scene_internal_id: Option<String>,
    lockout: bool,
    scene_configs: Vec<crate::scenes::SceneConfig>,
    cued_scene_internal_id: Option<String>,
    show_file_path: Option<PathBuf>,
    show_file_dirty: bool,
    show_file_last_saved_at: Option<String>,
    settings: AppSettings,
    logs: VecDeque<AppLogEntry>,
    next_log_id: u64,
    last_event_at: Option<String>,
}

impl Default for ProjectionCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectionCache {
    pub fn new() -> Self {
        Self {
            active_generation: 0,
            state_version: 0,
            lv1_snapshot: None,
            discovered_lv1_systems: Vec::new(),
            connected_lv1_identity: None,
            pending_lv1_identity: None,
            reconnect_state: ReconnectState::default(),
            fade_state: AppFadeState::Idle,
            selected_scene_internal_id: None,
            lockout: false,
            scene_configs: Vec::new(),
            settings: AppSettings::default(),
            cued_scene_internal_id: None,
            show_file_path: None,
            show_file_dirty: false,
            show_file_last_saved_at: None,
            logs: VecDeque::new(),
            next_log_id: 1,
            last_event_at: None,
        }
    }

    pub fn apply_settings(&mut self, settings: AppSettings) {
        self.settings = settings;
    }

    pub fn set_active_generation(&mut self, generation: u64) {
        self.active_generation = generation;
    }

    pub fn is_active_generation(&self, generation: u64) -> bool {
        self.active_generation == generation
    }

    pub fn apply_show_state(&mut self, state: ShowProjectionState) {
        self.lockout = state.lockout;
        self.show_file_path = state.show_file_path;
        self.show_file_dirty = state.show_file_dirty;
        self.show_file_last_saved_at = state.show_file_last_saved_at;
        self.discovered_lv1_systems = state.discovered_lv1_systems;
        self.connected_lv1_identity = state.connected_lv1_identity;
        self.pending_lv1_identity = state.pending_lv1_identity;
        self.reconnect_state = state.reconnect;
        self.last_event_at = state.last_event_at;
    }

    pub fn apply_scenes_state(&mut self, state: ScenesProjectionState) {
        self.scene_configs = state.scene_configs;
        self.cued_scene_internal_id = state.cued_scene_internal_id;
        self.selected_scene_internal_id = state.selected_scene_internal_id;
    }

    pub fn apply_lv1_event(&mut self, generation: u64, event: &Lv1Event) -> bool {
        if generation != self.active_generation {
            return false;
        }
        match event {
            Lv1Event::Connected => {
                self.ensure_lv1_snapshot().connection = ConnectionStatus::Connected;
            }
            Lv1Event::Disconnected { .. } => {
                self.lv1_snapshot = None;
            }
            Lv1Event::SceneChanged(scene) => {
                self.ensure_lv1_snapshot().scene = Some(scene.clone());
            }
            Lv1Event::SceneListChanged(scene_list) => {
                self.ensure_lv1_snapshot().scene_list = scene_list.clone();
            }
            Lv1Event::FaderChanged {
                group,
                channel,
                gain_db,
            } => {
                self.update_channel(*group, *channel, |channel_info| {
                    channel_info.gain_db = *gain_db
                });
            }
            Lv1Event::MuteChanged {
                group,
                channel,
                muted,
            } => {
                self.update_channel(*group, *channel, |channel_info| channel_info.muted = *muted);
            }
            Lv1Event::PanChanged {
                group,
                channel,
                pan,
            } => {
                self.update_channel(*group, *channel, |channel_info| {
                    channel_info.pan = Some(*pan)
                });
            }
            Lv1Event::BalanceChanged {
                group,
                channel,
                balance,
            } => {
                self.update_channel(*group, *channel, |channel_info| {
                    channel_info.balance = Some(*balance)
                });
            }
            Lv1Event::WidthChanged {
                group,
                channel,
                width,
            } => {
                self.update_channel(*group, *channel, |channel_info| {
                    channel_info.width = Some(*width)
                });
            }
            Lv1Event::ChannelTopologyChanged(channels) => {
                self.ensure_lv1_snapshot().channels = channels.clone();
            }
        }
        true
    }

    pub fn apply_fade_event(&mut self, generation: u64, event: &FadeEvent) -> bool {
        if generation != self.active_generation {
            return false;
        }
        match event {
            FadeEvent::FadeStarted => self.fade_state = AppFadeState::Running,
            FadeEvent::FadeCompleted | FadeEvent::FadeAborted => {
                self.fade_state = AppFadeState::Idle
            }
            FadeEvent::ChannelCompleted { .. } | FadeEvent::ChannelCancelled { .. } => {}
            FadeEvent::ChannelOverride { .. } => self.fade_state = AppFadeState::Blocked,
            FadeEvent::WriteFailed { .. } => {}
        }
        true
    }

    pub fn append_log(&mut self, event: UiLogEvent) {
        let entry = AppLogEntry {
            id: self.next_log_id,
            timestamp: crate::time::current_timestamp_millis(),
            severity: event.severity,
            message: event.message,
        };
        self.next_log_id = self.next_log_id.saturating_add(1);
        self.logs.push_back(entry);
        while self.logs.len() > MAX_PROJECTOR_LOGS {
            self.logs.pop_front();
        }
    }

    pub fn seed_from_view_state(&mut self, snapshot: &AppViewState) {
        self.active_generation = snapshot.state_version;
        self.state_version = snapshot.state_version;
        self.lv1_snapshot = match snapshot.connection {
            AppConnectionState::Disconnected => None,
            AppConnectionState::Connecting | AppConnectionState::Connected => {
                Some(Lv1StateSnapshot {
                    connection: match snapshot.connection {
                        AppConnectionState::Disconnected => ConnectionStatus::Disconnected,
                        AppConnectionState::Connecting => ConnectionStatus::Connecting,
                        AppConnectionState::Connected => ConnectionStatus::Connected,
                    },
                    scene: snapshot.current_scene.as_ref().map(|scene| SceneState {
                        index: scene.index,
                        name: scene.name.clone(),
                    }),
                    scene_list: snapshot
                        .scenes
                        .iter()
                        .map(|scene| SceneListEntry {
                            index: scene.index,
                            name: scene.name.clone(),
                        })
                        .collect(),
                    channels: snapshot
                        .channels
                        .iter()
                        .map(|channel| ChannelInfo {
                            group: channel.group,
                            channel: channel.channel,
                            name: channel.name.clone(),
                            gain_db: 0.0,
                            muted: false,
                            pan: None,
                            balance: None,
                            width: None,
                            pan_mode: None,
                        })
                        .collect(),
                })
            }
        };
        self.discovered_lv1_systems = snapshot.discovered_lv1_systems.clone();
        self.connected_lv1_identity = snapshot.connected_lv1_identity.clone();
        self.pending_lv1_identity = snapshot.pending_lv1_identity.clone();
        self.reconnect_state = snapshot.reconnect.clone();
        self.fade_state = snapshot.fade_state.clone();
        self.selected_scene_internal_id = snapshot.selected_scene_internal_id.clone();
        self.show_file_path = snapshot.show_file_path.as_ref().map(PathBuf::from);
        self.show_file_dirty = snapshot.show_file_dirty;
        self.show_file_last_saved_at = snapshot.show_file_last_saved_at.clone();
        self.settings = snapshot.settings.clone();
        self.logs = snapshot.logs.iter().cloned().collect();
        self.next_log_id = snapshot
            .logs
            .iter()
            .map(|entry| entry.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.last_event_at = snapshot.last_event_at.clone();
    }

    pub fn build_snapshot(&mut self) -> AppViewState {
        self.state_version = self.state_version.saturating_add(1);
        let state_version = self.state_version;

        let (connection, current_scene, scenes, channels) = self
            .lv1_snapshot
            .as_ref()
            .map(|snapshot| {
                let connection = match snapshot.connection {
                    ConnectionStatus::Connecting => AppConnectionState::Connecting,
                    ConnectionStatus::Connected => AppConnectionState::Connected,
                    ConnectionStatus::Disconnected => AppConnectionState::Disconnected,
                };
                let current_scene = snapshot.scene.as_ref().map(|scene| SceneSummary {
                    index: scene.index,
                    name: scene.name.clone(),
                });
                let scenes = snapshot
                    .scene_list
                    .iter()
                    .map(|scene| SceneSummary {
                        index: scene.index,
                        name: scene.name.clone(),
                    })
                    .collect::<Vec<_>>();
                let channels = snapshot
                    .channels
                    .iter()
                    .map(|channel| ChannelSummary {
                        group: channel.group,
                        channel: channel.channel,
                        name: channel.name.clone(),
                    })
                    .collect::<Vec<_>>();
                (connection, current_scene, scenes, channels)
            })
            .unwrap_or((
                AppConnectionState::Disconnected,
                None,
                Vec::new(),
                Vec::new(),
            ));

        AppViewState {
            connection,
            discovered_lv1_systems: self.discovered_lv1_systems.clone(),
            connected_lv1_identity: self.connected_lv1_identity.clone(),
            pending_lv1_identity: self.pending_lv1_identity.clone(),
            reconnect: self.reconnect_state.clone(),
            current_scene,
            scenes: scenes.clone(),
            scene_count: scenes.len(),
            channel_count: channels.len(),
            channels,
            fade_state: self.fade_state.clone(),
            lockout: self.lockout,
            scene_configs: self.scene_configs.clone(),
            cued_scene_internal_id: self.cued_scene_internal_id.clone(),
            selected_scene_internal_id: self.selected_scene_internal_id.clone(),
            show_file_name: self
                .show_file_path
                .as_ref()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
                .unwrap_or_else(|| "Untitled Session".to_string()),
            show_file_path: self
                .show_file_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            show_file_dirty: self.show_file_dirty,
            show_file_last_saved_at: self.show_file_last_saved_at.clone(),
            settings: self.settings.clone(),
            logs: self.logs.iter().cloned().collect(),
            last_event_at: self.last_event_at.clone(),
            state_version,
        }
    }

    fn ensure_lv1_snapshot(&mut self) -> &mut Lv1StateSnapshot {
        self.lv1_snapshot.get_or_insert_with(|| Lv1StateSnapshot {
            connection: ConnectionStatus::Disconnected,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
        })
    }

    fn update_channel(&mut self, group: i32, channel: i32, apply: impl FnOnce(&mut ChannelInfo)) {
        if let Some(existing) = self
            .ensure_lv1_snapshot()
            .channels
            .iter_mut()
            .find(|existing| existing.group == group && existing.channel == channel)
        {
            apply(existing);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::FadeParameter;
    use crate::lv1::{ChannelInfo, SceneState};
    use crate::projector::LogSeverity;
    use crate::settings::AppSettings;
    #[test]
    fn cache_builds_initial_disconnected_snapshot_with_incrementing_versions() {
        let mut cache = ProjectionCache::new();

        let first = cache.build_snapshot();
        let second = cache.build_snapshot();

        assert_eq!(first.connection, AppConnectionState::Disconnected);
        assert_eq!(first.show_file_name, "Untitled Session");
        assert_eq!(first.state_version, 1);
        assert_eq!(second.state_version, 2);
    }

    #[test]
    fn cache_applies_lv1_scene_and_topology_events() {
        let mut cache = ProjectionCache::new();

        cache.apply_lv1_event(0, &Lv1Event::Connected);
        cache.apply_lv1_event(
            0,
            &Lv1Event::SceneChanged(SceneState {
                index: 3,
                name: "Bridge".to_string(),
            }),
        );
        cache.apply_lv1_event(
            0,
            &Lv1Event::ChannelTopologyChanged(vec![ChannelInfo {
                group: 1,
                channel: 2,
                name: "Vox".to_string(),
                gain_db: -5.0,
                muted: false,
                pan: Some(0.0),
                balance: None,
                width: None,
                pan_mode: None,
            }]),
        );

        let snapshot = cache.build_snapshot();

        assert_eq!(snapshot.connection, AppConnectionState::Connected);
        assert_eq!(snapshot.current_scene.unwrap().name, "Bridge");
        assert_eq!(snapshot.channel_count, 1);
        assert_eq!(snapshot.channels[0].name, "Vox");
    }

    #[test]
    fn cache_seeds_from_connected_view_state() {
        let mut cache = ProjectionCache::new();
        cache.seed_from_view_state(&AppViewState {
            connection: AppConnectionState::Connected,
            discovered_lv1_systems: Vec::new(),
            connected_lv1_identity: None,
            pending_lv1_identity: None,
            reconnect: ReconnectState::default(),
            current_scene: Some(SceneSummary {
                index: 1,
                name: "Intro".to_string(),
            }),
            scenes: vec![SceneSummary {
                index: 1,
                name: "Intro".to_string(),
            }],
            scene_count: 1,
            channel_count: 0,
            channels: Vec::new(),
            fade_state: AppFadeState::Idle,
            lockout: false,
            scene_configs: Vec::new(),
            settings: AppSettings::default(),
            cued_scene_internal_id: None,
            selected_scene_internal_id: None,
            show_file_name: "Untitled Session".to_string(),
            show_file_path: None,
            show_file_dirty: false,
            show_file_last_saved_at: None,
            logs: vec![AppLogEntry {
                id: 7,
                timestamp: "2026-01-01T00:00:00.000Z".to_string(),
                severity: LogSeverity::Info,
                message: "seed log".to_string(),
            }],
            last_event_at: None,
            state_version: 11,
        });

        cache.append_log(UiLogEvent {
            severity: LogSeverity::Warning,
            message: "projected log".to_string(),
        });
        let snapshot = cache.build_snapshot();

        assert_eq!(snapshot.connection, AppConnectionState::Connected);
        assert_eq!(snapshot.current_scene.unwrap().name, "Intro");
        assert_eq!(snapshot.scenes.len(), 1);
        assert_eq!(snapshot.logs[0].id, 7);
        assert_eq!(snapshot.logs[1].id, 8);
    }

    #[test]
    fn cache_clears_lv1_snapshot_on_disconnect() {
        let mut cache = ProjectionCache::new();

        cache.connected_lv1_identity = Some(Lv1SystemIdentity {
            uuid: Some("connected-uuid".to_string()),
            host: Some("lv1.local".to_string()),
            address: "192.0.2.10".to_string(),
            port: 7788,
        });
        cache.pending_lv1_identity = Some(Lv1SystemIdentity {
            uuid: Some("pending-uuid".to_string()),
            host: Some("pending.local".to_string()),
            address: "192.0.2.11".to_string(),
            port: 7788,
        });
        cache.reconnect_state = ReconnectState {
            active: true,
            attempt: 42,
        };

        cache.apply_lv1_event(
            0,
            &Lv1Event::Disconnected {
                reason: "link lost".to_string(),
            },
        );

        let snapshot = cache.build_snapshot();

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert!(snapshot.current_scene.is_none());
        assert_eq!(snapshot.scenes.len(), 0);
    }

    #[test]
    fn lv1_disconnect_does_not_clear_show_owned_connection_metadata() {
        let mut cache = ProjectionCache::new();

        let connected_identity = Lv1SystemIdentity {
            uuid: Some("connected-uuid".to_string()),
            host: Some("lv1.local".to_string()),
            address: "192.0.2.10".to_string(),
            port: 7788,
        };
        let pending_identity = Lv1SystemIdentity {
            uuid: Some("pending-uuid".to_string()),
            host: Some("pending.local".to_string()),
            address: "192.0.2.11".to_string(),
            port: 7788,
        };
        let reconnect = ReconnectState {
            active: true,
            attempt: 42,
        };

        cache.apply_show_state(ShowProjectionState {
            lockout: false,
            show_file_path: None,
            show_file_name: "Untitled Session".to_string(),
            show_file_dirty: false,
            show_file_last_saved_at: None,
            discovered_lv1_systems: Vec::new(),
            connected_lv1_identity: Some(connected_identity.clone()),
            pending_lv1_identity: Some(pending_identity.clone()),
            reconnect: reconnect.clone(),
            last_event_at: None,
        });

        let changed = cache.apply_lv1_event(
            0,
            &Lv1Event::Disconnected {
                reason: "link lost".to_string(),
            },
        );

        let snapshot = cache.build_snapshot();

        assert!(changed);
        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert_eq!(snapshot.connected_lv1_identity, Some(connected_identity));
        assert_eq!(snapshot.pending_lv1_identity, Some(pending_identity));
        assert_eq!(snapshot.reconnect, reconnect);
    }

    #[test]
    fn cache_applies_scenes_projection_state_separately_from_show_state() {
        let mut cache = ProjectionCache::new();

        cache.apply_show_state(ShowProjectionState {
            lockout: true,
            show_file_path: None,
            show_file_name: "Untitled Session".to_string(),
            show_file_dirty: false,
            show_file_last_saved_at: None,
            discovered_lv1_systems: Vec::new(),
            connected_lv1_identity: None,
            pending_lv1_identity: None,
            reconnect: ReconnectState::default(),
            last_event_at: None,
        });
        cache.apply_scenes_state(crate::scenes::ScenesProjectionState {
            scene_configs: vec![crate::scenes::SceneConfig {
                internal_scene_id: uuid::Uuid::from_u128(0x11111111111141118111111111111111),
                scene_index: Some(5),
                scene_name: "Verse".to_string(),
                duration_ms: 1500,
                channel_configs: vec![],
                scoped_channels: vec![],
                scope_toggles: Default::default(),
            }],
            cued_scene_internal_id: Some("cue-id".to_string()),
            selected_scene_internal_id: Some("selected-id".to_string()),
        });

        let snapshot = cache.build_snapshot();

        assert!(snapshot.lockout);
        assert_eq!(snapshot.scene_configs.len(), 1);
        assert_eq!(snapshot.cued_scene_internal_id.as_deref(), Some("cue-id"));
        assert_eq!(
            snapshot.selected_scene_internal_id.as_deref(),
            Some("selected-id")
        );
    }

    #[test]
    fn cache_applies_fade_state_events() {
        let mut cache = ProjectionCache::new();

        assert!(cache.apply_fade_event(0, &FadeEvent::FadeStarted));
        assert_eq!(cache.build_snapshot().fade_state, AppFadeState::Running);

        assert!(cache.apply_fade_event(
            0,
            &FadeEvent::ChannelOverride {
                group: 1,
                channel: 1,
                parameter: FadeParameter::FaderDb,
            }
        ));
        assert_eq!(cache.build_snapshot().fade_state, AppFadeState::Blocked);

        assert!(cache.apply_fade_event(0, &FadeEvent::FadeCompleted));
        assert_eq!(cache.build_snapshot().fade_state, AppFadeState::Idle);
    }

    #[test]
    fn cache_ignores_stale_generation_fade_events() {
        let mut cache = ProjectionCache::new();
        cache.set_active_generation(2);

        assert!(!cache.apply_fade_event(1, &FadeEvent::FadeStarted));

        assert_eq!(cache.build_snapshot().fade_state, AppFadeState::Idle);
    }

    #[test]
    fn cache_keeps_fade_state_when_channel_cancelled() {
        let mut cache = ProjectionCache::new();

        cache.apply_fade_event(0, &FadeEvent::FadeStarted);
        cache.apply_fade_event(
            0,
            &FadeEvent::ChannelCancelled {
                group: 1,
                channel: 1,
                parameter: FadeParameter::FaderDb,
            },
        );

        assert_eq!(cache.build_snapshot().fade_state, AppFadeState::Running);
    }

    #[test]
    fn cache_owns_bounded_log_entries() {
        let mut cache = ProjectionCache::new();

        for index in 0..(MAX_PROJECTOR_LOGS + 2) {
            cache.append_log(UiLogEvent {
                severity: LogSeverity::Info,
                message: format!("log {index}"),
            });
        }

        let snapshot = cache.build_snapshot();

        assert_eq!(snapshot.logs.len(), MAX_PROJECTOR_LOGS);
        assert_eq!(snapshot.logs[0].id, 3);
        assert_eq!(snapshot.logs.last().unwrap().message, "log 201");
    }
}
